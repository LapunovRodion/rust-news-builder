//! The local filesystem, behind [`FileStore`].
//!
//! Not a task of its own — it exists because [`ServerStore`](crate::model::server_store) and
//! both frontends need one real implementation of the port rather than three near-identical
//! ones. Everything it does is what the trait says, with every `std::io::Error` given the path
//! it happened to, because constitution principle V wants the offending file named.

use std::path::Path;

use crate::error::{Error, Result};
use crate::ports::FileStore;

/// [`FileStore`] over `std::fs`.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFiles;

impl LocalFiles {
    /// A handle to the local filesystem.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl FileStore for LocalFiles {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        std::fs::read(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::write(path, bytes).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    fn exists(&self, path: &Path) -> Result<bool> {
        match std::fs::metadata(path) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            // A path that cannot be examined is not the same as one that is not there:
            // answering `false` would send the caller on to create it and fail again, less
            // clearly.
            Err(source) => Err(Error::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LocalFiles;
    use crate::ports::FileStore;

    /// A directory this test owns, under the system temporary directory.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("newsbuilder-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn writing_creates_the_parent_directories() {
        let dir = scratch("localfiles-parents");
        let path = dir.join("a/b/c/servers.json");
        let files = LocalFiles::new();

        files.write(&path, b"{}").expect("writes");
        assert_eq!(files.read(&path).expect("reads"), b"{}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_absent_file_does_not_exist_and_is_not_an_error() {
        let dir = scratch("localfiles-absent");
        let files = LocalFiles::new();
        assert!(
            !files
                .exists(&dir.join("nothing.json"))
                .expect("an absent file is an answer, not a failure")
        );
    }

    #[test]
    fn reading_an_absent_file_names_it() {
        let dir = scratch("localfiles-missing");
        let path = dir.join("gone.json");
        let error = LocalFiles::new().read(&path).expect_err("there is no file");
        assert!(error.to_string().contains("gone.json"), "{error}");
    }
}
