use std::path::Path;

use anyhow::Result;
use clap::Parser;
use clap_verbosity_flag::Verbosity;
use tracing::{debug, error, info};

mod imports;
pub mod infra;
mod matching;
mod name_map;
mod pyproject;
#[cfg(test)]
mod testgen;
use crate::imports::extract_imports;
use crate::infra::fs::{FileSystem, WalkFilters};
use crate::infra::process::CommandRunner;
use crate::matching::DependencyIndex;
use crate::pyproject::Manifest;

const IGNORE_FILE: &str = ".poetryudepsignore";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[clap(flatten)]
    pub verbose: Verbosity,
    #[arg(short = 'e', long)]
    /// Look for dependency usage in the poetry virtualenv.
    ///
    /// Assumes you have already installed all dependencies using poetry. It
    /// will check the directory specified by `poetry env info -p`.
    pub virtualenv: bool,
    #[arg(short, long)]
    /// Look for unused dependencies in dev-dependencies.
    ///
    /// Many projects include dev deps like CLI tools that are intentionally
    /// not directly used in the codebase.
    pub dev: bool,
    #[arg(long = "no-ignore")]
    /// Do not ignore the packages in .poetryudepsignore
    pub no_ignore: bool,
}

fn get_venv_path(runner: &CommandRunner) -> Result<String> {
    runner.run("poetry", &["env", "info", "-p"])
}

// Filter out dependencies from udeps if they are in the ignorefile. A
// missing or unreadable ignorefile filters nothing.
fn apply_ignorefile(fs: &FileSystem, udeps: Vec<String>) -> Vec<String> {
    let Ok(contents) = fs.read_to_string_lossy(Path::new(IGNORE_FILE)) else {
        return udeps;
    };
    let ignored: Vec<&str> = contents
        .lines()
        .filter(|line| !(line.is_empty() || line.trim_start().starts_with('#')))
        .collect();
    debug!(?ignored);
    udeps
        .into_iter()
        .filter(|dep| !ignored.contains(&dep.as_str()))
        .collect()
}

#[allow(clippy::missing_errors_doc)]
pub fn run(cli: &Cli) -> Result<Option<Vec<String>>> {
    let fs = FileSystem::create();
    let runner = CommandRunner::create();

    let pyproject_path = Path::new("pyproject.toml");
    if !fs.exists(pyproject_path) {
        error!("pyproject.toml not found. Are you in the root directory of your project?");
        // Just fall through, the subsequent read will raise the error for us
    }

    let manifest = Manifest::parse(&fs.read_to_string_lossy(pyproject_path)?)?;
    info!(?manifest);
    let mut main_deps = DependencyIndex::new(manifest.main_dependencies().map(str::to_owned));
    let mut dev_deps = DependencyIndex::new(manifest.dev_dependencies().map(str::to_owned));

    // The walks stream files from background threads while this thread
    // extracts imports and matches them.
    let mut walks = Vec::new();
    if cli.virtualenv {
        let venv_path = get_venv_path(&runner)?;
        info!("Reading files in venv: {venv_path}");
        walks.push(fs.walk_python_files(Path::new(&venv_path), WalkFilters::None));
    }
    walks.push(fs.walk_python_files(Path::new("./"), WalkFilters::Standard));

    for files in walks {
        for file in files {
            for import in extract_imports(&file.contents) {
                debug!(
                    module = import.module,
                    item = ?import.item,
                    path = %file.path.display(),
                    "Checking import",
                );
                for found in main_deps.mark_used(&import) {
                    info!(found, path = %file.path.display());
                }
                for found in dev_deps.mark_used(&import) {
                    info!(found, path = %file.path.display(), dev = true);
                }
            }
        }
    }

    let mut udeps = main_deps.unused();
    if cli.dev {
        udeps.extend(dev_deps.unused());
    }

    if !cli.no_ignore {
        udeps = apply_ignorefile(&fs, udeps);
    }
    if udeps.is_empty() {
        Ok(None)
    } else {
        Ok(Some(udeps))
    }
}
