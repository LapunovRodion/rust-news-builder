//! In-memory fakes for the four ports (T013).
//!
//! Constitution IV says every side effect enters through a port; these are what make that pay
//! off. The whole publish suite runs with no filesystem, no socket, and no clock, and the
//! properties that matter — convergence, dry-run, early refusal, no deletion, no secret leak —
//! are asserted against [`RecordingTransport`]'s ordered log rather than against a real server.
//!
//! Two rules hold throughout, because the leak test (T053) depends on them:
//!
//! - No fake stores a credential anywhere a `Debug` can reach it as plain text.
//!   [`FakeSecretStore`] holds [`Secret`]s, which redact themselves.
//! - [`RecordingTransport`] records the *name* of the server it connected to and never the
//!   credential it connected with.

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use newsbuilder_core::error::{Error, Result};
use newsbuilder_core::model::server::{CredentialRef, ServerConfig};
use newsbuilder_core::ports::{Clock, FileStore, RemoteEntry, SecretStore, Transport};
use newsbuilder_core::secret::Secret;
use time::OffsetDateTime;

// ---------------------------------------------------------------------------------------------
// The target
// ---------------------------------------------------------------------------------------------

/// A publishing target pointing at the same base path and public URL the goldens were captured
/// under, so a published URL in a test is the URL the reference produced.
///
/// Authentication is by password, because that is the case where a secret exists to leak.
#[must_use]
pub fn server_config(name: &str) -> ServerConfig {
    ServerConfig {
        name: name.to_owned(),
        host: "news.example.org".to_owned(),
        user: "editor".to_owned(),
        port: 22,
        remote_base_path: super::REMOTE_BASE_PATH.to_owned(),
        public_base_url: url::Url::parse(super::PUBLIC_BASE_URL).expect("a valid constant url"),
        credential: CredentialRef::Password {
            server: name.to_owned(),
        },
    }
}

// ---------------------------------------------------------------------------------------------
// FileStore
// ---------------------------------------------------------------------------------------------

/// A filesystem that lives in a map.
///
/// Interior mutability because [`FileStore`] takes `&self`: the real adapter needs no `&mut`,
/// and the fake must not force one on it.
#[derive(Debug, Default)]
pub struct FakeFileStore {
    files: RefCell<BTreeMap<PathBuf, Vec<u8>>>,
}

impl FakeFileStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds a file that is already on disk.
    #[must_use]
    pub fn with_file(self, path: impl Into<PathBuf>, bytes: impl Into<Vec<u8>>) -> Self {
        self.files.borrow_mut().insert(path.into(), bytes.into());
        self
    }

    /// Everything the store holds, for assertions.
    #[must_use]
    pub fn contents(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        self.files.borrow().clone()
    }

    /// Every path and every byte the store holds, flattened into one string.
    ///
    /// This is what the leak test searches: a sentinel that reached the disk shows up here
    /// whether it landed in a path or in a file's contents.
    #[must_use]
    pub fn as_haystack(&self) -> String {
        let files = self.files.borrow();
        let mut haystack = String::new();
        for (path, bytes) in files.iter() {
            haystack.push_str(&path.to_string_lossy());
            haystack.push('\n');
            haystack.push_str(&String::from_utf8_lossy(bytes));
            haystack.push('\n');
        }
        haystack
    }
}

impl FileStore for FakeFileStore {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| Error::Io {
                path: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "the fake file store holds no such file",
                ),
            })
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }

    fn exists(&self, path: &Path) -> Result<bool> {
        Ok(self.files.borrow().contains_key(path))
    }
}

// ---------------------------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------------------------

/// One recorded transport call.
///
/// [`TransportOp::Put`] carries the byte *count* rather than the bytes: the log is printed in
/// full by several failure messages, and a megabyte of JPEG in a panic message helps nobody.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportOp {
    /// The server's configuration name. The credential is deliberately not recorded.
    Connect {
        server: String,
    },
    List {
        dir: String,
    },
    EnsureDir {
        dir: String,
    },
    Put {
        path: String,
        bytes: usize,
    },
}

impl TransportOp {
    /// Whether this operation changes remote state. Dry-run asserts there are none of these.
    #[must_use]
    pub fn mutates(&self) -> bool {
        matches!(self, Self::EnsureDir { .. } | Self::Put { .. })
    }
}

/// A server that remembers everything asked of it, in order.
///
/// Seed it with [`RecordingTransport::with_existing`] to model a folder that is already
/// populated — that is how a re-publish, a collision, and an orphaned file are all set up.
#[derive(Debug, Default)]
pub struct RecordingTransport {
    log: Vec<TransportOp>,
    /// Remote directory → file name → bytes.
    dirs: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    fail_list: BTreeSet<String>,
    fail_put: BTreeSet<String>,
    connected: bool,
}

impl RecordingTransport {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds a file that is already on the server.
    #[must_use]
    pub fn with_existing(
        mut self,
        dir: impl Into<String>,
        name: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        self.dirs
            .entry(dir.into())
            .or_default()
            .insert(name.into(), bytes.into());
        self
    }

    /// Makes one directory unlistable, which is how the port reports a base path that cannot
    /// be used. An unlistable directory is *not* the same as an absent one: absent lists empty.
    #[must_use]
    pub fn refusing_to_list(mut self, dir: impl Into<String>) -> Self {
        self.fail_list.insert(dir.into());
        self
    }

    /// Makes one upload fail.
    #[must_use]
    pub fn refusing_to_put(mut self, remote_path: impl Into<String>) -> Self {
        self.fail_put.insert(remote_path.into());
        self
    }

    /// Every call made, in order.
    #[must_use]
    pub fn log(&self) -> &[TransportOp] {
        &self.log
    }

    /// The remote paths uploaded to, in order.
    #[must_use]
    pub fn puts(&self) -> Vec<&str> {
        self.log
            .iter()
            .filter_map(|op| match op {
                TransportOp::Put { path, .. } => Some(path.as_str()),
                _ => None,
            })
            .collect()
    }

    /// How many uploads happened. Zero is the convergence assertion (SC-009).
    #[must_use]
    pub fn put_count(&self) -> usize {
        self.puts().len()
    }

    /// Whether anything remote was changed. Dry-run asserts `false` (FR-031).
    #[must_use]
    pub fn mutated(&self) -> bool {
        self.log.iter().any(TransportOp::mutates)
    }

    /// The file names present in a remote directory, sorted.
    #[must_use]
    pub fn names_in(&self, dir: &str) -> Vec<String> {
        self.dirs
            .get(dir)
            .map(|files| files.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// One remote file's bytes.
    #[must_use]
    pub fn file(&self, dir: &str, name: &str) -> Option<&[u8]> {
        self.dirs.get(dir)?.get(name).map(Vec::as_slice)
    }

    #[must_use]
    pub fn connected(&self) -> bool {
        self.connected
    }

    /// Clears the log while keeping the server's contents, so a second publish against the
    /// same state can be asserted on its own.
    pub fn forget_log(&mut self) {
        self.log.clear();
    }

    /// Splits a remote file path into its directory and file name, the inverse of
    /// `publish::paths::remote_file`.
    fn split(remote_path: &str) -> (String, String) {
        match remote_path.rsplit_once('/') {
            Some((dir, name)) => (dir.to_owned(), name.to_owned()),
            None => (String::new(), remote_path.to_owned()),
        }
    }
}

impl Transport for RecordingTransport {
    fn connect(&mut self, target: &ServerConfig, _cred: &Secret) -> Result<()> {
        // `_cred` is used to authenticate and is never stored: SC-010 is structural here too.
        self.log.push(TransportOp::Connect {
            server: target.name.clone(),
        });
        self.connected = true;
        Ok(())
    }

    fn list(&mut self, remote_dir: &str) -> Result<Vec<RemoteEntry>> {
        self.log.push(TransportOp::List {
            dir: remote_dir.to_owned(),
        });
        if self.fail_list.contains(remote_dir) {
            return Err(Error::RemotePathUnusable {
                path: remote_dir.to_owned(),
                detail: "the fake transport was told this directory cannot be used".to_owned(),
            });
        }
        Ok(self
            .dirs
            .get(remote_dir)
            .map(|files| {
                files
                    .iter()
                    .map(|(name, bytes)| RemoteEntry {
                        name: name.clone(),
                        size: bytes.len() as u64,
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    fn ensure_dir(&mut self, remote_dir: &str) -> Result<()> {
        self.log.push(TransportOp::EnsureDir {
            dir: remote_dir.to_owned(),
        });
        self.dirs.entry(remote_dir.to_owned()).or_default();
        Ok(())
    }

    fn put(&mut self, remote_path: &str, bytes: &[u8]) -> Result<()> {
        self.log.push(TransportOp::Put {
            path: remote_path.to_owned(),
            bytes: bytes.len(),
        });
        if self.fail_put.contains(remote_path) {
            return Err(Error::Transport {
                step: format!("put {remote_path}"),
                detail: "the fake transport was told this upload fails".to_owned(),
            });
        }
        let (dir, name) = Self::split(remote_path);
        self.dirs
            .entry(dir)
            .or_default()
            .insert(name, bytes.to_vec());
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// SecretStore
// ---------------------------------------------------------------------------------------------

/// An OS secret store that is a map.
///
/// Secrets are held as [`Secret`], so deriving `Debug` on this cannot leak one.
#[derive(Debug)]
pub struct FakeSecretStore {
    entries: RefCell<BTreeMap<String, Secret>>,
    available: bool,
}

impl Default for FakeSecretStore {
    fn default() -> Self {
        Self {
            entries: RefCell::new(BTreeMap::new()),
            available: true,
        }
    }
}

impl FakeSecretStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A store that cannot be reached at all, sending the caller to per-session entry (FR-041).
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            entries: RefCell::new(BTreeMap::new()),
            available: false,
        }
    }

    /// Seeds a credential.
    #[must_use]
    pub fn with_secret(self, r: &CredentialRef, value: &str) -> Self {
        self.entries
            .borrow_mut()
            .insert(r.store_entry(), Secret::new(value));
        self
    }
}

impl SecretStore for FakeSecretStore {
    fn get(&self, r: &CredentialRef) -> Result<Option<Secret>> {
        Ok(self.entries.borrow().get(&r.store_entry()).cloned())
    }

    fn set(&self, r: &CredentialRef, s: &Secret) -> Result<()> {
        self.entries.borrow_mut().insert(r.store_entry(), s.clone());
        Ok(())
    }

    fn delete(&self, r: &CredentialRef) -> Result<()> {
        self.entries.borrow_mut().remove(&r.store_entry());
        Ok(())
    }

    fn available(&self) -> bool {
        self.available
    }
}

// ---------------------------------------------------------------------------------------------
// Clock
// ---------------------------------------------------------------------------------------------

/// A clock stuck at one instant, so nothing a test asserts can depend on the wall clock
/// (constitution IV).
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub OffsetDateTime);

impl Default for FixedClock {
    fn default() -> Self {
        // 2026-03-01T09:00:00Z — the month the goldens were captured under.
        Self(
            OffsetDateTime::from_unix_timestamp(1_772_355_600)
                .expect("a valid unix timestamp constant"),
        )
    }
}

impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        self.0
    }
}
