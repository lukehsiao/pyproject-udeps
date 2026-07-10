//! End-to-end tests of the CLI process contract: exit codes, stdout, and
//! flag behavior. These pin observable behavior so refactors can prove they
//! changed nothing (or exactly what they meant to).

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use tempfile::TempDir;

fn write(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn project(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (rel, contents) in files {
        write(dir.path(), rel, contents);
    }
    dir
}

fn cmd(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("poetry-udeps").unwrap();
    cmd.current_dir(dir.path());
    cmd
}

const POETRY_MANIFEST: &str = r#"
[tool.poetry.dependencies]
python = "^3.11"
requests = "^2.31"
"#;

#[test]
fn unused_dependency_is_reported_with_exit_code_1() {
    let dir = project(&[
        ("pyproject.toml", POETRY_MANIFEST),
        ("main.py", "import os\n"),
    ]);
    cmd(&dir).assert().code(1).stdout("requests\n");
}

#[test]
fn used_dependency_exits_0_with_no_output() {
    let dir = project(&[
        ("pyproject.toml", POETRY_MANIFEST),
        ("main.py", "import requests\n"),
    ]);
    cmd(&dir).assert().code(0).stdout("");
}

#[test]
fn missing_pyproject_exits_2() {
    let dir = project(&[]);
    let assert = cmd(&dir).assert().code(2);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(!stderr.is_empty(), "expected an error on stderr");
}

#[test]
fn unused_deps_are_reported_sorted_one_per_line() {
    let dir = project(&[
        (
            "pyproject.toml",
            r#"
[tool.poetry.dependencies]
python = "^3.11"
zulu = "^1.0"
alpha = "^1.0"
"#,
        ),
        ("main.py", "import os\n"),
    ]);
    cmd(&dir).assert().code(1).stdout("alpha\nzulu\n");
}

#[test]
fn dev_dependencies_are_only_reported_with_dev_flag() {
    let files = [
        (
            "pyproject.toml",
            r#"
[tool.poetry.dependencies]
python = "^3.11"
requests = "^2.31"

[tool.poetry.group.dev.dependencies]
pytest = "^8.0"
"#,
        ),
        ("main.py", "import requests\n"),
    ];

    let dir = project(&files);
    cmd(&dir).assert().code(0).stdout("");

    let dir = project(&files);
    cmd(&dir).arg("--dev").assert().code(1).stdout("pytest\n");
}

#[test]
fn pep621_project_dependencies_are_parsed() {
    let dir = project(&[
        (
            "pyproject.toml",
            r#"
[project]
name = "demo"
version = "0.1.0"
dependencies = ["requests>=2.31"]
"#,
        ),
        ("main.py", "import os\n"),
    ]);
    cmd(&dir).assert().code(1).stdout("requests\n");
}

#[test]
fn known_name_alias_marks_dependency_used() {
    let dir = project(&[
        (
            "pyproject.toml",
            r#"
[tool.poetry.dependencies]
python = "^3.11"
scikit-learn = "^1.4"
"#,
        ),
        ("main.py", "import sklearn\n"),
    ]);
    cmd(&dir).assert().code(0).stdout("");
}

#[test]
fn dash_to_underscore_alias_marks_dependency_used() {
    let dir = project(&[
        (
            "pyproject.toml",
            r#"
[tool.poetry.dependencies]
python = "^3.11"
python-dateutil = "^2.9"
"#,
        ),
        ("main.py", "import python_dateutil\n"),
    ]);
    cmd(&dir).assert().code(0).stdout("");
}

#[test]
fn aliased_dev_dependency_is_not_reported_when_used() {
    let dir = project(&[
        (
            "pyproject.toml",
            r#"
[tool.poetry.dependencies]
python = "^3.11"

[tool.poetry.group.dev.dependencies]
scikit-learn = "^1.4"
"#,
        ),
        ("main.py", "import sklearn\n"),
    ]);
    cmd(&dir).arg("--dev").assert().code(0).stdout("");
}

#[test]
fn bare_dbt_adapters_import_does_not_crash() {
    let dir = project(&[
        ("pyproject.toml", POETRY_MANIFEST),
        ("main.py", "import dbt.adapters\n"),
    ]);
    cmd(&dir).assert().code(1).stdout("requests\n");
}

#[test]
fn dbt_adapter_import_marks_adapter_package_used() {
    let dir = project(&[
        (
            "pyproject.toml",
            r#"
[tool.poetry.dependencies]
python = "^3.11"
dbt-postgres = "^1.9"
"#,
        ),
        ("main.py", "import dbt.adapters.postgres\n"),
    ]);
    cmd(&dir).assert().code(0).stdout("");
}

#[test]
fn non_utf8_python_file_does_not_crash() {
    let dir = project(&[
        ("pyproject.toml", POETRY_MANIFEST),
        ("main.py", "import requests\n"),
    ]);
    fs::write(dir.path().join("legacy.py"), b"import os\n\xff\xfe garbage\n").unwrap();
    cmd(&dir).assert().code(0).stdout("");
}

#[test]
fn ignorefile_filters_reported_dependencies() {
    let dir = project(&[
        ("pyproject.toml", POETRY_MANIFEST),
        ("main.py", "import os\n"),
        (".poetryudepsignore", "requests\n"),
    ]);
    cmd(&dir).assert().code(0).stdout("");
}

#[test]
fn ignorefile_skips_comments_and_blank_lines() {
    let dir = project(&[
        ("pyproject.toml", POETRY_MANIFEST),
        ("main.py", "import os\n"),
        (".poetryudepsignore", "# a comment\n\nrequests\n"),
    ]);
    cmd(&dir).assert().code(0).stdout("");
}

#[test]
fn no_ignore_flag_bypasses_the_ignorefile() {
    let dir = project(&[
        ("pyproject.toml", POETRY_MANIFEST),
        ("main.py", "import os\n"),
        (".poetryudepsignore", "requests\n"),
    ]);
    cmd(&dir)
        .arg("--no-ignore")
        .assert()
        .code(1)
        .stdout("requests\n");
}
