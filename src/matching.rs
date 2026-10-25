//! Matching imported modules against declared dependencies.
//!
//! This module hides the heuristics that bridge the gap between `PyPI`
//! package names and Python module names: the curated [`KNOWN_MODULES`]
//! map, the dash-to-underscore convention, and the namespace-package
//! patterns (google-cloud-*, dbt adapters).

use std::collections::{BTreeMap, BTreeSet};

use crate::imports::Import;
use crate::package::PackageName;

/// The modules a package provides, for packages whose modules the naming
/// convention in [`import_names_for`] cannot derive.
///
/// Keys are [`PackageName`]s. build.rs generates this map from
/// `src/name_map.toml`, which documents the format.
// phf_codegen writes its hash key as one long literal.
#[expect(clippy::unreadable_literal)]
static KNOWN_MODULES: phf::Map<&str, &[&str]> = include!(concat!(env!("OUT_DIR"), "/name_map.rs"));

/// Tracks which declared dependencies have been seen in an import.
///
/// Match keys are kept separate from the set of unmatched originals, so an
/// alias colliding with another declared package's name cannot clobber it,
/// and one key may satisfy several packages.
#[derive(Debug, Clone)]
pub struct DependencyIndex {
    /// Candidate match key -> original package names it satisfies.
    keys: BTreeMap<String, BTreeSet<String>>,
    /// Original package names not yet seen in any import.
    remaining: BTreeSet<String>,
}

impl DependencyIndex {
    pub fn new(packages: impl IntoIterator<Item = String>) -> Self {
        let mut keys: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut remaining = BTreeSet::new();
        for package in packages {
            for key in import_names_for(&package) {
                keys.entry(key).or_default().insert(package.clone());
            }
            remaining.insert(package);
        }
        DependencyIndex { keys, remaining }
    }

    /// Mark every dependency this import could refer to as used.
    ///
    /// Returns the packages that were newly marked, for logging. Marking is
    /// idempotent; an import that matches nothing returns an empty vec.
    pub fn mark_used(&mut self, import: &Import) -> Vec<String> {
        let mut newly_used = Vec::new();
        for candidate in candidate_keys(import) {
            if let Some(packages) = self.keys.get(&candidate) {
                for package in packages {
                    if self.remaining.remove(package) {
                        newly_used.push(package.clone());
                    }
                }
            }
        }
        newly_used
    }

    /// Declared dependencies never seen in an import, sorted.
    pub fn unused(&self) -> Vec<String> {
        self.remaining.iter().cloned().collect()
    }
}

/// The match keys under which an import marks `package` as used: its own
/// spellings, the module names the naming convention derives, and the
/// modules [`KNOWN_MODULES`] lists for it.
fn import_names_for(package: &str) -> BTreeSet<String> {
    let name = PackageName::new(package);
    let known = KNOWN_MODULES
        .get(name.as_str())
        .copied()
        .unwrap_or_default();
    // `candidate_keys` yields the dash-joined form of a module path for
    // every import at or below it, so `a-b` matches `import a.b`,
    // `import a.b.c`, `from a import b`, and `from a.b import c`.
    let mut names: BTreeSet<String> = known
        .iter()
        .map(|module| module.replace('.', "-"))
        .collect();
    // Modules conventionally swap dashes for underscores (python-dateutil is
    // an exception handled by the map). Lowercasing also lets a declared
    // "Flask" match `import flask` without a dedicated map entry.
    names.insert(name.as_str().replace('-', "_"));
    // The dashed form matches namespace imports for any spelling, so a
    // declared `Google_Auth` matches `import google.auth` as `google-auth`
    // does.
    names.insert(name.as_str().to_owned());
    // The declared spelling itself matches an import with the same case,
    // such as `PySide6`.
    names.insert(package.to_string());
    names
}

/// Package names an import could correspond to.
fn candidate_keys(import: &Import) -> Vec<String> {
    let module = import.module.as_str();
    // `from a.b import c` may import the module a.b.c, as in
    // `from google.cloud import bigquery`, so the joins span the item too.
    let parts: Vec<&str> = module.split('.').chain(import.item.as_deref()).collect();
    let mut candidates = Vec::new();

    // DBT adapters: dbt.adapters.postgres -> dbt-postgres. A bare
    // `import dbt.adapters` has no adapter segment.
    if let ["dbt", "adapters", adapter, ..] = parts[..] {
        candidates.push(format!("dbt-{adapter}"));
    }

    // Every dotted-prefix join, so a deep import like
    // `from airflow.providers.common.sql.hooks.sql import X` matches the
    // airflow-providers-common-sql key, and
    // `from google.auth.transport import requests` matches google-auth.
    for k in 2..=parts.len() {
        candidates.push(parts[..k].join("-"));
    }
    // The top-level package (google.cloud -> google), and the module as
    // written, which a declared spelling such as `ruamel.yaml` matches. An
    // undotted module is its own top-level package.
    candidates.push(parts[0].to_string());
    if module != parts[0] {
        candidates.push(module.to_string());
    }
    candidates
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn index(packages: &[&str]) -> DependencyIndex {
        DependencyIndex::new(packages.iter().map(ToString::to_string))
    }

    fn plain(module: &str) -> Import {
        Import {
            module: module.to_string(),
            item: None,
        }
    }

    fn from(module: &str, item: &str) -> Import {
        Import {
            module: module.to_string(),
            item: Some(item.to_string()),
        }
    }

    #[test]
    fn exact_name_matches() {
        let mut idx = index(&["requests", "numpy"]);
        assert_eq!(idx.mark_used(&plain("requests")), vec!["requests"]);
        assert_eq!(idx.unused(), vec!["numpy"]);
    }

    #[test]
    fn known_module_matches() {
        let mut idx = index(&["scikit-learn"]);
        idx.mark_used(&plain("sklearn"));
        assert_eq!(idx.unused(), Vec::<String>::new());
    }

    #[test]
    fn dash_to_underscore_alias_matches() {
        let mut idx = index(&["typing-extensions"]);
        idx.mark_used(&plain("typing_extensions"));
        assert_eq!(idx.unused(), Vec::<String>::new());
    }

    #[test]
    fn google_style_from_import_matches() {
        let mut idx = index(&["google-cloud-bigquery"]);
        idx.mark_used(&from("google.cloud", "bigquery"));
        assert_eq!(idx.unused(), Vec::<String>::new());
    }

    #[test]
    fn declared_case_and_separators_are_normalized() {
        let mut idx = index(&["Flask", "PyYAML", "ruamel.yaml.clib"]);
        idx.mark_used(&plain("flask"));
        idx.mark_used(&plain("yaml"));
        idx.mark_used(&plain("ruamel_yaml_clib"));
        assert_eq!(idx.unused(), Vec::<String>::new());
    }

    #[test]
    fn deep_import_matches_intermediate_prefix_join() {
        // The map lists airflow.providers.common.sql for
        // apache-airflow-providers-common-sql, and an import below that
        // module must still match.
        let mut idx = index(&["apache-airflow-providers-common-sql"]);
        idx.mark_used(&from("airflow.providers.common.sql.hooks.sql", "X"));
        assert_eq!(idx.unused(), Vec::<String>::new());
    }

    #[test]
    fn dbt_adapter_import_matches() {
        let mut idx = index(&["dbt-postgres"]);
        idx.mark_used(&plain("dbt.adapters.postgres"));
        assert_eq!(idx.unused(), Vec::<String>::new());
    }

    #[test]
    fn dbt_adapter_from_import_matches() {
        let mut idx = index(&["dbt-postgres"]);
        idx.mark_used(&from("dbt.adapters", "postgres"));
        assert_eq!(idx.unused(), Vec::<String>::new());
    }

    #[test]
    fn bare_dbt_adapters_import_matches_nothing() {
        let mut idx = index(&["dbt-postgres"]);
        assert_eq!(idx.mark_used(&plain("dbt.adapters")), Vec::<String>::new());
        assert_eq!(idx.unused(), vec!["dbt-postgres"]);
    }

    #[test]
    fn dotted_join_matches_sqlalchemy_extensions() {
        let mut idx = index(&["sqlalchemy-ext"]);
        idx.mark_used(&plain("sqlalchemy.ext"));
        assert_eq!(idx.unused(), Vec::<String>::new());
    }

    #[test]
    fn first_two_segments_match() {
        let mut idx = index(&["google-auth"]);
        idx.mark_used(&from("google.auth.transport", "requests"));
        assert_eq!(idx.unused(), Vec::<String>::new());
    }

    #[test]
    fn top_level_segment_matches() {
        let mut idx = index(&["google"]);
        idx.mark_used(&plain("google.cloud"));
        assert_eq!(idx.unused(), Vec::<String>::new());
    }

    // Regression: an alias equal to another declared package's name used to
    // clobber that package's own entry, silently un-reporting it.
    #[test]
    fn alias_collision_with_declared_package_keeps_both() {
        // `foo-bar` generates alias `foo_bar`, which is also declared.
        let mut idx = index(&["foo-bar", "foo_bar"]);
        assert_eq!(idx.unused(), vec!["foo-bar", "foo_bar"]);
        // Importing foo_bar satisfies both: the exact name and the alias.
        idx.mark_used(&plain("foo_bar"));
        assert_eq!(idx.unused(), Vec::<String>::new());
    }

    #[test]
    fn marking_is_idempotent() {
        let mut idx = index(&["requests"]);
        assert_eq!(idx.mark_used(&plain("requests")), vec!["requests"]);
        assert_eq!(idx.mark_used(&plain("requests")), Vec::<String>::new());
    }
}

#[cfg(test)]
mod properties {
    use super::*;
    use crate::testgen::{identifier, module_path};
    use hegel::TestCase;
    use hegel::generators;
    use pretty_assertions::assert_eq;

    #[hegel::composite]
    fn package_name(tc: &hegel::TestCase) -> String {
        tc.draw(generators::from_regex("[a-zA-Z0-9][a-zA-Z0-9._-]{0,12}").fullmatch(true))
    }

    #[hegel::composite]
    fn arbitrary_import(tc: &hegel::TestCase) -> Import {
        let module = tc.draw(hegel::one_of!(
            generators::from_regex("dbt\\.adapters(\\.[a-z]{1,6}){0,2}").fullmatch(true),
            module_path(),
            generators::text().max_size(20),
        ));
        let item = if tc.draw(generators::booleans()) {
            Some(tc.draw(identifier()))
        } else {
            None
        };
        Import { module, item }
    }

    /// An import at or below `module`: `import module`, `import module.x`,
    /// `from module import x`, `from module.x import y`, or, for a dotted
    /// module, `from parent import last`.
    #[hegel::composite]
    fn import_at_or_below(tc: &hegel::TestCase, module: String) -> Import {
        let (module, item) = match tc.draw(generators::integers::<u8>().max_value(4)) {
            0 => (module, None),
            1 => (format!("{module}.{}", tc.draw(module_path())), None),
            2 => (module, Some(tc.draw(identifier()))),
            3 => (
                format!("{module}.{}", tc.draw(module_path())),
                Some(tc.draw(identifier())),
            ),
            _ => match module.rsplit_once('.') {
                Some((parent, last)) => (parent.to_string(), Some(last.to_string())),
                None => (module, None),
            },
        };
        Import { module, item }
    }

    /// A declared spelling that PEP 503 normalizes to `segments.join("-")`:
    /// the lowercase segments in random case, joined by runs of separators.
    #[hegel::composite]
    fn respelled(tc: &hegel::TestCase, segments: Vec<String>) -> String {
        let mut spelling = String::new();
        for (i, segment) in segments.iter().enumerate() {
            if i > 0 {
                spelling.push_str(&tc.draw(generators::from_regex("[-_.]{1,3}").fullmatch(true)));
            }
            for c in segment.chars() {
                if tc.draw(generators::booleans()) {
                    spelling.extend(c.to_uppercase());
                } else {
                    spelling.push(c);
                }
            }
        }
        spelling
    }

    // `import a.b.c` and `from a.b import c` mark the same packages, since
    // either one may import the module a.b.c. A declared spelling with
    // uppercase and dots, such as `A.B`, matches only `import A.B` as
    // written, so the declared names here are lowercase.
    #[hegel::test]
    fn from_import_matches_like_dotted_import(tc: TestCase) {
        // A tiny alphabet makes declared names and import paths collide.
        let packages: Vec<String> = tc.draw(
            generators::vecs(
                generators::from_regex("(dbt-)?[ab]{1,2}([-_.][ab]{1,2})?").fullmatch(true),
            )
            .max_size(6),
        );
        let prefix = tc.draw(generators::sampled_from(vec![
            vec![],
            vec!["dbt", "adapters"],
        ]));
        let rest: Vec<String> = tc.draw(
            generators::vecs(generators::from_regex("[ab]{1,2}").fullmatch(true))
                .min_size(1)
                .max_size(4),
        );
        let mut segments: Vec<String> = prefix.into_iter().map(str::to_string).collect();
        segments.extend(rest);
        tc.assume(segments.len() >= 2);
        let (last, parent) = segments.split_last().expect("at least two segments");

        let mut by_dotted = DependencyIndex::new(packages.iter().cloned());
        by_dotted.mark_used(&Import {
            module: segments.join("."),
            item: None,
        });
        let mut by_from = DependencyIndex::new(packages);
        by_from.mark_used(&Import {
            module: parent.join("."),
            item: Some(last.clone()),
        });
        assert_eq!(by_dotted.unused(), by_from.unused());
    }

    // A package is used when code imports the module its name spells, with
    // dots or underscores between the name's segments, however the package
    // is declared.
    #[hegel::test]
    fn conventional_import_marks_any_spelling_used(tc: TestCase) {
        let segments: Vec<String> = tc.draw(
            generators::vecs(generators::from_regex("[a-z][a-z0-9]{0,5}").fullmatch(true))
                .min_size(1)
                .max_size(3),
        );
        let separator = tc.draw(generators::sampled_from(vec![".", "_"]));
        let module = segments.join(separator);
        let mut idx = DependencyIndex::new([tc.draw(respelled(segments))]);
        idx.mark_used(&tc.draw(import_at_or_below(module)));
        assert_eq!(idx.unused(), Vec::<String>::new());
    }

    // A package in the map is used when code imports any module the map
    // lists for it, or anything below that module, however the package is
    // declared.
    #[hegel::test]
    fn known_module_import_marks_any_spelling_used(tc: TestCase) {
        let i = tc.draw(generators::integers::<usize>().max_value(KNOWN_MODULES.len() - 1));
        let (package, modules) = KNOWN_MODULES.entries().nth(i).expect("i is in range");
        let segments = package.split('-').map(str::to_string).collect();
        let module = tc
            .draw(generators::sampled_from(modules.to_vec()))
            .to_string();
        let mut idx = DependencyIndex::new([tc.draw(respelled(segments))]);
        idx.mark_used(&tc.draw(import_at_or_below(module)));
        assert_eq!(idx.unused(), Vec::<String>::new());
    }

    // P7: imports drawn from a disjoint alphabet can never satisfy any
    // declared dependency, so everything is reported.
    #[hegel::test]
    fn never_imported_is_always_reported(tc: TestCase) {
        let packages: Vec<String> = tc.draw(
            generators::vecs(generators::from_regex("aa[a-z0-9-]{0,10}").fullmatch(true))
                .max_size(8),
        );
        let mut idx = DependencyIndex::new(packages.iter().cloned());
        let imports: Vec<String> = tc.draw(
            generators::vecs(
                generators::from_regex("zz[a-z]{1,8}(\\.zz[a-z]{1,8}){0,3}").fullmatch(true),
            )
            .max_size(8),
        );
        for module in imports {
            let marked = idx.mark_used(&Import { module, item: None });
            assert_eq!(marked, Vec::<String>::new());
        }
        let mut expected: Vec<String> = packages;
        expected.sort();
        expected.dedup();
        assert_eq!(idx.unused(), expected);
    }

    // P8: marking is idempotent and monotone, and unused() is always a
    // sorted, deduplicated subset of the declared packages.
    #[hegel::test]
    fn marking_is_idempotent_and_monotone(tc: TestCase) {
        let packages: Vec<String> = tc.draw(generators::vecs(package_name()).max_size(8));
        let declared: BTreeSet<String> = packages.iter().cloned().collect();
        let mut idx = DependencyIndex::new(packages);

        let steps = tc.draw(generators::integers::<usize>().min_value(0).max_value(10));
        for _ in 0..steps {
            let import = tc.draw(arbitrary_import());
            let before = idx.unused();
            let first = idx.mark_used(&import);
            let after_once = idx.unused();
            let second = idx.mark_used(&import);
            assert_eq!(second, Vec::<String>::new(), "second mark must be a no-op");
            assert_eq!(idx.unused(), after_once);

            // Monotone: unused only shrinks, by exactly the marked names.
            assert!(after_once.iter().all(|p| before.contains(p)));
            assert_eq!(after_once.len() + first.len(), before.len());

            // Always sorted, deduplicated, and a subset of declared.
            assert!(after_once.windows(2).all(|w| w[0] < w[1]));
            assert!(after_once.iter().all(|p| declared.contains(p)));
        }
    }

    // P9: no import, however malformed, can panic the index, including an
    // import that stops short of the dbt adapter segment.
    #[hegel::test]
    fn marking_never_panics(tc: TestCase) {
        let packages: Vec<String> = tc.draw(generators::vecs(generators::text()).max_size(5));
        let mut idx = DependencyIndex::new(packages);
        let module = tc.draw(generators::text());
        let item = if tc.draw(generators::booleans()) {
            Some(tc.draw(generators::text()))
        } else {
            None
        };
        idx.mark_used(&Import { module, item });
        idx.unused();
    }

    // P10: the index agrees with a naive reference model under any sequence
    // of operations.
    struct IndexVsModel {
        idx: DependencyIndex,
        declared: Vec<String>,
        used: BTreeSet<String>,
    }

    impl IndexVsModel {
        fn model_mark(&mut self, import: &Import) {
            let candidates: BTreeSet<String> = candidate_keys(import).into_iter().collect();
            for package in &self.declared {
                if !import_names_for(package).is_disjoint(&candidates) {
                    self.used.insert(package.clone());
                }
            }
        }
    }

    #[hegel::state_machine]
    impl IndexVsModel {
        // The state-machine macro requires rules to take TestCase by value.
        #[allow(clippy::needless_pass_by_value)]
        #[rule]
        fn mark_arbitrary(&mut self, tc: TestCase) {
            let import = tc.draw(arbitrary_import());
            self.idx.mark_used(&import);
            self.model_mark(&import);
        }

        #[allow(clippy::needless_pass_by_value)]
        #[rule]
        fn mark_declared(&mut self, tc: TestCase) {
            tc.assume(!self.declared.is_empty());
            let package = tc.draw(generators::sampled_from(self.declared.clone()));
            let import = Import {
                module: package,
                item: None,
            };
            self.idx.mark_used(&import);
            self.model_mark(&import);
        }

        #[invariant]
        fn agrees_with_model(&mut self, _: TestCase) {
            let expected: Vec<String> = self
                .declared
                .iter()
                .filter(|p| !self.used.contains(*p))
                .cloned()
                .collect::<BTreeSet<String>>()
                .into_iter()
                .collect();
            assert_eq!(self.idx.unused(), expected);
        }
    }

    #[hegel::test]
    fn index_matches_reference_model(tc: TestCase) {
        let packages: Vec<String> = tc.draw(generators::vecs(package_name()).max_size(8));
        let machine = IndexVsModel {
            idx: DependencyIndex::new(packages.iter().cloned()),
            declared: packages,
            used: BTreeSet::new(),
        };
        hegel::stateful::machine(machine).run(tc);
    }
}
