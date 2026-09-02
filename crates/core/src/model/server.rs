//! Publishing targets, credential handles, and the slug.
//!
//! A [`ServerConfig`] never holds a password or key material (FR-040): it holds a
//! [`CredentialRef`], which is a handle into the OS secret store. That is what makes a
//! serialised configuration safe to write to disk.

use url::Url;

/// A folder name: non-empty, lowercase, `[a-z0-9-]+` (INV-2).
///
/// Built by [`crate::publish::slug::slugify`], which reproduces the reference's transliteration
/// table exactly.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Slug(String);

impl Slug {
    /// The reference's fallback when a title transliterates to nothing at all.
    #[must_use]
    pub fn fallback() -> Self {
        Self("news".to_owned())
    }

    /// Wraps a string that is already in slug shape, rejecting anything that is not.
    ///
    /// This is the invariant's only door: everything else goes through
    /// [`crate::publish::slug::slugify`], which cannot produce a value this rejects.
    pub fn parse(value: &str) -> Option<Self> {
        if Self::is_well_formed(value) {
            Some(Self(value.to_owned()))
        } else {
            None
        }
    }

    /// Whether a string satisfies INV-2.
    #[must_use]
    pub fn is_well_formed(value: &str) -> bool {
        !value.is_empty()
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    }

    /// The slug itself.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A copy of this slug with a numeric suffix, used when a remote folder collides
    /// (deviation D-8).
    #[must_use]
    pub fn suffixed(&self, n: u32) -> Self {
        Self(format!("{}-{n}", self.0))
    }
}

impl std::fmt::Display for Slug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A handle into the OS secret store. Never the credential itself.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CredentialRef {
    /// An SSH private key on disk; the stored secret is its passphrase, if it has one.
    Key {
        /// Where the private key lives.
        path: std::path::PathBuf,
        /// The server configuration this belongs to, which is the secret-store key.
        server: String,
    },
    /// A password held entirely in the secret store.
    Password {
        /// The server configuration this belongs to, which is the secret-store key.
        server: String,
    },
}

impl CredentialRef {
    /// The server configuration name this credential belongs to.
    #[must_use]
    pub fn server(&self) -> &str {
        match self {
            Self::Key { server, .. } | Self::Password { server } => server,
        }
    }

    /// The entry name used inside the OS secret store.
    #[must_use]
    pub fn store_entry(&self) -> String {
        match self {
            Self::Key { server, .. } => format!("{server}/key-passphrase"),
            Self::Password { server } => format!("{server}/password"),
        }
    }
}

/// A reusable publishing target (FR-033).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServerConfig {
    /// Unique; the key editors see.
    pub name: String,
    /// SSH host.
    pub host: String,
    /// SSH user.
    pub user: String,
    /// SSH port.
    pub port: u16,
    /// POSIX path; item folders are created beneath it.
    pub remote_base_path: String,
    /// Item URLs are built beneath this.
    pub public_base_url: Url,
    /// How to authenticate. Resolved against the secret store at publish time.
    pub credential: CredentialRef,
}

/// The name of a [`ServerConfig`], which is all a [`crate::model::item::NewsItem`] holds.
///
/// Keeping the item free of connection details is what makes a serialised item safe to move
/// between machines.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServerConfigRef(pub String);

impl ServerConfigRef {
    /// The referenced configuration's name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::Slug;

    #[test]
    fn well_formed_slugs_parse() {
        assert!(Slug::parse("den-konstitutsii").is_some());
        assert!(Slug::parse("news-2026").is_some());
    }

    #[test]
    fn malformed_slugs_are_rejected() {
        assert!(Slug::parse("").is_none(), "empty");
        assert!(Slug::parse("Den").is_none(), "uppercase");
        assert!(Slug::parse("den konstitutsii").is_none(), "space");
        assert!(Slug::parse("день").is_none(), "non-ascii");
        assert!(Slug::parse("a_b").is_none(), "underscore");
    }

    #[test]
    fn suffixing_stays_well_formed() {
        let base = Slug::fallback();
        let suffixed = base.suffixed(2);
        assert_eq!(suffixed.as_str(), "news-2");
        assert!(Slug::is_well_formed(suffixed.as_str()));
    }
}
