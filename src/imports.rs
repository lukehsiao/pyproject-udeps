//! Import extraction from Python source.
//!
//! Parsing is delegated to ruff's error-resilient parser: `parse_unchecked`
//! never fails, it produces a best-effort AST plus a list of syntax errors.
//! Walking the AST (instead of scanning text) means imports inside functions,
//! `try`/`except ImportError` blocks, and `if TYPE_CHECKING` sections are all
//! found, while text inside string literals, docstrings, and comments is not.

use ruff_python_ast::visitor::{Visitor, walk_stmt};
use ruff_python_ast::{self as ast, PySourceType, Stmt};
use ruff_python_parser::parse_unchecked_source;
use tracing::debug;

/// One imported module, as written in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// Dotted absolute module path, e.g. `google.cloud`.
    pub module: String,
    /// For `from X import Y`, the name Y (one `Import` per name).
    /// `None` for plain `import X` and for `from X import *`.
    pub item: Option<String>,
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
                    self.imports.push(Import {
                        module: alias.name.to_string(),
                        item: None,
                    });
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
                    self.imports.push(Import {
                        module: module.to_string(),
                        item: (name != "*").then(|| name.to_string()),
                    });
                }
            }
            _ => {}
        }
        // Recurse so imports nested in functions, classes, and try blocks
        // are found too.
        walk_stmt(self, stmt);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn import(module: &str) -> Import {
        Import {
            module: module.to_string(),
            item: None,
        }
    }

    fn from_import(module: &str, item: &str) -> Import {
        Import {
            module: module.to_string(),
            item: Some(item.to_string()),
        }
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
            vec![from_import("google.cloud", "bigquery")]
        );
    }

    #[test]
    fn from_import_multiple_names() {
        assert_eq!(
            extract_imports("from collections import OrderedDict, defaultdict\n"),
            vec![
                from_import("collections", "OrderedDict"),
                from_import("collections", "defaultdict"),
            ]
        );
    }

    #[test]
    fn from_import_parenthesized_multiline() {
        let source = "from os.path import (\n    join,\n    split,\n)\n";
        assert_eq!(
            extract_imports(source),
            vec![
                from_import("os.path", "join"),
                from_import("os.path", "split")
            ]
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
}
