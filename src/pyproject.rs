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
    /// Dependencies are the union of every place a tool can declare them, so
    /// a wrong guess about which tool manages the project can never drop a
    /// declaration. Main dependencies: `[tool.poetry.dependencies]`, PEP 621
    /// `[project].dependencies`, and every `[project.optional-dependencies]`
    /// extra. Dev dependencies: `[tool.poetry.dev-dependencies]` (poetry
    /// < 1.2), every `[tool.poetry.group.*.dependencies]` group, every PEP
    /// 735 `[dependency-groups]` group, and the legacy `[tool.uv]`
    /// dev-dependencies array. A manifest declaring dependencies nowhere at
    /// all is an error, since that usually means the tool was run outside a
    /// project root. The `python` interpreter requirement is never a
    /// dependency.
    pub fn parse(source: &str) -> Result<Manifest> {
        let value = Value::Table(source.parse::<toml::Table>()?);

        let mut main = BTreeSet::new();
        let mut declares_dependencies = false;

        if let Some(table) = poetry_table(&value, "dependencies") {
            main.extend(table_keys(table));
            declares_dependencies = true;
        }
        if let Some(array) = project_array(&value, "dependencies") {
            main.extend(requirement_names(array));
            declares_dependencies = true;
        }
        if let Some(extras) = lookup_table(&value, &["project", "optional-dependencies"]) {
            for array in extras.values().filter_map(Value::as_array) {
                main.extend(requirement_names(array));
            }
            declares_dependencies = true;
        }

        let mut dev = BTreeSet::new();

        if let Some(table) = poetry_table(&value, "dev-dependencies") {
            dev.extend(table_keys(table));
            declares_dependencies = true;
        }
        if let Some(groups) = lookup_table(&value, &["tool", "poetry", "group"]) {
            for group in groups.values() {
                if let Some(table) = group.get("dependencies").and_then(Value::as_table) {
                    dev.extend(table_keys(table));
                    declares_dependencies = true;
                }
            }
        }
        if let Some(groups) = lookup_table(&value, &["dependency-groups"]) {
            // Every group is unioned, so `{include-group = "..."}` entries
            // need no resolution: whatever they include is already counted.
            for array in groups.values().filter_map(Value::as_array) {
                dev.extend(requirement_names(array));
            }
            declares_dependencies = true;
        }
        if let Some(array) = lookup_table(&value, &["tool", "uv"])
            .and_then(|uv| uv.get("dev-dependencies"))
            .and_then(Value::as_array)
        {
            dev.extend(requirement_names(array));
            declares_dependencies = true;
        }

        if !declares_dependencies {
            bail!("no dependency declarations found in pyproject.toml");
        }
        if dev.is_empty() {
            info!("no dev dependencies found in pyproject.toml");
        }

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

fn lookup_table<'a>(value: &'a Value, path: &[&str]) -> Option<&'a toml::Table> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    current.as_table()
}

fn poetry_table<'a>(value: &'a Value, key: &str) -> Option<&'a toml::Table> {
    lookup_table(value, &["tool", "poetry", key])
}

fn table_keys(table: &toml::Table) -> BTreeSet<String> {
    table.keys().cloned().collect()
}

fn project_array<'a>(value: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
    value
        .get("project")
        .and_then(|project| project.get(key))
        .and_then(Value::as_array)
}

/// Package names from an array of PEP 508 requirement strings. Non-string
/// entries (e.g. PEP 735 include-group tables) are skipped.
fn requirement_names(array: &[Value]) -> BTreeSet<String> {
    array
        .iter()
        .filter_map(Value::as_str)
        .filter_map(requirement_name)
        .map(str::to_owned)
        .collect()
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
    fn poetry_and_project_dependencies_are_unioned() {
        let manifest = Manifest::parse(
            r#"
[project]
dependencies = ["from-project"]

[tool.poetry.dependencies]
from-poetry = "^1.0"
"#,
        )
        .unwrap();
        assert_eq!(
            names(manifest.main_dependencies()),
            vec!["from-poetry", "from-project"]
        );
    }

    #[test]
    fn optional_dependency_extras_count_as_main() {
        let manifest = Manifest::parse(
            r#"
[project]
dependencies = ["requests"]

[project.optional-dependencies]
plot = ["matplotlib>=3.8"]
excel = ["openpyxl", "xlsxwriter"]
"#,
        )
        .unwrap();
        assert_eq!(
            names(manifest.main_dependencies()),
            vec!["matplotlib", "openpyxl", "requests", "xlsxwriter"]
        );
    }

    #[test]
    fn all_poetry_groups_count_as_dev() {
        let manifest = Manifest::parse(
            r#"
[tool.poetry.dependencies]
requests = "^2.31"

[tool.poetry.group.dev.dependencies]
pytest = "^8.0"

[tool.poetry.group.docs.dependencies]
mkdocs = "^1.6"
"#,
        )
        .unwrap();
        assert_eq!(names(manifest.dev_dependencies()), vec!["mkdocs", "pytest"]);
    }

    #[test]
    fn pep735_dependency_groups_count_as_dev() {
        let manifest = Manifest::parse(
            r#"
[project]
dependencies = ["requests"]

[dependency-groups]
test = ["pytest>=8", "coverage"]
lint = ["ruff"]
all = [{include-group = "test"}, {include-group = "lint"}]
"#,
        )
        .unwrap();
        assert_eq!(
            names(manifest.dev_dependencies()),
            vec!["coverage", "pytest", "ruff"]
        );
    }

    #[test]
    fn cyclic_include_groups_parse_fine() {
        let manifest = Manifest::parse(
            r#"
[project]
dependencies = []

[dependency-groups]
a = ["pytest", {include-group = "b"}]
b = [{include-group = "a"}, "ruff"]
"#,
        )
        .unwrap();
        assert_eq!(names(manifest.dev_dependencies()), vec!["pytest", "ruff"]);
    }

    #[test]
    fn legacy_tool_uv_dev_dependencies_count_as_dev() {
        let manifest = Manifest::parse(
            r#"
[project]
dependencies = ["requests"]

[tool.uv]
dev-dependencies = ["pytest>=8"]
"#,
        )
        .unwrap();
        assert_eq!(names(manifest.dev_dependencies()), vec!["pytest"]);
    }

    #[test]
    fn dependency_groups_alone_are_a_valid_manifest() {
        let manifest = Manifest::parse(
            r#"
[dependency-groups]
dev = ["pytest"]
"#,
        )
        .unwrap();
        assert_eq!(names(manifest.main_dependencies()), Vec::<String>::new());
        assert_eq!(names(manifest.dev_dependencies()), vec!["pytest"]);
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
    fn hybrid_layout_unions_everything() {
        let manifest = Manifest::parse(
            r#"
[project]
dependencies = ["alpha"]

[project.optional-dependencies]
extra = ["bravo"]

[tool.poetry.dependencies]
charlie = "^1.0"

[tool.poetry.group.dev.dependencies]
delta = "^1.0"

[dependency-groups]
test = ["echo"]

[tool.uv]
dev-dependencies = ["foxtrot"]
"#,
        )
        .unwrap();
        assert_eq!(
            names(manifest.main_dependencies()),
            vec!["alpha", "bravo", "charlie"]
        );
        assert_eq!(
            names(manifest.dev_dependencies()),
            vec!["delta", "echo", "foxtrot"]
        );
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

#[cfg(test)]
mod properties {
    use super::*;
    use crate::testgen::identifier;
    use hegel::generators;
    use pretty_assertions::assert_eq;
    use std::fmt::Write;

    // P4: whichever table layout declares them, parsing returns exactly the
    // declared names in the right bucket.
    #[hegel::test]
    fn declared_names_are_extracted_from_any_layout(tc: hegel::TestCase) {
        // Rendered as TOML table keys, where duplicates are a parse error.
        let poetry_main: Vec<String> =
            tc.draw(generators::vecs(identifier()).unique(true).max_size(4));
        let poetry_group: Vec<String> =
            tc.draw(generators::vecs(identifier()).unique(true).max_size(4));
        // Rendered as array entries, where duplicates are fine.
        let project_main: Vec<String> = tc.draw(generators::vecs(identifier()).max_size(4));
        let extras: Vec<String> = tc.draw(generators::vecs(identifier()).max_size(4));
        let pep735: Vec<String> = tc.draw(generators::vecs(identifier()).max_size(4));

        let mut source = String::from("[project]\ndependencies = [\n");
        for name in &project_main {
            writeln!(source, "    \"{name}>=1.0\",").unwrap();
        }
        source.push_str("]\n\n[project.optional-dependencies]\nextra = [\n");
        for name in &extras {
            writeln!(source, "    \"{name}\",").unwrap();
        }
        source.push_str("]\n\n[tool.poetry.dependencies]\n");
        for name in &poetry_main {
            writeln!(source, "{name} = \"^1.0\"").unwrap();
        }
        source.push_str("\n[tool.poetry.group.lint.dependencies]\n");
        for name in &poetry_group {
            writeln!(source, "{name} = \"^1.0\"").unwrap();
        }
        source.push_str("\n[dependency-groups]\ntest = [\n");
        for name in &pep735 {
            writeln!(source, "    \"{name}\",").unwrap();
        }
        source.push_str("]\n");

        let manifest = Manifest::parse(&source).unwrap();

        let mut expected_main: BTreeSet<String> = poetry_main.into_iter().collect();
        expected_main.extend(project_main);
        expected_main.extend(extras);
        expected_main.remove("python");
        let mut expected_dev: BTreeSet<String> = poetry_group.into_iter().collect();
        expected_dev.extend(pep735);
        expected_dev.remove("python");

        let main: BTreeSet<String> = manifest.main_dependencies().map(str::to_owned).collect();
        let dev: BTreeSet<String> = manifest.dev_dependencies().map(str::to_owned).collect();
        assert_eq!(main, expected_main);
        assert_eq!(dev, expected_dev);
    }

    // P5: arbitrary include-group graphs, including cycles and dangling
    // references, parse fine and yield exactly the union of string entries.
    #[hegel::test]
    fn include_group_graphs_always_yield_the_union(tc: hegel::TestCase) {
        let group_count = tc.draw(generators::integers::<usize>().min_value(1).max_value(4));
        let group_names: Vec<String> = (0..group_count).map(|i| format!("group{i}")).collect();

        let mut source = String::from("[project]\ndependencies = []\n\n[dependency-groups]\n");
        let mut expected = BTreeSet::new();
        for name in &group_names {
            write!(source, "{name} = [").unwrap();
            let entries = tc.draw(generators::integers::<usize>().min_value(0).max_value(3));
            for _ in 0..entries {
                if tc.draw(generators::booleans()) {
                    let package = tc.draw(identifier());
                    write!(source, "\"{package}\", ").unwrap();
                    expected.insert(package);
                } else {
                    // Reference any group, including self and nonexistent.
                    let target = tc.draw(generators::integers::<usize>().min_value(0).max_value(5));
                    write!(source, "{{include-group = \"group{target}\"}}, ").unwrap();
                }
            }
            source.push_str("]\n");
        }
        expected.remove("python");

        let manifest = Manifest::parse(&source).unwrap();
        let dev: BTreeSet<String> = manifest.dev_dependencies().map(str::to_owned).collect();
        assert_eq!(dev, expected);
    }
}
