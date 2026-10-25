//! Smoke tests of the built binary on a real filesystem: the exit codes
//! and output a shell sees, the process setup in `main`, and a real
//! virtualenv walk. Behavior that needs neither the binary nor a real
//! filesystem is tested in-process.

use std::fs;
use std::path::Path;
use std::time::Duration;

use assert_cmd::Command;
use tempfile::TempDir;

fn write(root: &Path, rel: &str, contents: &[u8]) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn project(files: &[(&str, &[u8])]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (rel, contents) in files {
        write(dir.path(), rel, contents);
    }
    dir
}

fn cmd(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("pyproject-udeps").unwrap();
    cmd.current_dir(dir.path());
    cmd
}

const MANIFEST: &[u8] = br#"
[tool.poetry.dependencies]
python = "^3.11"
zulu = "^1.0"
requests = "^2.31"
"#;

#[test]
fn used_dependencies_exit_0_with_no_output() {
    let dir = project(&[
        ("pyproject.toml", MANIFEST),
        ("main.py", b"import requests, zulu\n"),
    ]);
    cmd(&dir).assert().code(0).stdout("").stderr("");
}

#[test]
fn unused_dependencies_exit_1_sorted_one_per_line() {
    let dir = project(&[("pyproject.toml", MANIFEST), ("main.py", b"import os\n")]);
    cmd(&dir).assert().code(1).stdout("requests\nzulu\n");
}

#[test]
fn missing_pyproject_exits_2_with_an_error() {
    let dir = project(&[]);
    let assert = cmd(&dir).assert().code(2).stdout("");
    assert_ne!(assert.get_output().stderr, [] as [u8; 0]);
}

#[test]
fn deeply_nested_python_is_parsed() {
    // 3000 nested operators: valid Python that overflows a 2 MiB thread
    // stack in the parser.
    let source = format!("x = {}1\nimport requests, zulu\n", "not ".repeat(3000));
    let dir = project(&[("pyproject.toml", MANIFEST), ("main.py", source.as_bytes())]);
    cmd(&dir).assert().code(0).stdout("");
}

#[test]
fn verbose_logs_from_the_walk_threads_do_not_hang() {
    // Each file with a syntax error logs from a walk thread at -vvv.
    let names: Vec<String> = (0..50).map(|i| format!("broken{i}.py")).collect();
    let mut files: Vec<(&str, &[u8])> = vec![("pyproject.toml", MANIFEST)];
    files.extend(names.iter().map(|name| {
        (
            name.as_str(),
            b"import requests, zulu\ndef broken(:\n" as &[u8],
        )
    }));
    let dir = project(&files);
    cmd(&dir)
        .arg("-vvv")
        .timeout(Duration::from_secs(30))
        .assert()
        .code(0);
}

#[test]
fn virtualenv_flag_walks_a_uv_projects_dot_venv() {
    let dir = project(&[
        (
            "pyproject.toml",
            br#"
[project]
name = "demo"
version = "0.1.0"
dependencies = ["requests>=2.31"]
"#,
        ),
        ("uv.lock", b""),
        ("main.py", b"import os\n"),
        (
            ".venv/lib/python3.12/site-packages/othertool/util.py",
            b"import requests\n",
        ),
    ]);
    // Without -e, requests looks unused; with -e, the venv walk finds it.
    cmd(&dir).assert().code(1).stdout("requests\n");
    cmd(&dir).arg("-e").assert().code(0).stdout("");
}
