//! Nullable wrapper over subprocess execution.

use std::collections::HashMap;
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::infra::util::{OutputListener, OutputTracker};

/// One command invocation, in rendered form (e.g. `poetry env info -p`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    pub command: String,
}

/// What a nulled runner answers for one command line.
#[derive(Debug, Clone)]
pub enum NullResponse {
    /// Successful run with this stdout.
    Stdout(String),
    /// Failed run with this error message.
    Failure(String),
}

#[derive(Debug)]
pub struct CommandRunner {
    inner: Inner,
    listener: OutputListener<Execution>,
}

#[derive(Debug)]
enum Inner {
    Real,
    Null(HashMap<String, NullResponse>),
}

impl CommandRunner {
    #[must_use]
    pub fn create() -> Self {
        CommandRunner {
            inner: Inner::Real,
            listener: OutputListener::new(),
        }
    }

    /// A runner that answers only the configured command lines. Any other
    /// command fails loudly, which is also the safe default world: a tool
    /// that is not installed. Keys are rendered command lines, e.g.
    /// `("poetry env info -p", NullResponse::Stdout("/venvs/x".into()))`.
    pub fn create_null(
        responses: impl IntoIterator<Item = (impl Into<String>, NullResponse)>,
    ) -> Self {
        CommandRunner {
            inner: Inner::Null(responses.into_iter().map(|(k, v)| (k.into(), v)).collect()),
            listener: OutputListener::new(),
        }
    }

    /// Run a command and return its trimmed stdout.
    ///
    /// # Errors
    ///
    /// Fails when the command cannot be spawned or exits nonzero, with
    /// stderr in the error message.
    pub fn run(&self, program: &str, args: &[&str]) -> Result<String> {
        let command = format!("{program} {}", args.join(" "));
        self.listener.emit(Execution {
            command: command.clone(),
        });
        match &self.inner {
            Inner::Real => {
                let output = Command::new(program)
                    .args(args)
                    .output()
                    .with_context(|| format!("failed to run `{command}`"))?;
                if !output.status.success() {
                    bail!(
                        "`{command}` failed ({}): {}",
                        output.status,
                        String::from_utf8_lossy(&output.stderr).trim()
                    );
                }
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            }
            Inner::Null(responses) => match responses.get(&command) {
                Some(NullResponse::Stdout(stdout)) => Ok(stdout.clone()),
                Some(NullResponse::Failure(message)) => Err(anyhow!(
                    "Nulled CommandRunner: configured failure for `{command}`: {message}"
                )),
                None => Err(anyhow!(
                    "Nulled CommandRunner: no response configured for `{command}`"
                )),
            },
        }
    }

    /// Observe every command this runner is asked to execute.
    pub fn track_runs(&self) -> OutputTracker<Execution> {
        self.listener.track()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn nulled_runner_answers_configured_command() {
        let runner = CommandRunner::create_null([(
            "poetry env info -p",
            NullResponse::Stdout("/venvs/demo".into()),
        )]);
        let out = runner.run("poetry", &["env", "info", "-p"]).unwrap();
        assert_eq!(out, "/venvs/demo");
    }

    #[test]
    fn nulled_runner_fails_loudly_on_unconfigured_command() {
        let runner = CommandRunner::create_null([] as [(&str, NullResponse); 0]);
        let err = runner.run("uv", &["python", "find"]).unwrap_err();
        assert!(err.to_string().contains("no response configured"), "{err}");
        assert!(err.to_string().contains("uv python find"), "{err}");
    }

    #[test]
    fn nulled_runner_returns_configured_failure() {
        let runner = CommandRunner::create_null([(
            "poetry env info -p",
            NullResponse::Failure("poetry could not find a venv".into()),
        )]);
        let err = runner.run("poetry", &["env", "info", "-p"]).unwrap_err();
        assert!(err.to_string().contains("poetry could not find a venv"));
    }

    #[test]
    fn tracker_records_executions_in_both_modes() {
        let runner = CommandRunner::create_null([("echo hi", NullResponse::Stdout(String::new()))]);
        let tracker = runner.track_runs();
        let _ = runner.run("echo", &["hi"]);
        let _ = runner.run("not", &["configured"]);
        assert_eq!(
            tracker.data(),
            vec![
                Execution {
                    command: "echo hi".into()
                },
                Execution {
                    command: "not configured".into()
                },
            ]
        );
    }

    // Narrow integration tests: document the real behavior the stub mimics.

    #[test]
    fn real_runner_returns_trimmed_stdout() {
        let runner = CommandRunner::create();
        assert_eq!(runner.run("echo", &["hello"]).unwrap(), "hello");
    }

    #[test]
    fn real_runner_reports_nonzero_exit_as_error() {
        let runner = CommandRunner::create();
        assert!(runner.run("false", &[]).is_err());
    }

    #[test]
    fn real_runner_reports_missing_binary_as_error() {
        let runner = CommandRunner::create();
        let err = runner
            .run("definitely-not-a-real-binary-9c4f", &[])
            .unwrap_err();
        assert!(err.to_string().contains("failed to run"), "{err}");
    }
}
