//! Where server configurations are kept (T062, FR-033, FR-040 – FR-042).
//!
//! The file holds connection settings only — host, user, port, remote base path, public base
//! URL — and a [`CredentialRef`], which is a *name* for a credential rather than the credential
//! itself. The password or key passphrase lives in the OS secret store and is fetched at
//! publish time (deviation D-5).
//!
//! That split is the whole point of this module, and it is structural rather than a matter of
//! care: [`ServerConfig`] has no field a secret could be written into, so serialising one
//! cannot leak. This is what SC-010 measures and what
//! `crates/core/tests/no_secret_leak.rs` asserts against a sentinel.
//!
//! Access goes through [`FileStore`] rather than `std::fs`, so the whole module is testable
//! with no filesystem at all (constitution IV).

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::model::server::ServerConfig;
use crate::ports::FileStore;

/// The file name inside the configuration directory.
pub const FILE_NAME: &str = "servers.json";

/// The configuration directory this application owns, e.g.
/// `~/.config/newsbuilder/` on Linux or `%APPDATA%\newsbuilder\config` on Windows.
///
/// `None` when the platform will not name one, which is a headless or misconfigured
/// environment; the caller then has to be told a path rather than guessing at one.
#[must_use]
pub fn default_directory() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "newsbuilder")
        .map(|dirs| dirs.config_dir().to_path_buf())
}

/// The default path of the configurations file.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    default_directory().map(|dir| dir.join(FILE_NAME))
}

/// The saved publishing targets, as a set keyed by name.
#[derive(Debug, Clone)]
pub struct ServerStore<F: FileStore> {
    files: F,
    path: PathBuf,
}

impl<F: FileStore> ServerStore<F> {
    /// A store over an explicit path.
    pub fn at(files: F, path: impl Into<PathBuf>) -> Self {
        Self {
            files,
            path: path.into(),
        }
    }

    /// A store at the platform's configuration directory.
    ///
    /// Fails rather than inventing a location: writing an editor's server list somewhere they
    /// will never find it is worse than saying the directory could not be determined.
    pub fn platform(files: F) -> Result<Self> {
        let path = default_path().ok_or_else(|| Error::Io {
            path: PathBuf::from(FILE_NAME),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "this platform does not name a configuration directory",
            ),
        })?;
        Ok(Self::at(files, path))
    }

    /// Where the configurations are written.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every saved configuration, by name.
    ///
    /// A file that is not there yet is an empty list, not an error — that is what a first run
    /// looks like.
    pub fn load(&self) -> Result<Vec<ServerConfig>> {
        if !self.files.exists(&self.path)? {
            return Ok(Vec::new());
        }
        let bytes = self.files.read(&self.path)?;
        let mut configs: Vec<ServerConfig> =
            serde_json::from_slice(&bytes).map_err(|error| Error::Io {
                path: self.path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("the server configuration file could not be read: {error}"),
                ),
            })?;
        // Sorting on the way out makes the file's order irrelevant to every caller, so a hand
        // edit cannot change which configuration a list shows first.
        configs.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(configs)
    }

    /// One configuration by name.
    pub fn get(&self, name: &str) -> Result<Option<ServerConfig>> {
        Ok(self.load()?.into_iter().find(|c| c.name == name))
    }

    /// One configuration by name, refusing rather than returning nothing.
    ///
    /// This is what a publish calls: "no such server" is a mistake to report, not an absence to
    /// work around.
    pub fn require(&self, name: &str) -> Result<ServerConfig> {
        self.get(name)?.ok_or_else(|| Error::NoCredential {
            server: name.to_owned(),
        })
    }

    /// Adds a configuration, or replaces the one with the same name.
    pub fn save(&self, config: ServerConfig) -> Result<()> {
        let mut configs = self.load()?;
        match configs.iter_mut().find(|c| c.name == config.name) {
            Some(existing) => *existing = config,
            None => configs.push(config),
        }
        self.write_all(&configs)
    }

    /// Removes a configuration. Returns whether one was there to remove.
    pub fn remove(&self, name: &str) -> Result<bool> {
        let mut configs = self.load()?;
        let before = configs.len();
        configs.retain(|c| c.name != name);
        if configs.len() == before {
            return Ok(false);
        }
        self.write_all(&configs)?;
        Ok(true)
    }

    fn write_all(&self, configs: &[ServerConfig]) -> Result<()> {
        let mut sorted = configs.to_vec();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        // Pretty-printed and sorted: this is a file an operator will read and sometimes edit,
        // and a stable byte order keeps it out of a diff when nothing changed.
        let json = serde_json::to_string_pretty(&sorted).map_err(|error| Error::Io {
            path: self.path.clone(),
            source: std::io::Error::other(format!(
                "the server configurations could not be written: {error}"
            )),
        })?;
        self.files.write(&self.path, json.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::{FILE_NAME, ServerStore, default_path};
    use crate::error::Result;
    use crate::model::server::{CredentialRef, ServerConfig};
    use crate::ports::FileStore;
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    /// A filesystem in a `RefCell`, so the store can be exercised with no disk.
    #[derive(Debug, Default)]
    struct MemoryFiles {
        entries: RefCell<Vec<(PathBuf, Vec<u8>)>>,
    }

    impl FileStore for MemoryFiles {
        fn read(&self, path: &Path) -> Result<Vec<u8>> {
            self.entries
                .borrow()
                .iter()
                .find(|(p, _)| p == path)
                .map(|(_, bytes)| bytes.clone())
                .ok_or_else(|| crate::error::Error::Io {
                    path: path.to_path_buf(),
                    source: std::io::Error::new(std::io::ErrorKind::NotFound, "not here"),
                })
        }

        fn write(&self, path: &Path, bytes: &[u8]) -> Result<()> {
            let mut entries = self.entries.borrow_mut();
            match entries.iter_mut().find(|(p, _)| p == path) {
                Some((_, slot)) => *slot = bytes.to_vec(),
                None => entries.push((path.to_path_buf(), bytes.to_vec())),
            }
            Ok(())
        }

        fn exists(&self, path: &Path) -> Result<bool> {
            Ok(self.entries.borrow().iter().any(|(p, _)| p == path))
        }
    }

    fn store() -> ServerStore<MemoryFiles> {
        ServerStore::at(MemoryFiles::default(), format!("/config/{FILE_NAME}"))
    }

    fn config(name: &str) -> ServerConfig {
        ServerConfig {
            name: name.to_owned(),
            host: "news.example.org".to_owned(),
            user: "editor".to_owned(),
            port: 22,
            remote_base_path: "/var/www/html/news".to_owned(),
            public_base_url: url::Url::parse("https://example.org/news/").expect("a valid url"),
            credential: CredentialRef::Password {
                server: name.to_owned(),
            },
        }
    }

    #[test]
    fn a_first_run_has_no_configurations_and_that_is_not_an_error() {
        assert!(store().load().expect("an absent file is empty").is_empty());
    }

    #[test]
    fn a_saved_configuration_comes_back() {
        let store = store();
        store.save(config("bsu")).expect("saves");
        let found = store.get("bsu").expect("loads").expect("it is there");
        assert_eq!(found.host, "news.example.org");
        assert_eq!(found.port, 22);
    }

    #[test]
    fn saving_the_same_name_replaces_rather_than_duplicates() {
        let store = store();
        store.save(config("bsu")).expect("saves");
        let mut changed = config("bsu");
        changed.host = "staging.example.org".to_owned();
        store.save(changed).expect("saves");

        let all = store.load().expect("loads");
        assert_eq!(all.len(), 1, "{all:?}");
        assert_eq!(all[0].host, "staging.example.org");
    }

    #[test]
    fn configurations_come_back_in_name_order_whatever_they_were_written_in() {
        let store = store();
        store.save(config("zeta")).expect("saves");
        store.save(config("alpha")).expect("saves");
        let names: Vec<String> = store
            .load()
            .expect("loads")
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, vec!["alpha", "zeta"]);
    }

    #[test]
    fn removing_reports_whether_there_was_anything_to_remove() {
        let store = store();
        store.save(config("bsu")).expect("saves");
        assert!(store.remove("bsu").expect("removes"));
        assert!(!store.remove("bsu").expect("already gone"));
        assert!(store.load().expect("loads").is_empty());
    }

    #[test]
    fn requiring_an_unknown_server_refuses_by_name() {
        let error = store()
            .require("nowhere")
            .expect_err("there is no such server");
        assert!(error.to_string().contains("nowhere"), "{error}");
    }

    #[test]
    fn the_written_file_contains_no_credential_only_a_reference() {
        // SC-010, structurally: `ServerConfig` has nowhere to put a secret, so this asserts the
        // shape of what is written rather than hoping nobody adds a field later.
        let store = store();
        store.save(config("bsu")).expect("saves");
        let bytes = store.files.read(store.path()).expect("it was written");
        let text = String::from_utf8(bytes).expect("json is utf-8");

        assert!(text.contains("\"kind\": \"password\""), "{text}");
        assert!(text.contains("\"server\": \"bsu\""), "{text}");
        for forbidden in ["password\":", "passphrase", "secret", "hunter2"] {
            assert!(
                !text.contains(&format!("\"{forbidden}")),
                "the file names a credential field: {text}"
            );
        }
    }

    #[test]
    fn a_corrupt_file_names_itself_rather_than_yielding_an_empty_list() {
        // Silently reporting "no servers" would look exactly like a first run, and the editor
        // would rebuild a list that is still on disk.
        let store = store();
        store
            .files
            .write(store.path(), b"not json at all")
            .expect("writes");
        let error = store.load().expect_err("this cannot be parsed");
        assert!(error.to_string().contains(FILE_NAME), "{error}");
    }

    #[test]
    fn the_platform_path_ends_in_the_expected_file() {
        if let Some(path) = default_path() {
            assert!(path.ends_with(FILE_NAME), "{}", path.display());
        }
    }
}
