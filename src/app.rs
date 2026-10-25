//! The application: wires manifest parsing, the import scan, matching,
//! and the ignorefile into one analysis run.

use std::collections::BTreeSet;
use std::io;
use std::path::Path;

use anyhow::{Result, anyhow};
use tracing::{debug, info};

use crate::ignorefile;
use crate::infra::env::Env;
use crate::infra::fs::FileSystem;
use crate::infra::process::CommandRunner;
#[cfg(test)]
use crate::infra::process::{Answer, Execution};
use crate::matching::{DependencyIndex, Scope};
use crate::pyproject::Manifest;
use crate::scan;
use crate::venv;

/// What to analyze, independent of the CLI layer.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Which unused dependencies to report.
    pub scope: Scope,
    /// Also search the project virtualenv for usage.
    pub virtualenv: bool,
    /// Skip the ignorefile.
    pub no_ignore: bool,
}

pub struct App {
    fs: FileSystem,
    runner: CommandRunner,
    env: Env,
}

/// Configuration for a nulled [`App`], in each wrapper's own language.
#[cfg(test)]
#[derive(Default)]
pub struct NullConfig {
    /// In-memory file tree; relative paths are files in the project.
    pub files: Vec<(String, String)>,
    /// Commands and how each one finishes.
    pub commands: Vec<(Execution, Answer)>,
    /// Environment variables.
    pub env: Vec<(String, String)>,
}

impl App {
    #[must_use]
    pub fn create() -> App {
        App {
            fs: FileSystem::create(),
            runner: CommandRunner::create(),
            env: Env::create(),
        }
    }

    #[cfg(test)]
    pub fn create_null(config: NullConfig) -> App {
        App {
            fs: FileSystem::create_null(config.files),
            runner: CommandRunner::create_null(config.commands),
            env: Env::create_null(config.env),
        }
    }

    /// Analyze the project in the current directory and return the unused
    /// dependencies, sorted by package name.
    ///
    /// # Errors
    ///
    /// Fails when pyproject.toml cannot be read or parsed, or when the
    /// virtualenv cannot be located with `Options::virtualenv` set.
    pub fn run(&self, options: &Options) -> Result<Vec<String>> {
        let source = self
            .fs
            .read_to_string_lossy(Path::new("pyproject.toml"))
            .map_err(|err| {
                if err.kind() == io::ErrorKind::NotFound {
                    anyhow!(
                        "pyproject.toml not found. Are you in the root directory of your project?"
                    )
                } else {
                    anyhow!("cannot read pyproject.toml: {err}")
                }
            })?;
        let manifest = Manifest::parse(&source)?;
        info!(?manifest);
        let ignored = if options.no_ignore {
            BTreeSet::new()
        } else {
            ignorefile::ignored_packages(&self.fs)
        };
        let mut index = DependencyIndex::new(manifest.dependencies(), &ignored);

        let venv_path = if options.virtualenv {
            let path = venv::find_venv(&self.fs, &self.runner, &self.env, manifest.flavor())?;
            info!("Reading files in venv: {}", path.display());
            Some(path)
        } else {
            None
        };
        for file in scan::scan_imports(&self.fs, venv_path.as_deref()) {
            for import in &file.imports {
                debug!(
                    import = import.as_str(),
                    path = %file.path.display(),
                    "Checking import",
                );
                for found in index.mark_used(import) {
                    info!(
                        found = found.spelling(),
                        group = ?found.group(),
                        path = %file.path.display(),
                    );
                }
            }
        }

        Ok(index.unused(options.scope))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    const MANIFEST: &str = "\
[tool.poetry.dependencies]
python = \"^3.11\"
requests = \"^2.31\"

[tool.poetry.group.dev.dependencies]
pytest = \"^8.0\"
";

    fn null_app(files: &[(&str, &str)]) -> App {
        App::create_null(NullConfig {
            files: files
                .iter()
                .map(|(p, c)| ((*p).to_string(), (*c).to_string()))
                .collect(),
            ..NullConfig::default()
        })
    }

    #[test]
    fn virtualenv_files_mark_usage() {
        let app = App::create_null(NullConfig {
            files: vec![
                ("pyproject.toml".into(), MANIFEST.into()),
                ("main.py".into(), "import os\n".into()),
                (
                    "/venvs/demo/lib/requests/__init__.py".into(),
                    "import requests\n".into(),
                ),
            ],
            commands: vec![(
                Execution::new("poetry", &["env", "info", "-p"]),
                Answer::stdout("/venvs/demo\n"),
            )],
            ..NullConfig::default()
        });
        let options = Options {
            virtualenv: true,
            ..Options::default()
        };
        assert_eq!(app.run(&options).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn poetry_is_not_invoked_without_the_virtualenv_option() {
        let fs = FileSystem::create_null([
            ("pyproject.toml", MANIFEST),
            ("main.py", "import requests\n"),
        ]);
        let runner = CommandRunner::create_null([]);
        let runs = runner.track_runs();
        let app = App {
            fs,
            runner,
            env: Env::create_null([] as [(&str, &str); 0]),
        };
        app.run(&Options::default()).unwrap();
        assert_eq!(runs.data(), vec![]);
    }

    #[test]
    fn ignorefile_matches_any_spelling() {
        let app = null_app(&[
            ("pyproject.toml", MANIFEST),
            ("main.py", "import os\n"),
            (ignorefile::IGNORE_FILE, "  Requests  \n"),
        ]);
        assert_eq!(app.run(&Options::default()).unwrap(), Vec::<String>::new());
    }
}

#[cfg(test)]
mod properties {
    use super::*;
    use crate::package::PackageName;
    use crate::testgen::identifier;
    use hegel::generators;
    use pretty_assertions::assert_eq;
    use std::fmt::Write;

    // For any set of declared dependencies and any imported subset, the app
    // reports exactly the difference, sorted by package name.
    #[hegel::test]
    fn reports_exactly_the_unimported_dependencies(tc: hegel::TestCase) {
        // Identifier-shaped package names import under their own name, so
        // the reference expectation needs no alias knowledge.
        let declared: Vec<String> =
            tc.draw(generators::vecs(identifier()).unique(true).max_size(8));
        let names: BTreeSet<PackageName> = declared.iter().map(|p| PackageName::new(p)).collect();
        tc.assume(names.len() == declared.len());
        let imported: BTreeSet<String> = declared
            .iter()
            .filter(|_| tc.draw(generators::booleans()))
            .cloned()
            .collect();

        let mut manifest = String::from("[tool.poetry.dependencies]\n");
        for package in &declared {
            writeln!(manifest, "{package} = \"^1.0\"").unwrap();
        }
        let mut source = String::new();
        for package in &imported {
            writeln!(source, "import {package}").unwrap();
        }

        let app = App::create_null(NullConfig {
            files: vec![
                ("pyproject.toml".into(), manifest),
                ("main.py".into(), source),
            ],
            ..NullConfig::default()
        });

        let mut expected: Vec<String> = declared
            .into_iter()
            .filter(|p| !imported.contains(p))
            .collect();
        expected.sort_by_key(|p| PackageName::new(p));
        assert_eq!(app.run(&Options::default()).unwrap(), expected);
    }
}
