//! Compiles `src/name_map.toml` into a perfect hash map.
//!
//! A malformed entry then fails the build instead of becoming a map entry
//! that never matches. Lookups also need no parsing at startup.

use std::env;
use std::fs;
use std::path::PathBuf;

#[path = "src/package.rs"]
mod package;

use package::PackageName;

const SOURCE: &str = "src/name_map.toml";

fn main() {
    println!("cargo::rerun-if-changed={SOURCE}");
    // The key check uses the normalization in this file, so a change to it
    // must check the keys again.
    println!("cargo::rerun-if-changed=src/package.rs");

    let text = fs::read_to_string(SOURCE).unwrap_or_else(|e| panic!("{SOURCE}: {e}"));
    // The `preserve_order` feature keeps file order, so the sort check below
    // sees the keys as written.
    let table: toml::Table = text.parse().unwrap_or_else(|e| panic!("{SOURCE}: {e}"));

    let mut map = phf_codegen::Map::new();
    let mut previous: Option<&str> = None;
    for (package, value) in &table {
        assert!(
            is_valid_package(package) && PackageName::new(package).as_str() == package,
            "{SOURCE}: `{package}` is not a PEP 503 normalized package name"
        );
        if let Some(previous) = previous {
            assert!(
                previous < package.as_str(),
                "{SOURCE}: `{package}` must sort before `{previous}`"
            );
        }
        previous = Some(package);
        map.entry(package.as_str(), format!("&{:?}", modules(package, value)));
    }

    let out =
        PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join("name_map.rs");
    fs::write(&out, map.build().to_string()).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
}

/// The module paths of one entry, checked to be non-empty, well formed, and
/// strictly sorted. Strict order also rules out duplicates.
fn modules<'a>(package: &str, value: &'a toml::Value) -> Vec<&'a str> {
    let modules: Option<Vec<&str>> = value
        .as_array()
        .and_then(|array| array.iter().map(toml::Value::as_str).collect());
    let modules = modules
        .filter(|modules| !modules.is_empty())
        .unwrap_or_else(|| panic!("{SOURCE}: `{package}` must be a non-empty array of strings"));
    for module in &modules {
        assert!(
            is_module_path(module),
            "{SOURCE}: `{package}`: `{module}` is not a dotted module path"
        );
    }
    assert!(
        modules.is_sorted_by(|a, b| a < b),
        "{SOURCE}: `{package}`: modules must be sorted and unique"
    );
    modules
}

/// A valid project name per PEP 508: ASCII letters, digits, `-`, `_`, and
/// `.`, starting and ending with a letter or digit.
fn is_valid_package(name: &str) -> bool {
    name.bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && name
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && name
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
}

/// ASCII identifiers joined by dots. Python accepts non-ASCII identifiers,
/// but no audited package defines a module with one.
fn is_module_path(path: &str) -> bool {
    path.split('.').all(|part| {
        part.bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
            && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}
