//! Matching imported modules against declared dependencies.
//!
//! This module hides the heuristics that bridge the gap between `PyPI`
//! package names and Python module names: the curated [`KNOWN_MODULES`]
//! map, the dash-to-underscore convention, and the namespace-package
//! patterns (google-cloud-*, dbt adapters).
//!
//! Both sides meet at a [`MatchKey`]: a package lists the module paths it
//! may provide, an import lists the module paths it may reach, and any
//! shared path marks the package used.

use std::borrow::{Borrow, Cow};
use std::collections::{BTreeMap, BTreeSet};

use crate::imports::Import;
use crate::package::PackageName;
use crate::pyproject::{DeclaredDependency, Group};

/// The modules a package provides, for packages whose modules the naming
/// convention in [`package_keys`] cannot derive.
///
/// Keys are [`PackageName`]s. build.rs generates this map from
/// `src/name_map.toml`, which documents the format.
// phf_codegen writes its hash key as one long literal.
#[expect(clippy::unreadable_literal)]
static KNOWN_MODULES: phf::Map<&str, &[&str]> = include!(concat!(env!("OUT_DIR"), "/name_map.rs"));

/// Which unused dependencies to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Main,
    MainAndDev,
}

/// A dotted module path that a package provides, such as `google.auth`.
///
/// Imports are dotted paths already, so the index looks up the paths an
/// import reaches as plain `str`s, without building a key for each one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct MatchKey(String);

impl Borrow<str> for MatchKey {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// Tracks which declared dependencies have been seen in an import.
///
/// Every declaration of one package collapses into a single entry, so a
/// package is reported at most once. Match keys are kept separate from the
/// unmatched entries, so an alias colliding with another declared
/// package's name cannot clobber it, and one key may satisfy several
/// packages.
#[derive(Debug, Clone)]
pub struct DependencyIndex {
    /// Candidate match key -> packages it satisfies.
    keys: BTreeMap<MatchKey, BTreeSet<PackageName>>,
    /// Packages not yet seen in any import, each with the declaration
    /// to report it by.
    remaining: BTreeMap<PackageName, DeclaredDependency>,
}

impl DependencyIndex {
    /// Index every declaration whose package is not in `ignored`.
    ///
    /// A package declared more than once is a main dependency if any
    /// declaration says so, and is reported by the spelling that sorts
    /// first among the declarations of that group.
    pub fn new<'a>(
        declared: impl IntoIterator<Item = &'a DeclaredDependency>,
        ignored: &BTreeSet<PackageName>,
    ) -> Self {
        let mut keys: BTreeMap<MatchKey, BTreeSet<PackageName>> = BTreeMap::new();
        let mut remaining: BTreeMap<PackageName, DeclaredDependency> = BTreeMap::new();
        for dep in declared {
            if ignored.contains(dep.name()) {
                continue;
            }
            for key in package_keys(dep) {
                keys.entry(key).or_default().insert(dep.name().clone());
            }
            let kept = remaining
                .entry(dep.name().clone())
                .or_insert_with(|| dep.clone());
            if (dep.group(), dep.spelling()) < (kept.group(), kept.spelling()) {
                *kept = dep.clone();
            }
        }
        DependencyIndex { keys, remaining }
    }

    /// Mark every dependency this import could refer to as used.
    ///
    /// Returns the dependencies that were newly marked, for logging.
    /// Marking is idempotent; an import that matches nothing returns an
    /// empty vec.
    pub fn mark_used(&mut self, import: &Import) -> Vec<DeclaredDependency> {
        let mut newly_used = Vec::new();
        for key in import_keys(import.as_str()) {
            if let Some(packages) = self.keys.get(key.as_ref()) {
                newly_used.extend(
                    packages
                        .iter()
                        .filter_map(|name| self.remaining.remove(name)),
                );
            }
        }
        newly_used
    }

    /// The declared spellings of the unused dependencies in `scope`, sorted
    /// by package name.
    pub fn unused(&self, scope: Scope) -> Vec<String> {
        self.remaining
            .values()
            .filter(|dep| scope == Scope::MainAndDev || dep.group() == Group::Main)
            .map(|dep| dep.spelling().to_owned())
            .collect()
    }
}

/// The module paths whose import marks `dep` as used: those in
/// [`KNOWN_MODULES`], those its name derives, and its declared spelling.
///
/// An import marks the package used when it reaches any of these paths
/// or anything below them, which [`import_keys`] takes care of.
fn package_keys(dep: &DeclaredDependency) -> impl Iterator<Item = MatchKey> {
    let name = dep.name().as_str();
    let known = KNOWN_MODULES.get(name).copied().unwrap_or_default();
    known
        .iter()
        .map(|module| (*module).to_owned())
        .chain([
            // Modules conventionally swap dashes for underscores
            // (python-dateutil is an exception handled by the map).
            // Lowercasing also lets a declared "Flask" match `import flask`
            // without a dedicated map entry.
            name.replace('-', "_"),
            // Dashes also separate namespace packages, so a declared
            // `Google_Auth` matches `import google.auth` as `google-auth`
            // does.
            name.replace('-', "."),
            // The declared spelling keeps its case, for modules such as
            // `PySide6`, and its dashes separate segments as dots do.
            dep.spelling().replace('-', "."),
        ])
        .map(MatchKey)
}

/// The module paths an import of `path` reaches: `path` and each of its
/// dotted prefixes, so `google.auth.transport.requests` reaches
/// google.auth and the top-level google.
fn import_keys(path: &str) -> impl Iterator<Item = Cow<'_, str>> {
    let prefixes = path
        .match_indices('.')
        .map(|(end, _)| &path[..end])
        .chain([path])
        .map(Cow::Borrowed);
    // dbt adapters live under dbt.adapters but are named after dbt.<name>,
    // as in dbt.adapters.postgres from dbt-postgres. A bare
    // `import dbt.adapters` has no adapter segment.
    let adapter = path.strip_prefix("dbt.adapters.").map(|rest| {
        let adapter = rest.split_once('.').map_or(rest, |(adapter, _)| adapter);
        Cow::Owned(format!("dbt.{adapter}"))
    });
    prefixes.chain(adapter)
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn declared(spellings: &[&str], group: Group) -> Vec<DeclaredDependency> {
        spellings
            .iter()
            .map(|spelling| DeclaredDependency::new((*spelling).to_owned(), group))
            .collect()
    }

    fn index(packages: &[&str]) -> DependencyIndex {
        DependencyIndex::new(&declared(packages, Group::Main), &BTreeSet::new())
    }

    fn unused(idx: &DependencyIndex) -> Vec<String> {
        idx.unused(Scope::MainAndDev)
    }

    fn mark(idx: &mut DependencyIndex, import: &Import) -> Vec<String> {
        idx.mark_used(import)
            .iter()
            .map(|dep| dep.spelling().to_owned())
            .collect()
    }

    fn import(path: &str) -> Import {
        Import::new(path)
    }

    #[test]
    fn exact_name_matches() {
        let mut idx = index(&["requests", "numpy"]);
        assert_eq!(mark(&mut idx, &import("requests")), vec!["requests"]);
        assert_eq!(unused(&idx), vec!["numpy"]);
    }

    #[test]
    fn known_module_matches() {
        let mut idx = index(&["scikit-learn"]);
        idx.mark_used(&import("sklearn"));
        assert_eq!(unused(&idx), Vec::<String>::new());
    }

    #[test]
    fn dash_to_underscore_alias_matches() {
        let mut idx = index(&["typing-extensions"]);
        idx.mark_used(&import("typing_extensions"));
        assert_eq!(unused(&idx), Vec::<String>::new());
    }

    #[test]
    fn namespace_package_module_matches() {
        let mut idx = index(&["google-cloud-bigquery"]);
        idx.mark_used(&import("google.cloud.bigquery"));
        assert_eq!(unused(&idx), Vec::<String>::new());
    }

    #[test]
    fn declared_case_and_separators_are_normalized() {
        let mut idx = index(&["Flask", "PyYAML", "ruamel.yaml.clib"]);
        idx.mark_used(&import("flask"));
        idx.mark_used(&import("yaml"));
        idx.mark_used(&import("ruamel_yaml_clib"));
        assert_eq!(unused(&idx), Vec::<String>::new());
    }

    #[test]
    fn deep_import_matches_intermediate_prefix_join() {
        // The map lists airflow.providers.common.sql for
        // apache-airflow-providers-common-sql, and an import below that
        // module must still match.
        let mut idx = index(&["apache-airflow-providers-common-sql"]);
        idx.mark_used(&import("airflow.providers.common.sql.hooks.sql.X"));
        assert_eq!(unused(&idx), Vec::<String>::new());
    }

    #[test]
    fn dbt_adapter_import_matches() {
        let mut idx = index(&["dbt-postgres"]);
        idx.mark_used(&import("dbt.adapters.postgres"));
        assert_eq!(unused(&idx), Vec::<String>::new());
    }

    #[test]
    fn bare_dbt_adapters_import_matches_nothing() {
        let mut idx = index(&["dbt-postgres"]);
        assert_eq!(
            mark(&mut idx, &import("dbt.adapters")),
            Vec::<String>::new()
        );
        assert_eq!(unused(&idx), vec!["dbt-postgres"]);
    }

    #[test]
    fn dotted_join_matches_sqlalchemy_extensions() {
        let mut idx = index(&["sqlalchemy-ext"]);
        idx.mark_used(&import("sqlalchemy.ext"));
        assert_eq!(unused(&idx), Vec::<String>::new());
    }

    #[test]
    fn first_two_segments_match() {
        let mut idx = index(&["google-auth"]);
        idx.mark_used(&import("google.auth.transport.requests"));
        assert_eq!(unused(&idx), Vec::<String>::new());
    }

    #[test]
    fn top_level_segment_matches() {
        let mut idx = index(&["google"]);
        idx.mark_used(&import("google.cloud"));
        assert_eq!(unused(&idx), Vec::<String>::new());
    }

    // Regression: an alias equal to another declared package's name used to
    // clobber that package's own entry, silently un-reporting it.
    #[test]
    fn alias_collision_with_declared_package_keeps_both() {
        // The map lists module `yaml` for pyyaml, and `yaml` is also a
        // package of its own.
        let mut idx = index(&["pyyaml", "yaml"]);
        assert_eq!(unused(&idx), vec!["pyyaml", "yaml"]);
        assert_eq!(mark(&mut idx, &import("yaml")), vec!["pyyaml", "yaml"]);
    }

    #[test]
    fn marking_is_idempotent() {
        let mut idx = index(&["requests"]);
        assert_eq!(mark(&mut idx, &import("requests")), vec!["requests"]);
        assert_eq!(mark(&mut idx, &import("requests")), Vec::<String>::new());
    }

    #[test]
    fn a_package_declared_in_main_and_dev_is_one_main_dependency() {
        let mut deps = declared(&["requests"], Group::Main);
        deps.extend(declared(&["Requests", "pytest"], Group::Dev));
        let idx = DependencyIndex::new(&deps, &BTreeSet::new());
        assert_eq!(idx.unused(Scope::Main), vec!["requests"]);
        assert_eq!(idx.unused(Scope::MainAndDev), vec!["pytest", "requests"]);
    }

    #[test]
    fn report_is_sorted_by_package_name() {
        let idx = index(&["Zope", "attrs", "PyYAML"]);
        assert_eq!(unused(&idx), vec!["attrs", "PyYAML", "Zope"]);
    }

    #[test]
    fn first_sorting_spelling_of_the_group_is_reported() {
        let mut deps = declared(&["ruamel_yaml", "ruamel.yaml"], Group::Main);
        deps.extend(declared(&["Ruamel-YAML"], Group::Dev));
        let idx = DependencyIndex::new(&deps, &BTreeSet::new());
        assert_eq!(unused(&idx), vec!["ruamel.yaml"]);
    }

    #[test]
    fn any_declared_spelling_marks_the_package_used() {
        let mut deps = declared(&["foo-bar"], Group::Main);
        deps.extend(declared(&["Foo_Bar"], Group::Dev));
        let mut idx = DependencyIndex::new(&deps, &BTreeSet::new());
        assert_eq!(mark(&mut idx, &import("Foo_Bar")), vec!["foo-bar"]);
    }

    #[test]
    fn dotted_spelling_matches_its_module_and_modules_below() {
        // Not in the name map, so only the declared spelling can match.
        let mut idx = index(&["Acme.Widgets"]);
        assert_eq!(
            mark(&mut idx, &import("Acme.Widgets")),
            vec!["Acme.Widgets"]
        );
        let mut idx = index(&["Acme.Widgets"]);
        assert_eq!(
            mark(&mut idx, &import("Acme.Widgets.core")),
            vec!["Acme.Widgets"]
        );
    }

    #[test]
    fn ignored_packages_are_never_reported() {
        let ignored = BTreeSet::from([PackageName::new("pyyaml")]);
        let idx = DependencyIndex::new(&declared(&["PyYAML", "requests"], Group::Main), &ignored);
        assert_eq!(unused(&idx), vec!["requests"]);
    }
}

#[cfg(test)]
mod properties {
    use super::*;
    use crate::testgen::module_path;
    use hegel::TestCase;
    use hegel::generators;
    use pretty_assertions::assert_eq;

    fn index_of(packages: &[String]) -> DependencyIndex {
        let declared: Vec<DeclaredDependency> = packages
            .iter()
            .map(|spelling| DeclaredDependency::new(spelling.clone(), Group::Main))
            .collect();
        DependencyIndex::new(&declared, &BTreeSet::new())
    }

    fn all_unused(idx: &DependencyIndex) -> Vec<String> {
        idx.unused(Scope::MainAndDev)
    }

    #[hegel::composite]
    fn package_name(tc: &hegel::TestCase) -> String {
        tc.draw(generators::from_regex("[a-zA-Z0-9][a-zA-Z0-9._-]{0,12}").fullmatch(true))
    }

    #[hegel::composite]
    fn arbitrary_import(tc: &hegel::TestCase) -> Import {
        let path = tc.draw(hegel::one_of!(
            generators::from_regex("dbt\\.adapters(\\.[a-z]{1,6}){0,3}").fullmatch(true),
            module_path(),
            generators::text().max_size(20),
        ));
        Import::new(&path)
    }

    /// An import of `module` or of a module below it.
    #[hegel::composite]
    fn import_at_or_below(tc: &hegel::TestCase, module: String) -> Import {
        if tc.draw(generators::booleans()) {
            Import::new(&module)
        } else {
            Import::new(&format!("{module}.{}", tc.draw(module_path())))
        }
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
        let mut idx = index_of(&[tc.draw(respelled(segments))]);
        idx.mark_used(&tc.draw(import_at_or_below(module)));
        assert_eq!(all_unused(&idx), Vec::<String>::new());
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
        let mut idx = index_of(&[tc.draw(respelled(segments))]);
        idx.mark_used(&tc.draw(import_at_or_below(module)));
        assert_eq!(all_unused(&idx), Vec::<String>::new());
    }

    // Imports drawn from a disjoint alphabet can never satisfy any
    // declared dependency, so everything is reported. The names are
    // already normalized, so each one is its own package.
    #[hegel::test]
    fn never_imported_is_always_reported(tc: TestCase) {
        let packages: Vec<String> = tc.draw(
            generators::vecs(generators::from_regex("aa[a-z0-9]{0,10}").fullmatch(true))
                .max_size(8),
        );
        let mut idx = index_of(&packages);
        let imports: Vec<String> = tc.draw(
            generators::vecs(
                generators::from_regex("zz[a-z]{1,8}(\\.zz[a-z]{1,8}){0,3}").fullmatch(true),
            )
            .max_size(8),
        );
        for module in imports {
            let marked = idx.mark_used(&Import::new(&module));
            assert_eq!(marked, Vec::new());
        }
        let mut expected: Vec<String> = packages;
        expected.sort();
        expected.dedup();
        assert_eq!(all_unused(&idx), expected);
    }

    // Marking is idempotent and monotone, and unused() is always a
    // subset of the declared spellings, sorted by distinct package names.
    #[hegel::test]
    fn marking_is_idempotent_and_monotone(tc: TestCase) {
        let packages: Vec<String> = tc.draw(generators::vecs(package_name()).max_size(8));
        let declared: BTreeSet<String> = packages.iter().cloned().collect();
        let mut idx = index_of(&packages);

        let steps = tc.draw(generators::integers::<usize>().min_value(0).max_value(10));
        for _ in 0..steps {
            let import = tc.draw(arbitrary_import());
            let before = all_unused(&idx);
            let first = idx.mark_used(&import);
            let after_once = all_unused(&idx);
            let second = idx.mark_used(&import);
            assert_eq!(second, Vec::new(), "second mark must be a no-op");
            assert_eq!(all_unused(&idx), after_once);

            // Monotone: unused only shrinks, by exactly the marked packages.
            assert!(after_once.iter().all(|p| before.contains(p)));
            assert_eq!(after_once.len() + first.len(), before.len());

            // Always sorted by distinct package names, and declared.
            assert!(
                after_once
                    .windows(2)
                    .all(|w| PackageName::new(&w[0]) < PackageName::new(&w[1]))
            );
            assert!(after_once.iter().all(|p| declared.contains(p)));
        }
    }

    // No import, however malformed, can panic the index, including an
    // import that stops short of the dbt adapter segment.
    #[hegel::test]
    fn marking_never_panics(tc: TestCase) {
        let packages: Vec<String> = tc.draw(generators::vecs(generators::text()).max_size(5));
        let mut idx = index_of(&packages);
        idx.mark_used(&Import::new(&tc.draw(generators::text())));
        idx.unused(Scope::MainAndDev);
    }

    // The index agrees with a reference model under any sequence of
    // operations. The model works on lists of path segments and never calls
    // the key functions, so it catches the two sides of a match disagreeing
    // about the key format.
    struct IndexVsModel {
        idx: DependencyIndex,
        declared: Vec<DeclaredDependency>,
        used: BTreeSet<PackageName>,
    }

    impl IndexVsModel {
        /// Mark every package that provides a module at or above a path the
        /// import reaches. A package provides the modules the name map lists
        /// for it, its name's dash-separated segments, its name with
        /// underscores as one module, and its declared spelling with dashes
        /// or dots between segments.
        fn model_mark(&mut self, import: &Import) {
            let path: Vec<&str> = import.as_str().split('.').collect();
            let mut reached = vec![path.clone()];
            if let ["dbt", "adapters", adapter, ..] = path[..] {
                reached.push(vec!["dbt", adapter]);
            }
            for dep in &self.declared {
                let name = dep.name().as_str();
                let underscored = name.replace('-', "_");
                let mut provided: Vec<Vec<&str>> = vec![
                    name.split('-').collect(),
                    vec![underscored.as_str()],
                    dep.spelling().split(['-', '.']).collect(),
                ];
                provided.extend(
                    KNOWN_MODULES
                        .get(name)
                        .into_iter()
                        .flat_map(|modules| modules.iter())
                        .map(|module| module.split('.').collect()),
                );
                if provided
                    .iter()
                    .any(|p| reached.iter().any(|r| r.starts_with(p)))
                {
                    self.used.insert(dep.name().clone());
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
            let import = Import::new(package.spelling());
            self.idx.mark_used(&import);
            self.model_mark(&import);
        }

        #[invariant]
        fn agrees_with_model(&mut self, _: TestCase) {
            // Each unused package, by the smallest of its spellings.
            let mut expected: BTreeMap<&PackageName, &str> = BTreeMap::new();
            for dep in self
                .declared
                .iter()
                .filter(|dep| !self.used.contains(dep.name()))
            {
                let spelling = expected.entry(dep.name()).or_insert(dep.spelling());
                *spelling = (*spelling).min(dep.spelling());
            }
            let expected: Vec<&str> = expected.into_values().collect();
            assert_eq!(all_unused(&self.idx), expected);
        }
    }

    #[hegel::test]
    fn index_matches_reference_model(tc: TestCase) {
        let packages: Vec<String> = tc.draw(generators::vecs(package_name()).max_size(8));
        let machine = IndexVsModel {
            idx: index_of(&packages),
            declared: packages
                .into_iter()
                .map(|spelling| DeclaredDependency::new(spelling, Group::Main))
                .collect(),
            used: BTreeSet::new(),
        };
        hegel::stateful::machine(machine).run(tc);
    }
}
