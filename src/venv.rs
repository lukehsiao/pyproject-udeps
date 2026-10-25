//! Virtualenv discovery, and the installed files in a virtualenv.
//!
//! Which tool manages the environment decides where to look. Lockfiles are
//! the strongest signal, since a lockfile proves which tool actually
//! manages the environment; the flavor the manifest suggests is the
//! tiebreak. Whatever the flavor, discovery returns the path the flavor
//! reports if it exists, else the project's `.venv` if it exists, else an
//! error.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, anyhow};
use tracing::debug;

use crate::infra::env::Env;
use crate::infra::fs::{FileSystem, is_python_file};
use crate::infra::process::CommandRunner;
use crate::package::PackageName;

/// The tool that manages a project's environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    Poetry,
    Uv,
    Pep621,
}

/// Locate the project's virtualenv directory. `suggested` is the flavor
/// the manifest suggests, used when the lockfiles do not decide.
///
/// # Errors
///
/// Fails when no existing virtualenv can be found for the project, with a
/// flavor-appropriate suggestion.
pub fn find_venv(
    fs: &FileSystem,
    runner: &CommandRunner,
    env: &Env,
    suggested: Flavor,
) -> Result<PathBuf> {
    let flavor = detect_flavor(fs, suggested);
    debug!(?flavor, "discovering virtualenv");
    let (reported, hint) = match flavor {
        Flavor::Poetry => (
            poetry_reported_venv(runner),
            "install the project with `poetry install`",
        ),
        Flavor::Uv => (
            env_var_path(env, "UV_PROJECT_ENVIRONMENT"),
            "run `uv sync` to create one",
        ),
        Flavor::Pep621 => (
            env_var_path(env, "VIRTUAL_ENV"),
            "activate one or create .venv in the project root",
        ),
    };
    let err = match reported {
        Ok(path) if fs.exists(&path) => return Ok(path),
        Ok(path) => anyhow!("{} does not exist", path.display()),
        Err(err) => err,
    };
    let local = PathBuf::from(".venv");
    if fs.exists(&local) {
        debug!(%err, "using .venv");
        return Ok(local);
    }
    Err(anyhow!("{err:#}, and .venv does not exist")
        .context(format!("could not locate a virtualenv; {hint}")))
}

fn detect_flavor(fs: &FileSystem, suggested: Flavor) -> Flavor {
    let uv_lock = fs.exists(Path::new("uv.lock"));
    let poetry_lock = fs.exists(Path::new("poetry.lock"));
    match (uv_lock, poetry_lock) {
        (true, false) => Flavor::Uv,
        (false, true) => Flavor::Poetry,
        _ => suggested,
    }
}

fn poetry_reported_venv(runner: &CommandRunner) -> Result<PathBuf> {
    let path = runner.run("poetry", &["env", "info", "-p"])?;
    // Some poetry 1.x versions print nothing and exit 0 when no venv
    // exists, so empty output is a miss, not a venv at "".
    if path.is_empty() {
        return Err(anyhow!("poetry reported no virtualenv path"));
    }
    Ok(PathBuf::from(path))
}

fn env_var_path(env: &Env, name: &str) -> Result<PathBuf> {
    env.var(name)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("${name} is not set"))
}

/// The package that installed each Python file in the virtualenv at
/// `venv`, from the `RECORD` file of every `*.dist-info` directory in its
/// `site-packages`.
///
/// A distribution without a readable `RECORD` owns no files here, so its
/// files count as any other file does.
pub fn installed_owners(fs: &FileSystem, venv: &Path) -> HashMap<PathBuf, PackageName> {
    let mut owners = HashMap::new();
    for site in site_packages(fs, venv) {
        for dist_info in fs.read_dir(&site).unwrap_or_default() {
            let Some(name) = dist_info_name(&dist_info) else {
                continue;
            };
            let record = match fs.read_to_string_lossy(&dist_info.join("RECORD")) {
                Ok(record) => record,
                Err(err) => {
                    debug!(%err, dist_info = %dist_info.display(), "no RECORD");
                    continue;
                }
            };
            // `RECORD` is a CSV file whose columns are path, hash, and
            // size. CSV quotes a path only when it holds a special
            // character such as `,` or `"`, and no module path has those.
            // A quoted row gives a path that starts with `"`, which no
            // walked file has, so its file counts as any other file does.
            for row in record.lines() {
                let path = row.split_once(',').map_or(row, |(path, _)| path);
                if is_python_file(Path::new(path)) {
                    owners.insert(resolve(&site, path), name.clone());
                }
            }
        }
    }
    owners
}

/// The `site-packages` directories of the virtualenv at `venv`:
/// `lib/*/site-packages`, such as `lib/python3.14` or `lib/pypy3.10`, or
/// `Lib/site-packages` on Windows. A directory that does not exist lists
/// no entries.
///
/// Virtualenvs make `lib64` a link to `lib`, and the walk does not follow
/// links, so `lib64` would only repeat files the walk never sends.
fn site_packages(fs: &FileSystem, venv: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs
        .read_dir(&venv.join("lib"))
        .unwrap_or_default()
        .into_iter()
        .map(|dir| dir.join("site-packages"))
        .collect();
    dirs.push(venv.join("Lib").join("site-packages"));
    dirs
}

/// `site` joined with the `RECORD` path `relative`. Each `..` removes the
/// last component, since walk paths contain no `..`. Scripts in `bin/`
/// have rows such as `../../../bin/tool.py`, and the walk visits them.
fn resolve(site: &Path, relative: &str) -> PathBuf {
    let mut path = site.to_path_buf();
    for component in Path::new(relative).components() {
        if component == Component::ParentDir {
            path.pop();
        } else {
            path.push(component);
        }
    }
    path
}

/// The package a `{name}-{version}.dist-info` directory belongs to. The
/// wheel format replaces each `-` in the name with `_`, so the first `-`
/// ends the name.
fn dist_info_name(dir: &Path) -> Option<PackageName> {
    let stem = dir.file_name()?.to_str()?.strip_suffix(".dist-info")?;
    let (name, _version) = stem.split_once('-')?;
    Some(PackageName::new(name))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::infra::process::{Answer, Execution};
    use pretty_assertions::assert_eq;

    fn no_commands() -> CommandRunner {
        CommandRunner::create_null([])
    }

    fn poetry_env_info(answer: Answer) -> CommandRunner {
        CommandRunner::create_null([(Execution::new("poetry", &["env", "info", "-p"]), answer)])
    }

    fn no_env() -> Env {
        Env::create_null([] as [(&str, &str); 0])
    }

    #[test]
    fn installed_owners_come_from_each_record() {
        // PyPy names the directory under `lib` pypy3.X, not python3.X.
        let site = "/venvs/demo/lib/pypy3.10/site-packages";
        let fs = FileSystem::create_null([
            (
                format!("{site}/pyyaml-6.0.3.dist-info/RECORD"),
                "_yaml/__init__.py,sha256=x,1\n\
                 yaml/__init__.py,sha256=x,1\n\
                 yaml/__init__.pyi,sha256=x,1\n\
                 yaml/_yaml.cpython-314-x86_64-linux-gnu.so,sha256=x,1\n\
                 ../../../bin/yaml-tool,sha256=x,1\n\
                 ../../../bin/yaml_tool.py,sha256=x,1\n\
                 pyyaml-6.0.3.dist-info/RECORD,,\n"
                    .to_owned(),
            ),
            (
                format!("{site}/opentelemetry_instrumentation_logging-0.65b0.dist-info/RECORD"),
                "opentelemetry/instrumentation/logging/__init__.py,sha256=x,1\n".to_owned(),
            ),
            (
                "/venvs/demo/Lib/site-packages/humanize-4.0.dist-info/RECORD".to_owned(),
                "humanize/__init__.py,,\n../../Scripts/humanize_tool.py,,\n".to_owned(),
            ),
            // No RECORD, as with some old installers.
            (
                format!("{site}/legacy-1.0.dist-info/METADATA"),
                String::new(),
            ),
            (format!("{site}/yaml/__init__.py"), String::new()),
        ]);
        let owners = installed_owners(&fs, Path::new("/venvs/demo"));
        let mut owners: Vec<(String, &str)> = owners
            .iter()
            .map(|(path, name)| (path.display().to_string(), name.as_str()))
            .collect();
        owners.sort();
        assert_eq!(
            owners,
            vec![
                (
                    "/venvs/demo/Lib/site-packages/humanize/__init__.py".to_owned(),
                    "humanize"
                ),
                (
                    "/venvs/demo/Scripts/humanize_tool.py".to_owned(),
                    "humanize"
                ),
                ("/venvs/demo/bin/yaml_tool.py".to_owned(), "pyyaml"),
                (format!("{site}/_yaml/__init__.py"), "pyyaml"),
                (
                    format!("{site}/opentelemetry/instrumentation/logging/__init__.py"),
                    "opentelemetry-instrumentation-logging"
                ),
                (format!("{site}/yaml/__init__.py"), "pyyaml"),
                (format!("{site}/yaml/__init__.pyi"), "pyyaml"),
            ]
        );
    }

    #[test]
    fn poetry_project_asks_poetry() {
        let fs =
            FileSystem::create_null([("poetry.lock", ""), ("/venvs/demo-py3.12/lib/site.py", "")]);
        let runner = poetry_env_info(Answer::stdout("/venvs/demo-py3.12\n"));
        let venv = find_venv(&fs, &runner, &no_env(), Flavor::Poetry).unwrap();
        assert_eq!(venv, PathBuf::from("/venvs/demo-py3.12"));
    }

    #[test]
    fn poetry_failure_falls_back_to_local_venv() {
        let fs = FileSystem::create_null([("poetry.lock", ""), (".venv/lib/site.py", "")]);
        let venv = find_venv(&fs, &no_commands(), &no_env(), Flavor::Poetry).unwrap();
        assert_eq!(venv, PathBuf::from(".venv"));
    }

    #[test]
    fn empty_poetry_output_falls_back_to_local_venv() {
        let fs = FileSystem::create_null([("poetry.lock", ""), (".venv/lib/site.py", "")]);
        let runner = poetry_env_info(Answer::stdout(""));
        let venv = find_venv(&fs, &runner, &no_env(), Flavor::Poetry).unwrap();
        assert_eq!(venv, PathBuf::from(".venv"));
    }

    #[test]
    fn stale_poetry_path_falls_back_to_local_venv() {
        let fs = FileSystem::create_null([("poetry.lock", ""), (".venv/lib/site.py", "")]);
        let runner = poetry_env_info(Answer::stdout("/venvs/gone\n"));
        let venv = find_venv(&fs, &runner, &no_env(), Flavor::Poetry).unwrap();
        assert_eq!(venv, PathBuf::from(".venv"));
    }

    #[test]
    fn stale_poetry_path_without_local_venv_names_the_path() {
        let fs = FileSystem::create_null([("poetry.lock", "")]);
        let runner = poetry_env_info(Answer::stdout("/venvs/gone\n"));
        let err = find_venv(&fs, &runner, &no_env(), Flavor::Poetry).unwrap_err();
        let err = format!("{err:#}");
        assert!(err.contains("poetry install"), "{err}");
        assert!(err.contains("/venvs/gone does not exist"), "{err}");
    }

    #[test]
    fn poetry_failure_without_local_venv_is_an_error() {
        let fs = FileSystem::create_null([("poetry.lock", "")]);
        let err = find_venv(&fs, &no_commands(), &no_env(), Flavor::Poetry).unwrap_err();
        assert!(err.to_string().contains("poetry install"), "{err}");
    }

    #[test]
    fn uv_project_never_invokes_a_subprocess() {
        let fs = FileSystem::create_null([("uv.lock", ""), (".venv/lib/site.py", "")]);
        let runner = no_commands();
        let runs = runner.track_runs();
        let venv = find_venv(&fs, &runner, &no_env(), Flavor::Uv).unwrap();
        assert_eq!(venv, PathBuf::from(".venv"));
        assert_eq!(runs.data(), vec![]);
    }

    #[test]
    fn uv_project_environment_variable_takes_precedence() {
        let fs = FileSystem::create_null([
            ("uv.lock", ""),
            (".venv/lib/site.py", ""),
            ("/custom/env/lib/site.py", ""),
        ]);
        let env = Env::create_null([("UV_PROJECT_ENVIRONMENT", "/custom/env")]);
        let venv = find_venv(&fs, &no_commands(), &env, Flavor::Uv).unwrap();
        assert_eq!(venv, PathBuf::from("/custom/env"));
    }

    #[test]
    fn nonexistent_uv_project_environment_falls_back_to_local_venv() {
        let fs = FileSystem::create_null([("uv.lock", ""), (".venv/lib/site.py", "")]);
        let env = Env::create_null([("UV_PROJECT_ENVIRONMENT", "/ghost")]);
        let venv = find_venv(&fs, &no_commands(), &env, Flavor::Uv).unwrap();
        assert_eq!(venv, PathBuf::from(".venv"));
    }

    #[test]
    fn uv_project_without_venv_names_both_misses() {
        let fs = FileSystem::create_null([("uv.lock", "")]);
        let err = find_venv(&fs, &no_commands(), &no_env(), Flavor::Uv).unwrap_err();
        assert_eq!(
            format!("{err:#}"),
            "could not locate a virtualenv; run `uv sync` to create one: \
             $UV_PROJECT_ENVIRONMENT is not set, and .venv does not exist"
        );
    }

    #[test]
    fn uv_lock_beats_a_suggested_poetry_flavor() {
        // A lockfile proves which tool manages the environment.
        let fs = FileSystem::create_null([("uv.lock", ""), (".venv/lib/site.py", "")]);
        let runner = no_commands();
        let runs = runner.track_runs();
        let venv = find_venv(&fs, &runner, &no_env(), Flavor::Poetry).unwrap();
        assert_eq!(venv, PathBuf::from(".venv"));
        assert_eq!(runs.data(), vec![]);
    }

    #[test]
    fn suggested_flavor_decides_without_lockfiles() {
        let fs = FileSystem::create_null([("/custom/env/lib/site.py", "")]);
        let env = Env::create_null([
            ("UV_PROJECT_ENVIRONMENT", "/custom/env"),
            ("VIRTUAL_ENV", "/active/venv"),
        ]);
        let venv = find_venv(&fs, &no_commands(), &env, Flavor::Uv).unwrap();
        assert_eq!(venv, PathBuf::from("/custom/env"));
    }

    #[test]
    fn pep621_project_uses_the_active_virtualenv() {
        let fs = FileSystem::create_null([("/active/venv/lib/site.py", "")]);
        let env = Env::create_null([("VIRTUAL_ENV", "/active/venv")]);
        let venv = find_venv(&fs, &no_commands(), &env, Flavor::Pep621).unwrap();
        assert_eq!(venv, PathBuf::from("/active/venv"));
    }

    #[test]
    fn pep621_project_falls_back_to_local_venv_then_errors() {
        let fs = FileSystem::create_null([(".venv/lib/site.py", "")]);
        let venv = find_venv(&fs, &no_commands(), &no_env(), Flavor::Pep621).unwrap();
        assert_eq!(venv, PathBuf::from(".venv"));

        let fs = FileSystem::create_null([] as [(&str, &str); 0]);
        let err = find_venv(&fs, &no_commands(), &no_env(), Flavor::Pep621).unwrap_err();
        assert!(err.to_string().contains("activate one"), "{err}");
    }
}
