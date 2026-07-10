//! Declared-dependency extraction from pyproject.toml.
//!
//! This module hides the zoo of places dependencies can be declared. Callers
//! see two flat name sets: main and dev.

use std::collections::BTreeSet;

use anyhow::{Result, bail};
use toml::Value;
use tracing::info;

/// The declared dependencies of a project, by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    main: BTreeSet<String>,
    dev: BTreeSet<String>,
}

impl Manifest {
    /// Parse pyproject.toml source.
    ///
    /// Main dependencies come from `[tool.poetry.dependencies]`, falling back
    /// to PEP 621 `[project].dependencies`; missing both is an error since a
    /// manifest with no dependency declarations cannot be analyzed. Dev
    /// dependencies come from `[tool.poetry.dev-dependencies]` (poetry < 1.2)
    /// or `[tool.poetry.group.dev.dependencies]`, and are optional. The
    /// `python` interpreter requirement is never a dependency.
    pub fn parse(source: &str) -> Result<Manifest> {
        let value = Value::Table(source.parse::<toml::Table>()?);

        let main: BTreeSet<String> = match poetry_table(&value, "dependencies") {
            Some(deps) => table_keys(deps),
            None => match project_dependency_array(&value) {
                Some(deps) => deps,
                None => bail!("failed to parse dependencies from pyproject.toml"),
            },
        };

        let dev: BTreeSet<String> = poetry_table(&value, "dev-dependencies")
            .map(table_keys)
            .or_else(|| {
                value
                    .get("tool")
                    .and_then(|tool| tool.get("poetry"))
                    .and_then(|poetry| poetry.get("group"))
                    .and_then(|group| group.get("dev"))
                    .and_then(|dev| dev.get("dependencies"))
                    .and_then(Value::as_table)
                    .map(table_keys)
            })
            .unwrap_or_else(|| {
                info!("no dev dependencies found in pyproject.toml");
                BTreeSet::new()
            });

        Ok(Manifest {
            main: without_python(main),
            dev: without_python(dev),
        })
    }

    pub fn main_dependencies(&self) -> impl Iterator<Item = &str> + '_ {
        self.main.iter().map(String::as_str)
    }

    pub fn dev_dependencies(&self) -> impl Iterator<Item = &str> + '_ {
        self.dev.iter().map(String::as_str)
    }
}

fn poetry_table<'a>(value: &'a Value, key: &str) -> Option<&'a toml::Table> {
    value
        .get("tool")
        .and_then(|tool| tool.get("poetry"))
        .and_then(|poetry| poetry.get(key))
        .and_then(Value::as_table)
}

fn table_keys(table: &toml::Table) -> BTreeSet<String> {
    table.keys().cloned().collect()
}

fn project_dependency_array(value: &Value) -> Option<BTreeSet<String>> {
    value
        .get("project")
        .and_then(|project| project.get("dependencies"))
        .and_then(Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(Value::as_str)
                .filter_map(requirement_name)
                .map(str::to_owned)
                .collect()
        })
}

fn without_python(mut names: BTreeSet<String>) -> BTreeSet<String> {
    names.remove("python");
    names
}

/// Extract the package name from a PEP 508 requirement string.
///
/// The name is the longest leading run of `[A-Za-z0-9._-]`; everything after
/// it (extras, version specifiers, markers) is irrelevant here. Entries with
/// no leading name (malformed requirements) yield `None`.
fn requirement_name(requirement: &str) -> Option<&str> {
    let trimmed = requirement.trim_start();
    let end = trimmed
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
        .unwrap_or(trimmed.len());
    let name = &trimmed[..end];
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn names(manifest_deps: impl Iterator<Item = impl Into<String>>) -> Vec<String> {
        manifest_deps.map(Into::into).collect()
    }

    #[test]
    fn poetry_main_dependencies() {
        let manifest = Manifest::parse(
            r#"
[tool.poetry.dependencies]
python = "^3.11"
requests = "^2.31"
numpy = "^1.26"
"#,
        )
        .unwrap();
        assert_eq!(
            names(manifest.main_dependencies()),
            vec!["numpy", "requests"]
        );
        assert_eq!(names(manifest.dev_dependencies()), Vec::<String>::new());
    }

    #[test]
    fn pep621_project_dependencies() {
        let manifest = Manifest::parse(
            r#"
[project]
name = "demo"
dependencies = [
    "requests[security]>=2.31 ; python_version >= '3.9'",
    "numpy",
    "python-dateutil~=2.9",
]
"#,
        )
        .unwrap();
        assert_eq!(
            names(manifest.main_dependencies()),
            vec!["numpy", "python-dateutil", "requests"]
        );
    }

    #[test]
    fn poetry_table_takes_precedence_over_project_array() {
        let manifest = Manifest::parse(
            r#"
[project]
dependencies = ["from-project"]

[tool.poetry.dependencies]
from-poetry = "^1.0"
"#,
        )
        .unwrap();
        assert_eq!(names(manifest.main_dependencies()), vec!["from-poetry"]);
    }

    #[test]
    fn missing_dependency_declarations_is_an_error() {
        assert!(Manifest::parse("[tool.black]\nline-length = 100\n").is_err());
    }

    #[test]
    fn malformed_toml_is_an_error() {
        assert!(Manifest::parse("not toml [").is_err());
    }

    #[test]
    fn legacy_dev_dependencies_table() {
        let manifest = Manifest::parse(
            r#"
[tool.poetry.dependencies]
requests = "^2.31"

[tool.poetry.dev-dependencies]
pytest = "^8.0"
"#,
        )
        .unwrap();
        assert_eq!(names(manifest.dev_dependencies()), vec!["pytest"]);
    }

    #[test]
    fn dev_group_dependencies_table() {
        let manifest = Manifest::parse(
            r#"
[tool.poetry.dependencies]
requests = "^2.31"

[tool.poetry.group.dev.dependencies]
pytest = "^8.0"
mypy = "^1.10"
"#,
        )
        .unwrap();
        assert_eq!(names(manifest.dev_dependencies()), vec!["mypy", "pytest"]);
    }

    #[test]
    fn python_is_never_a_dependency() {
        let manifest = Manifest::parse(
            r#"
[tool.poetry.dependencies]
python = "^3.11"
"#,
        )
        .unwrap();
        assert_eq!(names(manifest.main_dependencies()), Vec::<String>::new());
    }

    #[test]
    fn requirement_name_extraction() {
        assert_eq!(requirement_name("requests"), Some("requests"));
        assert_eq!(requirement_name("requests>=2.31"), Some("requests"));
        assert_eq!(
            requirement_name("requests[socks,security]"),
            Some("requests")
        );
        assert_eq!(requirement_name("  requests == 2.31"), Some("requests"));
        assert_eq!(
            requirement_name("python-dateutil~=2.9"),
            Some("python-dateutil")
        );
        assert_eq!(requirement_name("ruamel.yaml"), Some("ruamel.yaml"));
        assert_eq!(requirement_name(""), None);
        assert_eq!(requirement_name(">=2.0"), None);
    }
}
