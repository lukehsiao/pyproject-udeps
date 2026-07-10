use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader, Read},
    path::{Path, PathBuf},
    thread,
};

use anyhow::Result;
use clap::Parser;
use clap_verbosity_flag::Verbosity;
use ignore::{WalkBuilder, types::TypesBuilder};
use tracing::{debug, error, info};
use xshell::{Shell, cmd};

mod imports;
mod matching;
mod name_map;
mod pyproject;
#[cfg(test)]
mod testgen;
use crate::imports::{Import, extract_imports};
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

fn get_venv_path() -> Result<String> {
    let sh = Shell::new()?;

    Ok(cmd!(sh, "poetry env info -p").quiet().read()?)
}

// Read lines from ignorefile. Ignore empty lines and comments.
fn read_lines(file: &File) -> io::Result<Vec<String>> {
    let lines: Vec<_> = BufReader::new(file).lines().collect::<Result<_, _>>()?;
    Ok(lines
        .into_iter()
        .filter(|line| !(line.is_empty() || line.trim_start().starts_with('#')))
        .collect())
}

// Filter out dependencies from udeps if they are in the ignorefile.
fn apply_ignorefile(udeps: Vec<String>) -> io::Result<Vec<String>> {
    let ignore_packages = match File::open(IGNORE_FILE) {
        Ok(poetryudepsignore) => read_lines(&poetryudepsignore)?,
        Err(_) => return Ok(udeps),
    };

    debug!(ignored = ?ignore_packages);
    Ok(udeps
        .into_iter()
        .filter(|dep| !ignore_packages.contains(dep))
        .collect())
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::missing_errors_doc)]
#[allow(clippy::missing_panics_doc)]
pub fn run(cli: &Cli) -> Result<Option<Vec<String>>> {
    let pyproject_path = Path::new("pyproject.toml");

    match pyproject_path.try_exists() {
        Ok(true) => (),
        Ok(false) => {
            error!("pyproject.toml not found. Are you in the root directory of your project?",);
            // Just fall through, the subsequent read will raise the error for us
        }
        Err(e) => {
            error!("pyproject.toml not found. Are you in the root directory of your project?",);
            return Err(e.into());
        }
    }

    let manifest = Manifest::parse(&fs::read_to_string(pyproject_path)?)?;
    info!(?manifest);
    let mut main_deps = DependencyIndex::new(manifest.main_dependencies().map(str::to_owned));
    let mut dev_deps = DependencyIndex::new(manifest.dev_dependencies().map(str::to_owned));

    let (tx, rx) = flume::bounded::<(Import, PathBuf)>(100);

    // Setup main thread for stdout
    let check_dev_deps = cli.dev;
    let no_ignore = cli.no_ignore;
    let stdout_thread = thread::spawn(move || -> io::Result<Option<Vec<String>>> {
        for (import, path) in rx {
            debug!(
                module = import.module,
                item = ?import.item,
                path = path.to_str(),
                "Checking import",
            );
            for found in main_deps.mark_used(&import) {
                info!(found, path = path.to_str());
            }
            for found in dev_deps.mark_used(&import) {
                info!(found, path = path.to_str(), dev = true);
            }
        }

        let mut udeps = main_deps.unused();
        if check_dev_deps {
            udeps.extend(dev_deps.unused());
        }

        if udeps.is_empty() {
            Ok(None)
        } else if no_ignore {
            Ok(Some(udeps))
        } else {
            // Filter out those from ignorefile
            let filtered = apply_ignorefile(udeps)?;
            if filtered.is_empty() {
                Ok(None)
            } else {
                Ok(Some(filtered))
            }
        }
    });

    if cli.virtualenv {
        // Iterate over Python files in parallel in the venv
        let venv_path = get_venv_path()?;
        info!("Reading files in venv: {}", venv_path);
        let types = TypesBuilder::new().add_defaults().select("py").build()?;
        let walker = WalkBuilder::new(venv_path)
            .standard_filters(false)
            .types(types)
            .build_parallel();
        walker.run(|| {
            let tx = tx.clone();
            Box::new(move |result| {
                use ignore::WalkState::Continue;

                if let Ok(dir) = result
                    && dir.file_type().unwrap().is_file()
                {
                    let mut file = File::open(dir.path()).unwrap();
                    let mut buf = Vec::new();
                    file.read_to_end(&mut buf).unwrap();
                    let contents = String::from_utf8_lossy(&buf);
                    let v = extract_imports(&contents);

                    let path = dir.into_path();
                    for import in v {
                        tx.send((import, path.clone())).unwrap();
                    }
                }

                Continue
            })
        });
    }

    // Iterate over Python files in parallel in the current directory
    let types = TypesBuilder::new().add_defaults().select("py").build()?;
    let walker = WalkBuilder::new("./")
        .standard_filters(true)
        .types(types)
        .build_parallel();
    walker.run(|| {
        let tx = tx.clone();
        Box::new(move |result| {
            use ignore::WalkState::Continue;

            if let Ok(dir) = result
                && dir.file_type().unwrap().is_file()
            {
                let mut file = File::open(dir.path()).unwrap();
                let mut buf = Vec::new();
                file.read_to_end(&mut buf).unwrap();
                let contents = String::from_utf8_lossy(&buf);
                let v = extract_imports(&contents);

                let path = dir.into_path();
                for import in v {
                    tx.send((import, path.clone())).unwrap();
                }
            }

            Continue
        })
    });

    drop(tx);
    match stdout_thread.join() {
        Ok(j) => {
            match j {
                Ok(deps) => Ok(deps),
                Err(err) => {
                    // A broken pipe means graceful termination, so fall through.
                    // Otherwise, something bad happened while writing to stdout, so bubble
                    // it up.
                    if err.kind() == io::ErrorKind::BrokenPipe {
                        Ok(None)
                    } else {
                        Err(err.into())
                    }
                }
            }
        }
        Err(_) => todo!(),
    }
}
