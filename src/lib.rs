use std::{
    collections::BTreeMap,
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
mod name_map;
mod pyproject;
use crate::imports::{Import, extract_imports};
use crate::name_map::KNOWN_NAMES;
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

/// Builds a matching map from declared package names.
///
/// The map is filled with either the original package name -> [], or with
/// the alias -> [package names]. This helps us quickly determine which original
/// dependency to eliminate if either the original package name or alias is
/// found.
///
/// We do not simply track the aliases alone, as reporting an alias as obsolete
/// is not as straightforward to the user which line to eliminate from their
/// pyproject.toml.
fn build_alias_map<'a>(packages: impl Iterator<Item = &'a str>) -> BTreeMap<String, Vec<String>> {
    let mut dependencies: BTreeMap<String, Vec<String>> = BTreeMap::new();

    // Generate a list of possible aliases for the package
    packages.for_each(|s| {
        let package = String::from(s);
        dependencies.insert(package.clone(), vec![]);
        let mut alias = KNOWN_NAMES.get(&package).map(|a| String::from(*a));

        // Or basic replacement
        if alias.is_none() && package.contains('-') {
            alias = Some(package.replace('-', "_").to_lowercase());
        }
        if let Some(a) = alias {
            dependencies.entry(a).or_default().push(package);
        } else {
            dependencies.insert(package, vec![]);
        }
    });
    dependencies
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
    let mut main_deps = build_alias_map(manifest.main_dependencies());
    info!(?main_deps);
    let mut dev_deps = build_alias_map(manifest.dev_dependencies());
    info!(?dev_deps);

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
            // Packages may have several aliases
            let mut aliases = vec![];
            if let Some(item) = &import.item {
                // Google-style package naming
                aliases.push(format!("{}-{}", import.module.replace('.', "-"), item));
            }
            // DBT Adapters
            if import.module.starts_with("dbt.adapters") {
                let parts: Vec<&str> = import.module.split('.').collect();
                // A bare `import dbt.adapters` has no adapter segment.
                if parts.len() >= 3 {
                    aliases.push([parts[0], parts[2]].join("-"));
                }
            }
            // SQLAlchemy Extentions
            if import.module.contains('.') {
                aliases.push(import.module.split('.').collect::<Vec<&str>>().join("-"));
            }
            if let Some(p) = import.module.split_once('.') {
                aliases.push(p.0.to_string());
            }

            // Include parent packages after 1 level deep.
            // This is to catch things like
            // `from google.auth.transport import requests` --> google-auth
            let v: Vec<&str> = import.module.split('.').collect();
            if v.len() >= 2 {
                aliases.push(format!("{}-{}", v[0], v[1]));
            }

            // Just the package
            aliases.push(import.module);

            for alias in aliases {
                if main_deps.contains_key(&alias)
                    && let Some(v) = main_deps.remove(&alias)
                {
                    if v.is_empty() {
                        info!(found = alias, path = path.to_str());
                    } else {
                        for orig in v {
                            info!(found = orig, path = path.to_str());
                            main_deps.remove(&orig);
                        }
                    }
                }
                if dev_deps.contains_key(&alias)
                    && let Some(v) = dev_deps.remove(&alias)
                {
                    if v.is_empty() {
                        info!("Found {} in {}", alias, path.display());
                    } else {
                        for orig in v {
                            info!("Found {} in {}", orig, path.display());
                            dev_deps.remove(&orig);
                        }
                    }
                }
            }
        }

        let mut udeps = Vec::new();
        for (key, value) in &main_deps {
            // Only print the non-alias names
            if value.is_empty() {
                udeps.push(key.to_owned());
            }
        }
        if check_dev_deps {
            for (key, value) in &dev_deps {
                // Only print the non-alias names
                if value.is_empty() {
                    udeps.push(key.to_owned());
                }
            }
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
