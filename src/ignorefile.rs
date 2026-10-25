//! The ignorefile: packages the user never wants reported.

use std::collections::BTreeSet;
use std::path::Path;

use tracing::debug;

use crate::infra::fs::FileSystem;
use crate::package::PackageName;

pub const IGNORE_FILE: &str = ".pyprojectudepsignore";
/// The pre-rename ignorefile name, still honored so existing projects keep
/// working. The new name wins when both exist.
pub const LEGACY_IGNORE_FILE: &str = ".poetryudepsignore";

/// The packages listed in the project's ignorefile.
///
/// Each line names one package in any spelling. Surrounding whitespace,
/// empty lines, and `#` comment lines are skipped. A missing or unreadable
/// ignorefile is simply an empty set.
pub fn ignored_packages(fs: &FileSystem) -> BTreeSet<PackageName> {
    for path in [IGNORE_FILE, LEGACY_IGNORE_FILE] {
        if let Ok(contents) = fs.read_to_string_lossy(Path::new(path)) {
            let ignored = parse(&contents);
            debug!(?ignored, path);
            return ignored;
        }
    }
    BTreeSet::new()
}

fn parse(contents: &str) -> BTreeSet<PackageName> {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !(line.is_empty() || line.starts_with('#')))
        .map(PackageName::new)
        .collect()
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn names(names: &[&str]) -> BTreeSet<PackageName> {
        names.iter().copied().map(PackageName::new).collect()
    }

    #[test]
    fn parses_names_skipping_comments_and_blanks() {
        let contents = "# tools we run via CLI only\n\npytest\n  # indented comment\n  \nruff  \n";
        assert_eq!(parse(contents), names(&["pytest", "ruff"]));
    }

    #[test]
    fn any_spelling_names_the_package() {
        assert_eq!(
            parse("PyYAML\nruamel_yaml\n"),
            names(&["pyyaml", "ruamel.yaml"])
        );
    }

    #[test]
    fn missing_ignorefile_is_an_empty_set() {
        let fs = FileSystem::create_null([] as [(&str, &str); 0]);
        assert_eq!(ignored_packages(&fs), BTreeSet::new());
    }

    #[test]
    fn reads_the_ignorefile_from_the_project_root() {
        let fs = FileSystem::create_null([(IGNORE_FILE, "pytest\n")]);
        assert_eq!(ignored_packages(&fs), names(&["pytest"]));
    }

    #[test]
    fn falls_back_to_the_legacy_ignorefile_name() {
        let fs = FileSystem::create_null([(LEGACY_IGNORE_FILE, "pytest\n")]);
        assert_eq!(ignored_packages(&fs), names(&["pytest"]));
    }

    #[test]
    fn new_ignorefile_name_wins_over_the_legacy_one() {
        let fs = FileSystem::create_null([
            (IGNORE_FILE, "from-new\n"),
            (LEGACY_IGNORE_FILE, "from-legacy\n"),
        ]);
        assert_eq!(ignored_packages(&fs), names(&["from-new"]));
    }
}

#[cfg(test)]
mod properties {
    use super::*;
    use crate::testgen::identifier;
    use hegel::generators;

    // P11: whatever mix of names, comments, and blank lines the file holds,
    // parsing returns exactly the names.
    #[hegel::test]
    fn parse_returns_exactly_the_listed_names(tc: hegel::TestCase) {
        let names: Vec<String> = tc.draw(generators::vecs(identifier()).max_size(8));
        let mut contents = String::new();
        for name in &names {
            if tc.draw(generators::booleans()) {
                contents.push_str("# a comment\n");
            }
            if tc.draw(generators::booleans()) {
                contents.push('\n');
            }
            contents.push_str(name);
            contents.push('\n');
        }
        let expected: BTreeSet<PackageName> =
            names.iter().map(|name| PackageName::new(name)).collect();
        assert_eq!(parse(&contents), expected);
    }
}
