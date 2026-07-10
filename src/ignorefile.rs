//! The ignorefile: packages the user never wants reported.

use std::collections::BTreeSet;
use std::path::Path;

use tracing::debug;

use crate::infra::fs::FileSystem;

pub const IGNORE_FILE: &str = ".poetryudepsignore";

/// The packages listed in the project's ignorefile.
///
/// Empty lines and `#` comments are skipped. A missing or unreadable
/// ignorefile is simply an empty set.
pub fn ignored_packages(fs: &FileSystem) -> BTreeSet<String> {
    let Ok(contents) = fs.read_to_string_lossy(Path::new(IGNORE_FILE)) else {
        return BTreeSet::new();
    };
    let ignored = parse(&contents);
    debug!(?ignored);
    ignored
}

fn parse(contents: &str) -> BTreeSet<String> {
    contents
        .lines()
        .filter(|line| !(line.is_empty() || line.trim_start().starts_with('#')))
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_names_skipping_comments_and_blanks() {
        let contents = "# tools we run via CLI only\n\npytest\n  # indented comment\nruff\n";
        assert_eq!(
            parse(contents),
            BTreeSet::from(["pytest".to_string(), "ruff".to_string()])
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
        assert_eq!(
            ignored_packages(&fs),
            BTreeSet::from(["pytest".to_string()])
        );
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
        let expected: BTreeSet<String> = names.into_iter().collect();
        assert_eq!(parse(&contents), expected);
    }
}
