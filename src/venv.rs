//! Virtualenv discovery.
//!
//! Which tool manages the environment decides where to look, and that
//! flavor is an implementation detail of this module. Lockfiles are the
//! strongest signal, since a lockfile proves which tool actually manages
//! the environment; the manifest's tool tables are the tiebreak.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::debug;

use crate::infra::env::Env;
use crate::infra::fs::FileSystem;
use crate::infra::process::CommandRunner;
use crate::pyproject::Manifest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flavor {
    Poetry,
    Uv,
    Pep621,
}

/// Locate the project's virtualenv directory.
///
/// # Errors
///
/// Fails when no virtualenv can be found for the project, with a
/// flavor-appropriate suggestion.
pub fn find_venv(
    fs: &FileSystem,
    runner: &CommandRunner,
    env: &Env,
    manifest: &Manifest,
) -> Result<PathBuf> {
    let flavor = detect_flavor(fs, manifest);
    debug!(?flavor, "discovering virtualenv");
    match flavor {
        Flavor::Poetry => poetry_venv(fs, runner),
        Flavor::Uv => uv_venv(fs, env),
        Flavor::Pep621 => pep621_venv(fs, env),
    }
}

fn detect_flavor(fs: &FileSystem, manifest: &Manifest) -> Flavor {
    let uv_lock = fs.exists(Path::new("uv.lock"));
    let poetry_lock = fs.exists(Path::new("poetry.lock"));
    match (uv_lock, poetry_lock) {
        (true, false) => Flavor::Uv,
        (false, true) => Flavor::Poetry,
        _ if manifest.poetry_hint() => Flavor::Poetry,
        _ if manifest.uv_hint() => Flavor::Uv,
        _ => Flavor::Pep621,
    }
}

/// The conventional in-project virtualenv, if present.
fn local_venv(fs: &FileSystem) -> Option<PathBuf> {
    let venv = PathBuf::from(".venv");
    fs.exists(&venv).then_some(venv)
}

fn poetry_venv(fs: &FileSystem, runner: &CommandRunner) -> Result<PathBuf> {
    match runner.run("poetry", &["env", "info", "-p"]) {
        Ok(path) => Ok(PathBuf::from(path)),
        Err(err) => match local_venv(fs) {
            Some(venv) => {
                debug!(%err, "poetry env info failed, using .venv");
                Ok(venv)
            }
            None => Err(err.context(
                "could not locate a virtualenv; install the project with `poetry install`",
            )),
        },
    }
}

fn uv_venv(fs: &FileSystem, env: &Env) -> Result<PathBuf> {
    if let Some(configured) = env.var("UV_PROJECT_ENVIRONMENT") {
        let path = PathBuf::from(configured);
        if fs.exists(&path) {
            return Ok(path);
        }
        debug!(path = %path.display(), "UV_PROJECT_ENVIRONMENT does not exist, trying .venv");
    }
    local_venv(fs).context("could not locate a virtualenv; run `uv sync` to create one")
}

fn pep621_venv(fs: &FileSystem, env: &Env) -> Result<PathBuf> {
    if let Some(active) = env.var("VIRTUAL_ENV") {
        let path = PathBuf::from(active);
        if fs.exists(&path) {
            return Ok(path);
        }
        debug!(path = %path.display(), "VIRTUAL_ENV does not exist, trying .venv");
    }
    local_venv(fs)
        .context("could not locate a virtualenv; activate one or create .venv in the project root")
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::infra::process::NullResponse;
    use pretty_assertions::assert_eq;

    const POETRY_MANIFEST: &str = "[tool.poetry.dependencies]\nrequests = \"^2.31\"\n";
    const UV_MANIFEST: &str =
        "[project]\ndependencies = [\"requests\"]\n\n[tool.uv]\ndev-dependencies = []\n";
    const PEP621_MANIFEST: &str = "[project]\ndependencies = [\"requests\"]\n";

    fn manifest(source: &str) -> Manifest {
        Manifest::parse(source).unwrap()
    }

    fn no_commands() -> CommandRunner {
        CommandRunner::create_null([] as [(&str, NullResponse); 0])
    }

    fn no_env() -> Env {
        Env::create_null([] as [(&str, &str); 0])
    }

    #[test]
    fn poetry_project_asks_poetry() {
        let fs = FileSystem::create_null([("poetry.lock", "")]);
        let runner = CommandRunner::create_null([(
            "poetry env info -p",
            NullResponse::Stdout("/venvs/demo-py3.12".into()),
        )]);
        let venv = find_venv(&fs, &runner, &no_env(), &manifest(POETRY_MANIFEST)).unwrap();
        assert_eq!(venv, PathBuf::from("/venvs/demo-py3.12"));
    }

    #[test]
    fn poetry_failure_falls_back_to_local_venv() {
        let fs = FileSystem::create_null([("poetry.lock", ""), (".venv/lib/site.py", "")]);
        let venv = find_venv(&fs, &no_commands(), &no_env(), &manifest(POETRY_MANIFEST)).unwrap();
        assert_eq!(venv, PathBuf::from(".venv"));
    }

    #[test]
    fn poetry_failure_without_local_venv_is_an_error() {
        let fs = FileSystem::create_null([("poetry.lock", "")]);
        let err =
            find_venv(&fs, &no_commands(), &no_env(), &manifest(POETRY_MANIFEST)).unwrap_err();
        assert!(err.to_string().contains("poetry install"), "{err}");
    }

    #[test]
    fn uv_project_never_invokes_a_subprocess() {
        let fs = FileSystem::create_null([("uv.lock", ""), (".venv/lib/site.py", "")]);
        let runner = no_commands();
        let runs = runner.track_runs();
        let venv = find_venv(&fs, &runner, &no_env(), &manifest(UV_MANIFEST)).unwrap();
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
        let venv = find_venv(&fs, &no_commands(), &env, &manifest(UV_MANIFEST)).unwrap();
        assert_eq!(venv, PathBuf::from("/custom/env"));
    }

    #[test]
    fn nonexistent_uv_project_environment_falls_back_to_local_venv() {
        let fs = FileSystem::create_null([("uv.lock", ""), (".venv/lib/site.py", "")]);
        let env = Env::create_null([("UV_PROJECT_ENVIRONMENT", "/ghost")]);
        let venv = find_venv(&fs, &no_commands(), &env, &manifest(UV_MANIFEST)).unwrap();
        assert_eq!(venv, PathBuf::from(".venv"));
    }

    #[test]
    fn uv_project_without_venv_suggests_uv_sync() {
        let fs = FileSystem::create_null([("uv.lock", "")]);
        let err = find_venv(&fs, &no_commands(), &no_env(), &manifest(UV_MANIFEST)).unwrap_err();
        assert!(err.to_string().contains("uv sync"), "{err}");
    }

    #[test]
    fn uv_lock_beats_a_poetry_tool_table() {
        // A lockfile proves which tool manages the environment.
        let fs = FileSystem::create_null([("uv.lock", ""), (".venv/lib/site.py", "")]);
        let runner = no_commands();
        let runs = runner.track_runs();
        let venv = find_venv(&fs, &runner, &no_env(), &manifest(POETRY_MANIFEST)).unwrap();
        assert_eq!(venv, PathBuf::from(".venv"));
        assert_eq!(runs.data(), vec![]);
    }

    #[test]
    fn tool_table_decides_without_lockfiles() {
        let fs = FileSystem::create_null([(".venv/lib/site.py", "")]);
        let venv = find_venv(&fs, &no_commands(), &no_env(), &manifest(UV_MANIFEST)).unwrap();
        assert_eq!(venv, PathBuf::from(".venv"));
    }

    #[test]
    fn pep621_project_uses_the_active_virtualenv() {
        let fs = FileSystem::create_null([("/active/venv/lib/site.py", "")]);
        let env = Env::create_null([("VIRTUAL_ENV", "/active/venv")]);
        let venv = find_venv(&fs, &no_commands(), &env, &manifest(PEP621_MANIFEST)).unwrap();
        assert_eq!(venv, PathBuf::from("/active/venv"));
    }

    #[test]
    fn pep621_project_falls_back_to_local_venv_then_errors() {
        let fs = FileSystem::create_null([(".venv/lib/site.py", "")]);
        let venv = find_venv(&fs, &no_commands(), &no_env(), &manifest(PEP621_MANIFEST)).unwrap();
        assert_eq!(venv, PathBuf::from(".venv"));

        let fs = FileSystem::create_null([] as [(&str, &str); 0]);
        assert!(find_venv(&fs, &no_commands(), &no_env(), &manifest(PEP621_MANIFEST)).is_err());
    }
}
