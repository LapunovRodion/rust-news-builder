//! The isolation boundary.
//!
//! Every side effect enters through one of these traits (constitution principle IV). `core`
//! depends on the traits; the adapters live in `crates/cli` and `crates/desktop`, and the tests
//! substitute in-memory fakes. Nothing in this crate calls `std::fs` or opens a socket.

use std::path::Path;

use time::OffsetDateTime;

use crate::error::Result;
use crate::model::server::{CredentialRef, ServerConfig};
use crate::model::site::{
    ArticleProbe, ArticleWrite, FindResult, SavedArticle, SiteCatalog, SiteTarget,
};
use crate::secret::Secret;

/// Local file access.
pub trait FileStore {
    /// Reads a file whole.
    fn read(&self, path: &Path) -> Result<Vec<u8>>;
    /// Writes a file, creating parent directories as needed.
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<()>;
    /// Whether a path exists.
    fn exists(&self, path: &Path) -> Result<bool>;
}

/// One entry in a remote directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    /// The file name, with no directory part.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
}

/// The only route to the network.
///
/// There is deliberately **no removal method**: deviation D-7 forbids deleting anything from
/// the server, and the absence of the capability is what enforces it. Do not add one.
pub trait Transport {
    /// Opens the connection.
    fn connect(&mut self, target: &ServerConfig, cred: &Secret) -> Result<()>;
    /// Lists a remote directory. An absent directory lists as empty, not as an error.
    fn list(&mut self, remote_dir: &str) -> Result<Vec<RemoteEntry>>;
    /// Creates a remote directory and any missing parents.
    fn ensure_dir(&mut self, remote_dir: &str) -> Result<()>;
    /// Uploads a file, overwriting any file already at that path.
    fn put(&mut self, remote_path: &str, bytes: &[u8]) -> Result<()>;
}

/// The route to a site's articles (002 FR-001), used on a connection [`Transport::connect`]
/// already opened.
///
/// Like [`Transport`], it has **no way to delete**: removing or trashing an article stays a
/// manual act in the CMS (FR-008), and the missing method is what guarantees it. Do not add one.
pub trait Site {
    /// The site's categories, view levels, languages and authors. Writes nothing.
    fn describe(&mut self, target: &SiteTarget) -> Result<SiteCatalog>;
    /// The articles that could be this item's. Writes nothing.
    fn find(&mut self, target: &SiteTarget, probe: &ArticleProbe) -> Result<FindResult>;
    /// Creates or updates one article.
    fn save(&mut self, target: &SiteTarget, write: &ArticleWrite) -> Result<SavedArticle>;
}

/// The OS secret store (FR-040).
pub trait SecretStore {
    /// Looks up a credential. `Ok(None)` means the store worked and held nothing.
    fn get(&self, r: &CredentialRef) -> Result<Option<Secret>>;
    /// Stores a credential.
    fn set(&self, r: &CredentialRef, s: &Secret) -> Result<()>;
    /// Removes a credential.
    fn delete(&self, r: &CredentialRef) -> Result<()>;
    /// Whether the store can be reached at all. `false` sends the caller to per-session
    /// entry (FR-041); it never sends it to a file on disk.
    fn available(&self) -> bool;
}

/// Wall-clock access, isolated so nothing in a rendered fragment can depend on it
/// (constitution principle IV).
pub trait Clock {
    /// The current instant.
    fn now(&self) -> OffsetDateTime;
}
