//! The application: wires manifest parsing, file walks, import matching,
//! and the ignorefile into one analysis run.

use std::path::Path;

use anyhow::Result;
use tracing::{debug, error, info};

use crate::ignorefile;
use crate::imports::extract_imports;
use crate::infra::fs::{FileSystem, WalkFilters};
use crate::infra::process::{CommandRunner, NullResponse};
use crate::matching::DependencyIndex;
use crate::pyproject::Manifest;
use crate::venv;

/// What to analyze, independent of the CLI layer.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Also report unused dev dependencies.
    pub dev: bool,
    /// Also search the project virtualenv for usage.
    pub virtualenv: bool,
    /// Skip the ignorefile.
    pub no_ignore: bool,
}

pub struct App {
    fs: FileSystem,
    runner: CommandRunner,
}

/// Configuration for a nulled [`App`], in each wrapper's own language.
#[derive(Default)]
pub struct NullConfig {
    /// In-memory file tree; relative paths are files in the project.
    pub files: Vec<(String, String)>,
    /// Rendered command line -> canned response.
    pub commands: Vec<(String, NullResponse)>,
}

impl App {
    #[must_use]
    pub fn create() -> App {
        App {
            fs: FileSystem::create(),
            runner: CommandRunner::create(),
        }
    }

    #[must_use]
    pub fn create_null(config: NullConfig) -> App {
        App {
            fs: FileSystem::create_null(config.files),
            runner: CommandRunner::create_null(config.commands),
        }
    }

    /// Analyze the project in the current directory and return the declared
    /// dependencies never seen in an import, sorted.
    ///
    /// # Errors
    ///
    /// Fails when pyproject.toml cannot be read or parsed, or when the
    /// virtualenv cannot be located with `Options::virtualenv` set.
    pub fn run(&self, options: &Options) -> Result<Vec<String>> {
        let pyproject_path = Path::new("pyproject.toml");
        if !self.fs.exists(pyproject_path) {
            error!("pyproject.toml not found. Are you in the root directory of your project?");
            // Just fall through, the read below raises the error for us.
        }

        let manifest = Manifest::parse(&self.fs.read_to_string_lossy(pyproject_path)?)?;
        info!(?manifest);
        let mut main_deps = DependencyIndex::new(manifest.main_dependencies().map(str::to_owned));
        let mut dev_deps = DependencyIndex::new(manifest.dev_dependencies().map(str::to_owned));

        // The walks stream files from background threads while this thread
        // extracts imports and matches them.
        let mut walks = Vec::new();
        if options.virtualenv {
            let venv_path = venv::find_venv(&self.runner)?;
            info!("Reading files in venv: {venv_path}");
            walks.push(
                self.fs
                    .walk_python_files(Path::new(&venv_path), WalkFilters::None),
            );
        }
        walks.push(
            self.fs
                .walk_python_files(Path::new("./"), WalkFilters::Standard),
        );

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
        if options.dev {
            udeps.extend(dev_deps.unused());
        }
        if !options.no_ignore {
            let ignored = ignorefile::ignored_packages(&self.fs);
            udeps.retain(|dep| !ignored.contains(dep));
        }
        Ok(udeps)
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
    fn reports_declared_minus_imported() {
        let app = null_app(&[("pyproject.toml", MANIFEST), ("main.py", "import os\n")]);
        assert_eq!(app.run(&Options::default()).unwrap(), vec!["requests"]);
    }

    #[test]
    fn imported_dependencies_are_not_reported() {
        let app = null_app(&[
            ("pyproject.toml", MANIFEST),
            ("main.py", "import requests\n"),
        ]);
        assert_eq!(app.run(&Options::default()).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn dev_dependencies_reported_only_with_dev_option() {
        let files = [
            ("pyproject.toml", MANIFEST),
            ("main.py", "import requests\n"),
        ];
        let app = null_app(&files);
        assert_eq!(app.run(&Options::default()).unwrap(), Vec::<String>::new());

        let app = null_app(&files);
        let options = Options {
            dev: true,
            ..Options::default()
        };
        assert_eq!(app.run(&options).unwrap(), vec!["pytest"]);
    }

    #[test]
    fn missing_pyproject_is_an_error() {
        let app = null_app(&[]);
        assert!(app.run(&Options::default()).is_err());
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
                "poetry env info -p".into(),
                NullResponse::Stdout("/venvs/demo".into()),
            )],
        });
        let options = Options {
            virtualenv: true,
            ..Options::default()
        };
        assert_eq!(app.run(&options).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn venv_discovery_failure_is_an_error() {
        let app = App::create_null(NullConfig {
            files: vec![("pyproject.toml".into(), MANIFEST.into())],
            commands: vec![(
                "poetry env info -p".into(),
                NullResponse::Failure("no venv".into()),
            )],
        });
        let options = Options {
            virtualenv: true,
            ..Options::default()
        };
        assert!(app.run(&options).is_err());
    }

    #[test]
    fn poetry_is_not_invoked_without_the_virtualenv_option() {
        let fs = FileSystem::create_null([
            ("pyproject.toml", MANIFEST),
            ("main.py", "import requests\n"),
        ]);
        let runner = CommandRunner::create_null([] as [(&str, NullResponse); 0]);
        let runs = runner.track_runs();
        let app = App { fs, runner };
        app.run(&Options::default()).unwrap();
        assert_eq!(runs.data(), vec![]);
    }

    #[test]
    fn ignorefile_filters_the_report() {
        let app = null_app(&[
            ("pyproject.toml", MANIFEST),
            ("main.py", "import os\n"),
            (ignorefile::IGNORE_FILE, "requests\n"),
        ]);
        assert_eq!(app.run(&Options::default()).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn no_ignore_option_bypasses_the_ignorefile() {
        let app = null_app(&[
            ("pyproject.toml", MANIFEST),
            ("main.py", "import os\n"),
            (ignorefile::IGNORE_FILE, "requests\n"),
        ]);
        let options = Options {
            no_ignore: true,
            ..Options::default()
        };
        assert_eq!(app.run(&options).unwrap(), vec!["requests"]);
    }
}

#[cfg(test)]
mod properties {
    use super::*;
    use crate::testgen::identifier;
    use hegel::generators;
    use pretty_assertions::assert_eq;
    use std::collections::BTreeSet;
    use std::fmt::Write;

    // P12: for any set of declared dependencies and any imported subset, the
    // app reports exactly the difference, sorted.
    #[hegel::test]
    fn reports_exactly_the_unimported_dependencies(tc: hegel::TestCase) {
        // Identifier-shaped package names import under their own name, so
        // the reference expectation needs no alias knowledge.
        let declared: Vec<String> =
            tc.draw(generators::vecs(identifier()).unique(true).max_size(8));
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

        let expected: Vec<String> = declared
            .iter()
            .filter(|p| !imported.contains(*p))
            .cloned()
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect();
        assert_eq!(app.run(&Options::default()).unwrap(), expected);
    }
}
