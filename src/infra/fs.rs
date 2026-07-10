//! Nullable wrapper over the filesystem: existence checks, lossy reads, and
//! parallel walks for Python files.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use ignore::{WalkBuilder, types::TypesBuilder};
use tracing::warn;

/// One Python file found by a walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PyFile {
    pub path: PathBuf,
    pub contents: String,
}

/// Which files a walk skips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkFilters {
    /// Respect gitignore files and skip hidden entries (project walks).
    Standard,
    /// Visit everything, including hidden directories like `.venv`.
    None,
}

#[derive(Debug, Clone)]
pub struct FileSystem {
    inner: Inner,
}

#[derive(Debug, Clone)]
enum Inner {
    Real,
    /// In-memory file tree: path -> contents.
    Null(Arc<BTreeMap<PathBuf, String>>),
}

impl FileSystem {
    #[must_use]
    pub fn create() -> Self {
        FileSystem { inner: Inner::Real }
    }

    /// A filesystem containing exactly the given files. Bare
    /// `create_null([])` is an empty filesystem.
    ///
    /// The nulled walk sends every `.py`/`.pyi` file under the walk root and
    /// deliberately does not emulate gitignore semantics; [`WalkFilters`]
    /// behavior belongs to the real arm and its integration tests.
    pub fn create_null(
        files: impl IntoIterator<Item = (impl Into<PathBuf>, impl Into<String>)>,
    ) -> Self {
        FileSystem {
            inner: Inner::Null(Arc::new(
                files
                    .into_iter()
                    .map(|(path, contents)| (path.into(), contents.into()))
                    .collect(),
            )),
        }
    }

    #[must_use]
    pub fn exists(&self, path: &Path) -> bool {
        match &self.inner {
            Inner::Real => path.exists(),
            Inner::Null(files) => {
                files.contains_key(path) || files.keys().any(|p| p.starts_with(path))
            }
        }
    }

    /// Read a file as UTF-8, replacing invalid bytes.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be opened or read.
    pub fn read_to_string_lossy(&self, path: &Path) -> io::Result<String> {
        match &self.inner {
            Inner::Real => {
                let mut file = File::open(path)?;
                let mut buf = Vec::new();
                file.read_to_end(&mut buf)?;
                Ok(String::from_utf8_lossy(&buf).into_owned())
            }
            Inner::Null(files) => files.get(path).cloned().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "Nulled FileSystem: no file configured at {}",
                        path.display()
                    ),
                )
            }),
        }
    }

    /// Walk `root` for Python files (`.py`/`.pyi`) in the background and
    /// stream them to the returned receiver. Unreadable entries are logged
    /// and skipped; the walk never fails a file into the caller.
    #[must_use]
    pub fn walk_python_files(&self, root: &Path, filters: WalkFilters) -> flume::Receiver<PyFile> {
        // Bounded so walkers block instead of buffering a whole venv in
        // memory when the consumer is slower than the walk.
        let (tx, rx) = flume::bounded::<PyFile>(100);
        match &self.inner {
            Inner::Real => spawn_real_walk(root.to_path_buf(), filters, tx),
            Inner::Null(files) => {
                let files = Arc::clone(files);
                let root = root.to_path_buf();
                thread::spawn(move || {
                    for (path, contents) in files.iter() {
                        if under_root(path, &root) && is_python_file(path) {
                            let file = PyFile {
                                path: path.clone(),
                                contents: contents.clone(),
                            };
                            if tx.send(file).is_err() {
                                return;
                            }
                        }
                    }
                });
            }
        }
        rx
    }
}

/// A nulled walk from `.` or `./` matches every relative path, mirroring how
/// a real walk of the current directory sees any relative file.
fn under_root(path: &Path, root: &Path) -> bool {
    if root == Path::new(".") || root == Path::new("./") {
        return path.is_relative();
    }
    path.starts_with(root)
}

fn is_python_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext == "py" || ext == "pyi")
}

fn spawn_real_walk(root: PathBuf, filters: WalkFilters, tx: flume::Sender<PyFile>) {
    thread::spawn(move || {
        let types = match TypesBuilder::new().add_defaults().select("py").build() {
            Ok(types) => types,
            Err(err) => {
                warn!(%err, "failed to build walk type filter");
                return;
            }
        };
        let walker = WalkBuilder::new(root)
            .standard_filters(matches!(filters, WalkFilters::Standard))
            .types(types)
            .build_parallel();
        walker.run(|| {
            let tx = tx.clone();
            Box::new(move |result| {
                use ignore::WalkState::{Continue, Quit};

                let Ok(entry) = result.inspect_err(|err| warn!(%err, "skipping walk entry")) else {
                    return Continue;
                };
                if !entry.file_type().is_some_and(|t| t.is_file()) {
                    return Continue;
                }
                let path = entry.into_path();
                let contents = match read_lossy(&path) {
                    Ok(contents) => contents,
                    Err(err) => {
                        warn!(%err, path = %path.display(), "skipping unreadable file");
                        return Continue;
                    }
                };
                if tx.send(PyFile { path, contents }).is_err() {
                    // The receiver is gone; no point continuing the walk.
                    return Quit;
                }
                Continue
            })
        });
    });
}

fn read_lossy(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn collect_sorted(rx: &flume::Receiver<PyFile>) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = rx.iter().map(|f| f.path).collect();
        paths.sort();
        paths
    }

    #[test]
    fn nulled_walk_sends_python_files_under_root() {
        let fs = FileSystem::create_null([
            ("main.py", "import os\n"),
            ("pkg/util.pyi", ""),
            ("README.md", "not python"),
            ("/elsewhere/other.py", ""),
        ]);
        let paths = collect_sorted(&fs.walk_python_files(Path::new("."), WalkFilters::Standard));
        assert_eq!(
            paths,
            vec![PathBuf::from("main.py"), PathBuf::from("pkg/util.pyi")]
        );
    }

    #[test]
    fn nulled_walk_scopes_to_the_given_root() {
        let fs = FileSystem::create_null([("/venv/lib/site.py", ""), ("/project/main.py", "")]);
        let paths = collect_sorted(&fs.walk_python_files(Path::new("/venv"), WalkFilters::None));
        assert_eq!(paths, vec![PathBuf::from("/venv/lib/site.py")]);
    }

    #[test]
    fn nulled_read_returns_configured_contents() {
        let fs = FileSystem::create_null([("a.py", "import json\n")]);
        assert_eq!(
            fs.read_to_string_lossy(Path::new("a.py")).unwrap(),
            "import json\n"
        );
    }

    #[test]
    fn nulled_read_of_missing_file_is_a_self_naming_error() {
        let fs = FileSystem::create_null([] as [(&str, &str); 0]);
        let err = fs.read_to_string_lossy(Path::new("ghost.py")).unwrap_err();
        assert!(err.to_string().contains("Nulled FileSystem"), "{err}");
    }

    #[test]
    fn nulled_exists_covers_files_and_directories() {
        let fs = FileSystem::create_null([("/venv/lib/site.py", "")]);
        assert!(fs.exists(Path::new("/venv/lib/site.py")));
        assert!(fs.exists(Path::new("/venv")));
        assert!(!fs.exists(Path::new("/other")));
    }

    // Narrow integration tests: document the real walk behavior the nulled
    // walk deliberately does not emulate.

    fn tempdir_with(files: &[(&str, &[u8])]) -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        for (rel, contents) in files {
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        dir
    }

    #[test]
    fn real_walk_respects_gitignore_under_standard_filters() {
        let dir = tempdir_with(&[
            (".gitignore", b"ignored/\n"),
            ("kept.py", b"import os\n"),
            ("stub.pyi", b""),
            ("ignored/skipped.py", b"import sys\n"),
            ("notes.txt", b"not python"),
        ]);
        // The ignore crate only respects .gitignore inside a git repository.
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let fs = FileSystem::create();
        let paths = collect_sorted(&fs.walk_python_files(dir.path(), WalkFilters::Standard));
        assert_eq!(
            paths,
            vec![dir.path().join("kept.py"), dir.path().join("stub.pyi")]
        );
    }

    #[test]
    fn real_walk_with_no_filters_visits_hidden_and_ignored_files() {
        let dir = tempdir_with(&[
            (".gitignore", b"ignored/\n"),
            ("ignored/skipped.py", b"import sys\n"),
            (".venv/lib/site.py", b""),
        ]);
        let fs = FileSystem::create();
        let paths = collect_sorted(&fs.walk_python_files(dir.path(), WalkFilters::None));
        assert_eq!(
            paths,
            vec![
                dir.path().join(".venv/lib/site.py"),
                dir.path().join("ignored/skipped.py"),
            ]
        );
    }

    #[test]
    fn real_walk_reads_non_utf8_files_lossily() {
        let dir = tempdir_with(&[("legacy.py", b"import os\n\xff\xfe\n" as &[u8])]);
        let fs = FileSystem::create();
        let files: Vec<PyFile> = fs
            .walk_python_files(dir.path(), WalkFilters::Standard)
            .iter()
            .collect();
        assert_eq!(files.len(), 1);
        assert!(files[0].contents.starts_with("import os\n"));
    }
}
