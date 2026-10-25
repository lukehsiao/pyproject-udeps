//! Nullable wrapper over subprocess execution.

#[cfg(test)]
use std::collections::HashMap;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::infra::nullable::OutputListener;
#[cfg(test)]
use crate::infra::nullable::OutputTracker;

/// One command invocation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Execution {
    pub program: String,
    pub args: Vec<String>,
}

impl Execution {
    #[must_use]
    pub fn new(program: &str, args: &[&str]) -> Self {
        Execution {
            program: program.to_owned(),
            args: args.iter().map(|&arg| arg.to_owned()).collect(),
        }
    }
}

/// What a finished command produced, before this wrapper reads it.
///
/// `std::process::ExitStatus` cannot be built portably, so the real and
/// stubbed runs both reduce to this, and one code path interprets it.
struct RawOutput {
    /// How a failed run ended, as `ExitStatus` describes it, such as
    /// `exit status: 1` or `signal: 9 (SIGKILL)`. `None` on success.
    failure: Option<String>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl From<std::process::Output> for RawOutput {
    fn from(output: std::process::Output) -> Self {
        RawOutput {
            failure: (!output.status.success()).then(|| output.status.to_string()),
            stdout: output.stdout,
            stderr: output.stderr,
        }
    }
}

#[derive(Debug)]
pub struct CommandRunner {
    runner: Runner,
    runs: OutputListener<Execution>,
}

#[derive(Debug)]
enum Runner {
    Real,
    #[cfg(test)]
    Stubbed(HashMap<Execution, Answer>),
}

impl CommandRunner {
    #[must_use]
    pub fn create() -> Self {
        CommandRunner {
            runner: Runner::Real,
            runs: OutputListener::default(),
        }
    }

    /// Run a command and return its trimmed stdout.
    ///
    /// # Errors
    ///
    /// Fails when the command cannot be spawned or exits nonzero, with
    /// stderr in the error message.
    pub fn run(&self, program: &str, args: &[&str]) -> Result<String> {
        self.runs.emit(|| Execution::new(program, args));
        let output = match &self.runner {
            Runner::Real => Command::new(program)
                .args(args)
                .output()
                .map(RawOutput::from),
            #[cfg(test)]
            Runner::Stubbed(answers) => stub::output(answers, program, args),
        }
        .with_context(|| format!("failed to run `{}`", command_line(program, args)))?;
        if let Some(status) = output.failure {
            bail!(
                "`{}` failed ({status}): {}",
                command_line(program, args),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

/// A command as a shell user would type it, for error messages.
fn command_line(program: &str, args: &[&str]) -> String {
    let mut line = program.to_owned();
    for arg in args {
        line.push(' ');
        line.push_str(arg);
    }
    line
}

/// How a stubbed command finishes.
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct Answer {
    code: i32,
    stdout: String,
    stderr: String,
}

#[cfg(test)]
impl Answer {
    /// A successful run that prints `stdout`.
    pub fn stdout(stdout: &str) -> Self {
        Answer {
            code: 0,
            stdout: stdout.to_owned(),
            stderr: String::new(),
        }
    }

    /// A run that exits with `code` and prints nothing.
    pub fn exit(code: i32) -> Self {
        Answer {
            code,
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    #[must_use]
    pub fn stderr(mut self, stderr: &str) -> Self {
        stderr.clone_into(&mut self.stderr);
        self
    }
}

#[cfg(test)]
impl CommandRunner {
    /// A runner that answers only the configured commands. Any other
    /// command fails to spawn, which is also the safe default world: a
    /// tool that is not installed.
    pub fn create_null(answers: impl IntoIterator<Item = (Execution, Answer)>) -> Self {
        CommandRunner {
            runner: Runner::Stubbed(answers.into_iter().collect()),
            runs: OutputListener::default(),
        }
    }

    /// Observe every command this runner is asked to execute.
    pub fn track_runs(&self) -> OutputTracker<Execution> {
        self.runs.track()
    }
}

#[cfg(test)]
mod stub {
    use std::collections::HashMap;
    use std::io;

    use super::{Answer, Execution, RawOutput, command_line};

    pub(super) fn output(
        answers: &HashMap<Execution, Answer>,
        program: &str,
        args: &[&str],
    ) -> io::Result<RawOutput> {
        let answer = answers.get(&Execution::new(program, args)).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "Nulled CommandRunner: no answer configured for `{}`",
                    command_line(program, args)
                ),
            )
        })?;
        Ok(RawOutput {
            // The same text a real `ExitStatus` shows on Unix.
            failure: (answer.code != 0).then(|| format!("exit status: {}", answer.code)),
            stdout: answer.stdout.clone().into_bytes(),
            stderr: answer.stderr.clone().into_bytes(),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn poetry_env_info() -> Execution {
        Execution::new("poetry", &["env", "info", "-p"])
    }

    #[test]
    fn nulled_runner_answers_configured_command_through_the_shared_path() {
        let runner =
            CommandRunner::create_null([(poetry_env_info(), Answer::stdout("/venvs/demo\n"))]);
        let out = runner.run("poetry", &["env", "info", "-p"]).unwrap();
        assert_eq!(out, "/venvs/demo");
    }

    #[test]
    fn nulled_runner_fails_loudly_on_unconfigured_command() {
        let runner = CommandRunner::create_null([]);
        let err = format!("{:#}", runner.run("uv", &["python", "find"]).unwrap_err());
        assert!(err.contains("no answer configured"), "{err}");
        assert!(err.contains("uv python find"), "{err}");
    }

    #[test]
    fn nonzero_exit_reports_the_code_and_stderr() {
        let runner = CommandRunner::create_null([(
            poetry_env_info(),
            Answer::exit(1).stderr("poetry could not find a venv\n"),
        )]);
        let err = runner
            .run("poetry", &["env", "info", "-p"])
            .unwrap_err()
            .to_string();
        assert!(err.contains("(exit status: 1)"), "{err}");
        assert!(err.ends_with(": poetry could not find a venv"), "{err}");
    }

    #[test]
    fn tracker_records_answered_and_unanswered_runs() {
        let runner =
            CommandRunner::create_null([(Execution::new("echo", &["hi"]), Answer::stdout(""))]);
        let tracker = runner.track_runs();
        let _ = runner.run("echo", &["hi"]);
        let _ = runner.run("not", &["configured"]);
        assert_eq!(
            tracker.data(),
            vec![
                Execution::new("echo", &["hi"]),
                Execution::new("not", &["configured"]),
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
        let err = runner.run("false", &[]).unwrap_err().to_string();
        assert!(err.contains("(exit status: 1)"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn real_runner_reports_the_signal_that_ended_the_command() {
        let runner = CommandRunner::create();
        // Signal 9 is SIGKILL, which the shell cannot catch.
        let err = runner
            .run("sh", &["-c", "kill -9 $$"])
            .unwrap_err()
            .to_string();
        assert!(err.contains("(signal: 9 (SIGKILL))"), "{err}");
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
