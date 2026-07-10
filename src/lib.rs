use anyhow::Result;
use clap::Parser;
use clap_verbosity_flag::Verbosity;

pub mod app;
mod ignorefile;
mod imports;
pub mod infra;
mod matching;
mod name_map;
mod pyproject;
#[cfg(test)]
mod testgen;
mod venv;
use crate::app::{App, Options};

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
    /// `$UV_PROJECT_ENVIRONMENT` or `.venv`, and other PEP 621 projects via
    /// `$VIRTUAL_ENV` or `.venv`.
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
            dev: cli.dev,
            virtualenv: cli.virtualenv,
            no_ignore: cli.no_ignore,
        }
    }
}

/// Analyze the project in the current directory and return the unused
/// dependencies, sorted.
///
/// # Errors
///
/// Fails when pyproject.toml cannot be read or parsed, or when the
/// virtualenv cannot be located with `--virtualenv`.
pub fn run(cli: &Cli) -> Result<Vec<String>> {
    App::create().run(&Options::from(cli))
}
