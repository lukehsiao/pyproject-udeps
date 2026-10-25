use std::io::Write;

use clap::Parser;
use clap_verbosity_flag::Verbosity;

mod app;
mod ignorefile;
mod imports;
mod infra;
mod matching;
mod package;
mod pyproject;
mod scan;
#[cfg(test)]
mod testgen;
mod venv;
use crate::app::{App, Options};
use crate::matching::Scope;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[clap(flatten)]
    pub verbose: Verbosity,
    #[arg(short = 'e', long)]
    /// Look for dependency usage in the project virtualenv.
    ///
    /// Assumes you have already installed all dependencies. The virtualenv
    /// is discovered from the project's lockfile and tool tables: poetry
    /// projects via `poetry env info -p`, uv projects via
    /// `$UV_PROJECT_ENVIRONMENT`, and other PEP 621 projects via
    /// `$VIRTUAL_ENV`, each falling back to `.venv`.
    pub virtualenv: bool,
    #[arg(short, long)]
    /// Look for unused dependencies in dev-dependencies.
    ///
    /// Many projects include dev deps like CLI tools that are intentionally
    /// not directly used in the codebase.
    pub dev: bool,
    #[arg(long = "no-ignore")]
    /// Do not ignore the packages in the ignorefile.
    ///
    /// The ignorefile is .pyprojectudepsignore, or the legacy
    /// .poetryudepsignore as a fallback.
    pub no_ignore: bool,
}

impl From<&Cli> for Options {
    fn from(cli: &Cli) -> Options {
        Options {
            scope: if cli.dev {
                Scope::MainAndDev
            } else {
                Scope::Main
            },
            virtualenv: cli.virtualenv,
            no_ignore: cli.no_ignore,
        }
    }
}

/// Analyze the project in the current directory, write the unused
/// dependencies to `out` one per line, sorted by package name, and write
/// any error to `err` with its causes.
///
/// Returns the process exit code: 0 when no dependency is unused, 1 when
/// some are, and 2 when the analysis fails.
pub fn run(cli: &Cli, out: &mut impl Write, err: &mut impl Write) -> u8 {
    run_with(&App::create(), cli, out, err)
}

fn run_with(app: &App, cli: &Cli, out: &mut impl Write, err: &mut impl Write) -> u8 {
    let report = app.run(&Options::from(cli)).and_then(|udeps| {
        for dep in &udeps {
            writeln!(out, "{dep}")?;
        }
        Ok(udeps.is_empty())
    });
    match report {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(e) => {
            // Nothing is left to report a failure to write the error to.
            let _ = writeln!(err, "{e:#}");
            2
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::app::NullConfig;
    use crate::infra::process::{Answer, Execution};
    use pretty_assertions::assert_eq;

    const MANIFEST: &str = "\
[tool.poetry.dependencies]
zope = \"*\"
requests = \"*\"

[tool.poetry.group.dev.dependencies]
pytest = \"*\"
";

    #[derive(Debug, PartialEq, Eq)]
    struct Outcome {
        code: u8,
        out: String,
        err: String,
    }

    fn outcome(code: u8, out: &str, err: &str) -> Outcome {
        Outcome {
            code,
            out: out.to_owned(),
            err: err.to_owned(),
        }
    }

    fn run_args(args: &[&str], config: NullConfig) -> Outcome {
        let cli = Cli::try_parse_from(["pyproject-udeps"].iter().chain(args)).unwrap();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run_with(&App::create_null(config), &cli, &mut out, &mut err);
        Outcome {
            code,
            out: String::from_utf8(out).unwrap(),
            err: String::from_utf8(err).unwrap(),
        }
    }

    fn project(files: &[(&str, &str)]) -> NullConfig {
        NullConfig {
            files: files
                .iter()
                .map(|(path, contents)| ((*path).to_owned(), (*contents).to_owned()))
                .collect(),
            ..NullConfig::default()
        }
    }

    #[test]
    fn nothing_unused_exits_0_silently() {
        let config = project(&[
            ("pyproject.toml", MANIFEST),
            ("main.py", "import requests, zope\n"),
        ]);
        assert_eq!(run_args(&[], config), outcome(0, "", ""));
    }

    #[test]
    fn unused_dependencies_exit_1_one_per_line_sorted() {
        let config = project(&[("pyproject.toml", MANIFEST), ("main.py", "import os\n")]);
        assert_eq!(run_args(&[], config), outcome(1, "requests\nzope\n", ""));
    }

    #[test]
    fn missing_pyproject_exits_2_with_the_hint() {
        assert_eq!(
            run_args(&[], project(&[])),
            outcome(
                2,
                "",
                "pyproject.toml not found. Are you in the root directory of your project?\n"
            )
        );
    }

    #[test]
    fn dev_flag_also_reports_dev_dependencies() {
        let files = [("pyproject.toml", MANIFEST), ("main.py", "import zope\n")];
        assert_eq!(run_args(&[], project(&files)), outcome(1, "requests\n", ""));
        assert_eq!(
            run_args(&["--dev"], project(&files)),
            outcome(1, "pytest\nrequests\n", "")
        );
    }

    #[test]
    fn no_ignore_flag_skips_the_ignorefile() {
        let files = [
            ("pyproject.toml", MANIFEST),
            ("main.py", "import zope\n"),
            (".pyprojectudepsignore", "requests\n"),
        ];
        assert_eq!(run_args(&[], project(&files)), outcome(0, "", ""));
        assert_eq!(
            run_args(&["--no-ignore"], project(&files)),
            outcome(1, "requests\n", "")
        );
    }

    #[test]
    fn errors_show_their_causes() {
        let config = NullConfig {
            commands: vec![(
                Execution::new("poetry", &["env", "info", "-p"]),
                Answer::stdout("/venvs/gone\n"),
            )],
            ..project(&[("pyproject.toml", MANIFEST), ("poetry.lock", "")])
        };
        assert_eq!(
            run_args(&["-e"], config),
            outcome(
                2,
                "",
                "could not locate a virtualenv; install the project with `poetry install`: \
                 /venvs/gone does not exist, and .venv does not exist\n"
            )
        );
    }
}
