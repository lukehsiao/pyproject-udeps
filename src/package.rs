//! Package identity.
//!
//! build.rs includes this file to check the name map's keys, so it must
//! compile with only `std`.

/// The identity of a package: its PEP 503 normalized name.
///
/// Declared names vary in case and separators (`PyYAML`, `pyyaml`,
/// `ruamel.yaml`), but every spelling names the same package on the index.
/// Normalization lowercases the name and collapses each run of `-`, `_`,
/// and `.` into one `-`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageName(String);

impl PackageName {
    #[must_use]
    pub fn new(spelling: &str) -> Self {
        let mut name = String::with_capacity(spelling.len());
        let mut prev_sep = false;
        for c in spelling.chars() {
            if matches!(c, '-' | '_' | '.') {
                if !prev_sep {
                    name.push('-');
                }
                prev_sep = true;
            } else {
                name.extend(c.to_lowercase());
                prev_sep = false;
            }
        }
        PackageName(name)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn normalizes_case_and_separator_runs() {
        for (spelling, name) in [
            ("PyYAML", "pyyaml"),
            ("ruamel.yaml", "ruamel-yaml"),
            ("Foo__Bar", "foo-bar"),
            ("a-_.b", "a-b"),
        ] {
            assert_eq!(PackageName::new(spelling).as_str(), name);
        }
    }
}
