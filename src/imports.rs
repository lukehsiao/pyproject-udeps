//! Import extraction from Python source.
//!
//! Parsing is delegated to ruff's error-resilient parser: `parse_unchecked`
//! never fails, it produces a best-effort AST plus a list of syntax errors.
//! Walking the AST (instead of scanning text) means imports inside functions,
//! `try`/`except ImportError` blocks, and `if TYPE_CHECKING` sections are all
//! found, while text inside string literals, docstrings, and comments is not.

use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{self as ast, Expr, PySourceType, Stmt};
use ruff_python_parser::parse_unchecked_source;
use tracing::debug;

/// The dotted module path that an import statement, or an
/// `importlib.import_module` or `__import__` call, may reach.
///
/// `from a import b` may import the module `a.b`, as in
/// `from google.cloud import bigquery`, so it is the import `a.b`.
/// `from a import *` is the import `a`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Import(String);

impl Import {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
impl Import {
    pub fn new(path: &str) -> Self {
        Import(path.to_owned())
    }
}

/// Extract all absolute imports from Python source.
///
/// Unparseable regions are skipped; the imports in the parseable rest are
/// still returned. Relative imports (`from . import x`) are excluded because
/// they can never refer to a third-party distribution.
pub fn extract_imports(source: &str) -> Vec<Import> {
    let parsed = parse_unchecked_source(source, PySourceType::Python);
    for err in parsed.errors() {
        debug!(%err, "syntax error, extracting from partial AST");
    }
    let mut collector = ImportCollector::default();
    collector.visit_body(&parsed.syntax().body);
    collector.imports
}

#[derive(Default)]
struct ImportCollector {
    imports: Vec<Import>,
}

impl ImportCollector {
    fn visit_body(&mut self, body: &[Stmt]) {
        for stmt in body {
            self.visit_stmt(stmt);
        }
    }
}

impl Visitor<'_> for ImportCollector {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Import(ast::StmtImport { names, .. }) => {
                for alias in names {
                    self.imports.push(Import(alias.name.to_string()));
                }
            }
            Stmt::ImportFrom(ast::StmtImportFrom {
                module: Some(module),
                names,
                level: 0,
                ..
            }) => {
                for alias in names {
                    let name = alias.name.as_str();
                    self.imports.push(Import(if name == "*" {
                        module.to_string()
                    } else {
                        format!("{module}.{name}")
                    }));
                }
            }
            _ => {}
        }
        // Recurse so imports nested in functions, classes, and try blocks
        // are found too.
        walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        if let Expr::Call(call) = expr
            && let Some(module) = dynamic_import_argument(call)
        {
            self.imports.push(Import(module));
        }
        walk_expr(self, expr);
    }
}

/// The module imported by `importlib.import_module("x")` or
/// `__import__("x")`, when the argument is a plain string literal.
///
/// Only those two spellings are recognized; a bare `import_module(...)`
/// could be anyone's function. Relative arguments (leading `.`) are skipped
/// like relative import statements.
fn dynamic_import_argument(call: &ast::ExprCall) -> Option<String> {
    let is_dynamic_import = match call.func.as_ref() {
        Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
            attr.as_str() == "import_module"
                && matches!(value.as_ref(), Expr::Name(name) if name.id.as_str() == "importlib")
        }
        Expr::Name(name) => name.id.as_str() == "__import__",
        _ => false,
    };
    if !is_dynamic_import {
        return None;
    }
    let Some(Expr::StringLiteral(literal)) = call.arguments.args.first() else {
        return None;
    };
    let module = literal.value.to_str();
    (!module.starts_with('.')).then(|| module.to_string())
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn import(path: &str) -> Import {
        Import::new(path)
    }

    #[test]
    fn simple_import() {
        assert_eq!(extract_imports("import os\n"), vec![import("os")]);
    }

    #[test]
    fn dotted_import() {
        assert_eq!(
            extract_imports("import google.cloud\n"),
            vec![import("google.cloud")]
        );
    }

    #[test]
    fn multiple_imports_on_one_line() {
        assert_eq!(
            extract_imports("import os, sys as system\n"),
            vec![import("os"), import("sys")]
        );
    }

    #[test]
    fn from_import_single() {
        assert_eq!(
            extract_imports("from google.cloud import bigquery\n"),
            vec![import("google.cloud.bigquery")]
        );
    }

    #[test]
    fn from_import_multiple_names() {
        assert_eq!(
            extract_imports("from collections import OrderedDict, defaultdict\n"),
            vec![
                import("collections.OrderedDict"),
                import("collections.defaultdict"),
            ]
        );
    }

    #[test]
    fn from_import_parenthesized_multiline() {
        let source = "from os.path import (\n    join,\n    split,\n)\n";
        assert_eq!(
            extract_imports(source),
            vec![import("os.path.join"), import("os.path.split")]
        );
    }

    #[test]
    fn from_import_star() {
        assert_eq!(
            extract_imports("from numpy import *\n"),
            vec![import("numpy")]
        );
    }

    #[test]
    fn relative_imports_are_excluded() {
        assert_eq!(extract_imports("from . import sibling\n"), vec![]);
        assert_eq!(extract_imports("from ..pkg import thing\n"), vec![]);
    }

    #[test]
    fn strings_and_comments_are_not_imports() {
        let source = concat!(
            "\"\"\"import fake_docstring\"\"\"\n",
            "'''import fake_single_quoted'''\n",
            "# import fake_comment\n",
            "x = \"import fake_string\"\n",
        );
        assert_eq!(extract_imports(source), vec![]);
    }

    #[test]
    fn nested_imports_are_found() {
        let source = concat!(
            "def f():\n",
            "    import json\n",
            "try:\n",
            "    import tomllib\n",
            "except ImportError:\n",
            "    import tomli\n",
        );
        assert_eq!(
            extract_imports(source),
            vec![import("json"), import("tomllib"), import("tomli")]
        );
    }

    #[test]
    fn syntax_errors_do_not_lose_parseable_imports() {
        let source = "import os\ndef broken(:\nimport sys\n";
        let imports = extract_imports(source);
        assert!(imports.contains(&import("os")), "{imports:?}");
    }

    #[test]
    fn empty_source() {
        assert_eq!(extract_imports(""), vec![]);
    }

    #[test]
    fn importlib_import_module_with_string_literal() {
        assert_eq!(
            extract_imports("import importlib\nx = importlib.import_module(\"requests\")\n"),
            vec![import("importlib"), import("requests")]
        );
    }

    #[test]
    fn dunder_import_with_string_literal() {
        assert_eq!(
            extract_imports("mod = __import__(\"numpy\")\n"),
            vec![import("numpy")]
        );
    }

    #[test]
    fn dynamic_imports_without_literals_are_ignored() {
        let source = concat!(
            "name = \"requests\"\n",
            "importlib.import_module(name)\n",
            "importlib.import_module(f\"pkg.{name}\")\n",
            "import_module(\"bare_call\")\n",
            "importlib.import_module(\"..relative\", package=__name__)\n",
        );
        assert_eq!(extract_imports(source), vec![]);
    }
}

#[cfg(test)]
mod properties {
    use super::*;
    use crate::testgen::{identifier, module_path};
    use hegel::generators;

    /// One import statement to render, plus the extraction we expect from it.
    #[derive(Debug, Clone)]
    enum Spec {
        Plain {
            path: String,
            alias: Option<String>,
        },
        From {
            path: String,
            names: Vec<(String, Option<String>)>,
        },
    }

    impl Spec {
        fn expected(&self) -> Vec<Import> {
            match self {
                Spec::Plain { path, .. } => vec![Import::new(path)],
                Spec::From { path, names } => names
                    .iter()
                    .map(|(name, _)| Import::new(&format!("{path}.{name}")))
                    .collect(),
            }
        }
    }

    #[hegel::composite]
    fn spec(tc: &hegel::TestCase) -> Spec {
        let maybe_alias = |tc: &hegel::TestCase| {
            if tc.draw(generators::booleans()) {
                Some(tc.draw(identifier()))
            } else {
                None
            }
        };
        if tc.draw(generators::booleans()) {
            Spec::Plain {
                path: tc.draw(module_path()),
                alias: maybe_alias(tc),
            }
        } else {
            let count = tc.draw(generators::integers::<usize>().min_value(1).max_value(3));
            let names = (0..count)
                .map(|_| (tc.draw(identifier()), maybe_alias(tc)))
                .collect();
            Spec::From {
                path: tc.draw(module_path()),
                names,
            }
        }
    }

    /// Text that mentions imports without being one: comments, docstrings,
    /// and string literals. Extraction must yield nothing from any of these.
    fn decoy(tc: &hegel::TestCase) -> String {
        let module = tc.draw(module_path());
        match tc.draw(generators::integers::<u8>().min_value(0).max_value(3)) {
            0 => format!("# import {module}\n"),
            1 => format!("\"\"\"\nimport {module}\n\"\"\"\n"),
            2 => format!("'''from {module} import thing'''\n"),
            _ => format!("_s = \"import {module}\"\n"),
        }
    }

    fn render_name_list(names: &[(String, Option<String>)]) -> Vec<String> {
        names
            .iter()
            .map(|(name, alias)| match alias {
                Some(a) => format!("{name} as {a}"),
                None => name.clone(),
            })
            .collect()
    }

    /// Render one spec with randomized but valid formatting choices.
    fn render(tc: &hegel::TestCase, spec: &Spec) -> String {
        let statement = match spec {
            Spec::Plain { path, alias } => match alias {
                Some(a) => format!("import {path} as {a}"),
                None if tc.draw(generators::booleans()) => {
                    format!("_m = importlib.import_module(\"{path}\")")
                }
                None => format!("import {path}"),
            },
            Spec::From { path, names } => {
                let rendered = render_name_list(names);
                if names.len() > 1 && tc.draw(generators::booleans()) {
                    // Parenthesized multi-line form, optional trailing comma.
                    let trailing = if tc.draw(generators::booleans()) {
                        ","
                    } else {
                        ""
                    };
                    format!(
                        "from {path} import (\n    {}{trailing}\n)",
                        rendered.join(",\n    ")
                    )
                } else {
                    format!("from {path} import {}", rendered.join(", "))
                }
            }
        };
        let semicolon = if tc.draw(generators::booleans()) {
            ";"
        } else {
            ""
        };
        if tc.draw(generators::booleans()) {
            // Nested in a block: indent every line of the statement.
            let indented = statement
                .lines()
                .map(|l| format!("    {l}"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("if True:\n{indented}{semicolon}\n")
        } else {
            format!("{statement}{semicolon}\n")
        }
    }

    fn sorted(mut imports: Vec<Import>) -> Vec<Import> {
        imports.sort();
        imports
    }

    // Whatever imports we render, however we format them, extraction
    // returns exactly those imports and nothing from the decoy text.
    #[hegel::test]
    fn extraction_roundtrips_rendered_imports(tc: hegel::TestCase) {
        let count = tc.draw(generators::integers::<usize>().min_value(0).max_value(6));
        let specs: Vec<Spec> = (0..count).map(|_| tc.draw(spec())).collect();

        let mut source = String::new();
        let mut expected = Vec::new();
        for s in &specs {
            if tc.draw(generators::booleans()) {
                source.push_str(&decoy(&tc));
            }
            source.push_str(&render(&tc, s));
            expected.extend(s.expected());
        }
        if tc.draw(generators::booleans()) {
            source.push_str(&decoy(&tc));
        }

        tc.note(&format!("source:\n{source}"));
        assert_eq!(sorted(extract_imports(&source)), sorted(expected));
    }

    // Arbitrary text, including invalid syntax and exotic Unicode, must
    // never panic the extractor.
    #[hegel::test]
    fn extraction_never_panics(tc: hegel::TestCase) {
        let source = tc.draw(generators::text());
        let _ = extract_imports(&source);
    }

    // Relative imports can never refer to a third-party distribution, so
    // they are never extracted no matter the dot depth.
    #[hegel::test]
    fn relative_imports_are_never_extracted(tc: hegel::TestCase) {
        let dots = tc.draw(generators::integers::<usize>().min_value(1).max_value(3));
        let path = if tc.draw(generators::booleans()) {
            tc.draw(module_path())
        } else {
            String::new()
        };
        let name = tc.draw(identifier());
        let source = format!("from {}{path} import {name}\n", ".".repeat(dots));
        assert_eq!(extract_imports(&source), vec![]);
    }
}
