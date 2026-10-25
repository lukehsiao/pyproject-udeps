//! Nullable wrapper over the filesystem: existence checks, lossy reads, and
//! parallel walks for Python files.

#[cfg(test)]
use std::collections::BTreeMap;
use std::io;
#[cfg(test)]
use std::path::Component;
use std::path::{Path, PathBuf};
#[cfg(test)]
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

/// What a walk visits.
#[derive(Debug)]
enum Walk {
    /// The project in the current directory, skipping gitignored entries
    /// and hidden directories such as `.venv`.
    Project,
    /// Everything under a virtualenv, where installed packages may sit in
    /// ignored or hidden places.
    Venv(PathBuf),
}

#[derive(Debug, Clone)]
pub struct FileSystem {
    inner: Inner,
}

#[derive(Debug, Clone)]
enum Inner {
    Real,
    /// In-memory file tree: path -> contents.
    #[cfg(test)]
    Null(Arc<BTreeMap<PathBuf, Vec<u8>>>),
}

impl FileSystem {
    #[must_use]
    pub fn create() -> Self {
        FileSystem { inner: Inner::Real }
    }

    #[must_use]
    pub fn exists(&self, path: &Path) -> bool {
        match &self.inner {
            Inner::Real => path.exists(),
            #[cfg(test)]
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
        let bytes = match &self.inner {
            Inner::Real => std::fs::read(path)?,
            #[cfg(test)]
            Inner::Null(files) => files.get(path).cloned().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "Nulled FileSystem: no file configured at {}",
                        path.display()
                    ),
                )
            })?,
        };
        Ok(decode_lossy(bytes))
    }

    /// Walk the project in the current directory for Python files
    /// (`.py`/`.pyi`) in the background, skipping gitignored entries and
    /// hidden directories. Unreadable entries are logged and skipped; the
    /// walk never fails a file into the caller.
    #[must_use]
    pub fn walk_project(&self) -> flume::Receiver<PyFile> {
        self.walk(Walk::Project)
    }

    /// Walk every Python file under the virtualenv at `venv` in the
    /// background, like [`FileSystem::walk_project`] but skipping nothing.
    #[must_use]
    pub fn walk_venv(&self, venv: &Path) -> flume::Receiver<PyFile> {
        self.walk(Walk::Venv(venv.to_path_buf()))
    }

    fn walk(&self, walk: Walk) -> flume::Receiver<PyFile> {
        // Bounded so walkers block instead of buffering a whole venv in
        // memory when the consumer is slower than the walk.
        let (tx, rx) = flume::bounded::<PyFile>(100);
        match &self.inner {
            Inner::Real => match walk {
                Walk::Project => spawn_real_walk(PathBuf::from("."), true, tx),
                Walk::Venv(venv) => spawn_real_walk(venv, false, tx),
            },
            #[cfg(test)]
            Inner::Null(files) => {
                let files = Arc::clone(files);
                thread::spawn(move || {
                    for (path, contents) in files.iter() {
                        let walked = match &walk {
                            Walk::Project => path.is_relative() && !in_hidden_dir(path),
                            Walk::Venv(venv) => path.starts_with(venv),
                        };
                        if walked && is_python_file(path) {
                            let file = PyFile {
                                path: path.clone(),
                                contents: decode_lossy(contents.clone()),
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

/// Whether `path` sits under a hidden, dot-prefixed directory.
///
/// The real walk skips hidden directories but still sends a hidden `.py`
/// file, because the ignore crate lets the Python type filter override
/// its hidden filter for files.
#[cfg(test)]
fn in_hidden_dir(path: &Path) -> bool {
    path.parent().is_some_and(|dir| {
        dir.components().any(|component| match component {
            Component::Normal(name) => name.as_encoded_bytes().starts_with(b"."),
            _ => false,
        })
    })
}

#[cfg(test)]
fn is_python_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext == "py" || ext == "pyi")
}

/// Decode file contents as UTF-8, replacing invalid bytes. Valid UTF-8, the
/// common case, is reused without a copy.
fn decode_lossy(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned())
}

/// Walk `root` with the real filesystem. `standard_filters` skips
/// gitignored entries and hidden directories. The Python type filter
/// still lets hidden `.py` files through.
fn spawn_real_walk(root: PathBuf, standard_filters: bool, tx: flume::Sender<PyFile>) {
    thread::spawn(move || {
        let types = match TypesBuilder::new().add_defaults().select("py").build() {
            Ok(types) => types,
            Err(err) => {
                warn!(%err, "failed to build walk type filter");
                return;
            }
        };
        let walker = WalkBuilder::new(root)
            .standard_filters(standard_filters)
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
                let contents = match std::fs::read(&path) {
                    Ok(bytes) => decode_lossy(bytes),
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

#[cfg(test)]
impl FileSystem {
    /// A filesystem containing exactly the given files. Bare
    /// `create_null([])` is an empty filesystem.
    ///
    /// Relative paths are project files. A nulled project walk skips hidden
    /// directories, as the real walk does, but it does not emulate
    /// gitignore files.
    pub fn create_null(
        files: impl IntoIterator<Item = (impl Into<PathBuf>, impl Into<Vec<u8>>)>,
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
    fn nulled_project_walk_skips_hidden_dirs() {
        let fs = FileSystem::create_null([
            ("main.py", "import os\n"),
            ("pkg/util.pyi", ""),
            ("README.md", "not python"),
            (".venv/lib/site.py", ""),
            ("pkg/.hidden.py", ""),
            ("/elsewhere/other.py", ""),
        ]);
        assert_eq!(
            collect_sorted(&fs.walk_project()),
            vec![
                PathBuf::from("main.py"),
                PathBuf::from("pkg/.hidden.py"),
                PathBuf::from("pkg/util.pyi"),
            ]
        );
    }

    #[test]
    fn nulled_venv_walk_sends_everything_under_the_venv() {
        let fs = FileSystem::create_null([
            (".venv/lib/site.py", ""),
            ("/venv/lib/.hidden/site.py", ""),
            ("main.py", ""),
        ]);
        assert_eq!(
            collect_sorted(&fs.walk_venv(Path::new(".venv"))),
            vec![PathBuf::from(".venv/lib/site.py")]
        );
        assert_eq!(
            collect_sorted(&fs.walk_venv(Path::new("/venv"))),
            vec![PathBuf::from("/venv/lib/.hidden/site.py")]
        );
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
    // walk mirrors, and the gitignore handling it does not.

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
    fn real_project_walk_skips_gitignored_entries_and_hidden_dirs() {
        let dir = tempdir_with(&[
            (".gitignore", b"ignored/\n"),
            ("kept.py", b"import os\n"),
            ("stub.pyi", b""),
            ("ignored/skipped.py", b"import sys\n"),
            (".venv/lib/site.py", b""),
            ("pkg/.hidden.py", b""),
            ("notes.txt", b"not python"),
        ]);
        // The ignore crate only respects .gitignore inside a git repository.
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        // The project walk always starts at the current directory, so walk
        // the temporary directory through the shared real walk instead.
        let (tx, rx) = flume::unbounded();
        spawn_real_walk(dir.path().to_path_buf(), true, tx);
        let paths = collect_sorted(&rx);
        assert_eq!(
            paths,
            vec![
                dir.path().join("kept.py"),
                dir.path().join("pkg/.hidden.py"),
                dir.path().join("stub.pyi"),
            ]
        );
    }

    #[test]
    fn real_venv_walk_visits_hidden_and_ignored_files() {
        let dir = tempdir_with(&[
            (".gitignore", b"ignored/\n"),
            ("ignored/skipped.py", b"import sys\n"),
            (".venv/lib/site.py", b""),
        ]);
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let fs = FileSystem::create();
        let paths = collect_sorted(&fs.walk_venv(dir.path()));
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
        let files: Vec<PyFile> = fs.walk_venv(dir.path()).iter().collect();
        assert_eq!(files.len(), 1);
        assert!(files[0].contents.starts_with("import os\n"));
    }
}
