//! Virtualenv discovery.

use anyhow::Result;

use crate::infra::process::CommandRunner;

/// Locate the project's virtualenv directory.
///
/// # Errors
///
/// Fails when poetry is not installed or reports no virtualenv for the
/// project.
pub fn find_venv(runner: &CommandRunner) -> Result<String> {
    runner.run("poetry", &["env", "info", "-p"])
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::infra::process::NullResponse;

    #[test]
    fn returns_the_path_poetry_reports() {
        let runner = CommandRunner::create_null([(
            "poetry env info -p",
            NullResponse::Stdout("/venvs/demo-py3.12".into()),
        )]);
        assert_eq!(find_venv(&runner).unwrap(), "/venvs/demo-py3.12");
    }

    #[test]
    fn propagates_poetry_failure() {
        let runner = CommandRunner::create_null([(
            "poetry env info -p",
            NullResponse::Failure("no virtualenv has been created".into()),
        )]);
        assert!(find_venv(&runner).is_err());
    }
}
