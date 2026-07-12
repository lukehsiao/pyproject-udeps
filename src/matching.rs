//! Matching imported modules against declared dependencies.
//!
//! This module hides the heuristics that bridge the gap between `PyPI`
//! package names and Python module names: the curated [`KNOWN_NAMES`] map, the
//! dash-to-underscore convention, and the namespace-package patterns
//! (google-cloud-*, dbt adapters, sqlalchemy extensions).

use std::collections::{BTreeMap, BTreeSet};

use crate::imports::Import;
use crate::name_map::KNOWN_NAMES;

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
            keys.entry(package.clone())
                .or_default()
                .insert(package.clone());
            for alias in import_names_for(&package) {
                keys.entry(alias).or_default().insert(package.clone());
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

/// PEP 503 style normalization: lowercase, with runs of `-`, `_`, and `.`
/// collapsed to a single `-`. Declared names vary in case and separators
/// (`PyYAML`, `pyyaml`, `ruamel.yaml`), but all spellings identify the same
/// package on the index, so the name map is keyed by the normalized form.
fn normalize(package: &str) -> String {
    let mut out = String::with_capacity(package.len());
    let mut prev_sep = false;
    for c in package.chars() {
        if matches!(c, '-' | '_' | '.') {
            if !prev_sep {
                out.push('-');
            }
            prev_sep = true;
        } else {
            out.extend(c.to_lowercase());
            prev_sep = false;
        }
    }
    out
}

/// The import names a package is known or likely to use, when they differ
/// from the package name itself.
fn import_names_for(package: &str) -> Vec<String> {
    let normalized = normalize(package);
    let mut names = Vec::new();
    if let Some(known) = KNOWN_NAMES.get(normalized.as_str()) {
        names.push((*known).to_string());
    }
    // Modules conventionally swap dashes for underscores (python-dateutil is
    // an exception handled by the map). Lowercasing also lets a declared
    // "Flask" match `import flask` without a dedicated map entry.
    let underscored = normalized.replace('-', "_");
    if underscored != package {
        names.push(underscored);
    }
    names
}

/// Package names an import could correspond to.
fn candidate_keys(import: &Import) -> Vec<String> {
    let module = import.module.as_str();
    let mut candidates = Vec::new();

    if let Some(item) = &import.item {
        // Google-style namespace packages:
        // `from google.cloud import bigquery` -> google-cloud-bigquery
        candidates.push(format!("{}-{item}", module.replace('.', "-")));
    }

    let parts: Vec<&str> = module.split('.').collect();

    // DBT adapters: dbt.adapters.postgres -> dbt-postgres. A bare
    // `import dbt.adapters` has no adapter segment.
    if module.starts_with("dbt.adapters") && parts.len() >= 3 {
        candidates.push([parts[0], parts[2]].join("-"));
    }

    // Every dotted-prefix join, so a deep import like
    // `from airflow.providers.common.sql.hooks.sql import X` can match a
    // package keyed as airflow-providers-common-sql, not just the full path
    // (sqlalchemy.ext -> sqlalchemy-ext) or the two-level parent
    // (`from google.auth.transport import requests` -> google-auth).
    if parts.len() >= 2 {
        for k in 2..=parts.len() {
            candidates.push(parts[..k].join("-"));
        }
        // The top-level package: google.cloud -> google
        candidates.push((*parts.first().expect("split is never empty")).to_string());
    }

    candidates.push(module.to_string());
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
    fn known_names_alias_matches() {
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
        // The map points apache-airflow-providers-common-sql at the
        // airflow-providers-common-sql join, which a deep import must
        // produce even when the module path continues past it.
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

    // Independent re-statement of the alias rule, kept deliberately simple so
    // it can serve as an oracle for the index's bookkeeping.
    fn key_set(package: &str) -> BTreeSet<String> {
        let mut keys = BTreeSet::from([package.to_string()]);
        let normalized = normalize(package);
        if let Some(known) = crate::name_map::KNOWN_NAMES.get(normalized.as_str()) {
            keys.insert((*known).to_string());
        }
        let underscored = normalized.replace('-', "_");
        if underscored != package {
            keys.insert(underscored);
        }
        keys
    }

    #[hegel::composite]
    fn package_name(tc: hegel::TestCase) -> String {
        tc.draw(generators::from_regex("[a-zA-Z0-9][a-zA-Z0-9._-]{0,12}").fullmatch(true))
    }

    #[hegel::composite]
    fn arbitrary_import(tc: hegel::TestCase) -> Import {
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

    // P6: a dependency that is imported under any of its derivable names is
    // never reported unused.
    #[hegel::test]
    fn declared_and_imported_is_never_reported(tc: TestCase) {
        let package = tc.draw(package_name());
        let mut idx = DependencyIndex::new([package.clone()]);
        // Import it by any name the alias rule derives for it.
        let by: Vec<String> = key_set(&package).into_iter().collect();
        let module = tc.draw(generators::sampled_from(by));
        idx.mark_used(&Import { module, item: None });
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

    // P9: no import, however malformed, can panic the index. This is the
    // property that would have caught the dbt.adapters parts[2] panic.
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
                if !key_set(package).is_disjoint(&candidates) {
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
        hegel::stateful::run(machine, tc);
    }
}
