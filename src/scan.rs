//! The import scan: every import in the project's Python files, and
//! optionally in its virtualenv.
//!
//! Parsing is the expensive step, so each file is parsed on the walk
//! thread that read it, and only its imports cross to the caller.

use std::path::{Path, PathBuf};

use crate::imports::{Import, extract_imports};
use crate::infra::fs::{FileSystem, PyFile};

/// The imports found in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub imports: Vec<Import>,
}

/// Scan the project, and the virtualenv at `venv` if given, at the same
/// time. Files arrive in no particular order.
pub fn scan_imports(fs: &FileSystem, venv: Option<&Path>) -> impl Iterator<Item = ScannedFile> {
    // Bounded so the walks block, instead of buffering a whole virtualenv,
    // when the caller falls behind. Each file in flight is only a short
    // list of imports, so the exact bound matters little.
    let (tx, rx) = flume::bounded(256);
    if let Some(venv) = venv {
        fs.walk_venv(venv, scan_file, tx.clone());
    }
    fs.walk_project(scan_file, tx);
    rx.into_iter()
}

fn scan_file(file: PyFile) -> ScannedFile {
    ScannedFile {
        imports: extract_imports(&file.contents),
        path: file.path,
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn scanned(fs: &FileSystem, venv: Option<&Path>) -> Vec<ScannedFile> {
        let mut files: Vec<ScannedFile> = scan_imports(fs, venv).collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files
    }

    fn file(path: &str, imports: &[&str]) -> ScannedFile {
        ScannedFile {
            path: PathBuf::from(path),
            imports: imports.iter().map(|path| Import::new(path)).collect(),
        }
    }

    #[test]
    fn scans_project_files_only_without_a_virtualenv() {
        let fs = FileSystem::create_null([
            (
                "main.py",
                "import requests\nfrom google.cloud import bigquery\n",
            ),
            (".venv/lib/site.py", "import yaml\n"),
            ("/venvs/demo/lib/x.py", "import numpy\n"),
        ]);
        assert_eq!(
            scanned(&fs, None),
            vec![file("main.py", &["requests", "google.cloud.bigquery"])]
        );
    }

    #[test]
    fn scans_the_virtualenv_and_the_project_together() {
        let fs = FileSystem::create_null([
            ("main.py", "import requests\n"),
            ("/venvs/demo/lib/x.py", "import numpy\n"),
        ]);
        assert_eq!(
            scanned(&fs, Some(Path::new("/venvs/demo"))),
            vec![
                file("/venvs/demo/lib/x.py", &["numpy"]),
                file("main.py", &["requests"]),
            ]
        );
    }
}
