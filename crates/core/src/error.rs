//! Typed errors and warnings.
//!
//! Constitution principle V: every variant names the specific offending item — the file, the
//! marker, the photo — and no variant carries a [`Secret`](crate::Secret), a password, or key
//! bytes. That is what SC-007 and SC-010 measure.

use std::path::PathBuf;

use crate::model::photo::PhotoId;

/// The crate-wide result type.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A failure that stops the operation in flight.
///
/// Problems that a build can survive are [`Warning`]s instead (FR-034).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A document or photo whose format the tool does not handle.
    #[error("unsupported format for `{name}`: {detail}")]
    UnsupportedFormat {
        /// The offending file name.
        name: String,
        /// What specifically was not understood.
        detail: String,
    },

    /// A document that could not be parsed at all.
    #[error("document could not be read: {detail}")]
    DocumentUnreadable {
        /// What specifically failed.
        detail: String,
    },

    /// A photo whose bytes could not be decoded.
    #[error("photo `{name}` could not be read: {detail}")]
    PhotoUnreadable {
        /// The offending photo's file name.
        name: String,
        /// What specifically failed.
        detail: String,
    },

    /// The quality search hit its floor with the photo still over budget (FR-028).
    #[error("photo `{name}` is still {achieved} bytes at the minimum quality of {floor_quality}")]
    SizeBudgetUnreachable {
        /// The offending photo's file name.
        name: String,
        /// The configured minimum quality that was reached.
        floor_quality: u8,
        /// The size achieved at that quality, in bytes.
        achieved: u64,
    },

    /// No key and no password resolved for the chosen server (FR-029, INV-8).
    #[error("no credential is stored for server `{server}`")]
    NoCredential {
        /// The server configuration's name.
        server: String,
    },

    /// The OS secret store could not be reached (FR-041).
    #[error("the operating system secret store is unavailable: {detail}")]
    SecretStoreUnavailable {
        /// What specifically failed.
        detail: String,
    },

    /// The remote base path is missing, is not a directory, or cannot be written to.
    #[error("remote path `{path}` cannot be used: {detail}")]
    RemotePathUnusable {
        /// The offending remote path.
        path: String,
        /// What specifically failed.
        detail: String,
    },

    /// A transport step failed. `step` names the operation, never the credential.
    #[error("transport failed during {step}: {detail}")]
    Transport {
        /// The operation in flight, e.g. `put news/photo-01.jpg`.
        step: String,
        /// What specifically failed.
        detail: String,
    },

    /// A local filesystem operation failed.
    #[error("i/o error on `{path}`")]
    Io {
        /// The offending path.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },

    /// An appearance configuration file was rejected (contracts/appearance-config.md).
    #[error("appearance configuration is invalid: {detail}")]
    InvalidAppearance {
        /// Which field was rejected and why.
        detail: String,
    },
}

/// A problem the build survives. Collected and reported, never fatal (FR-034).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Warning {
    /// A marker names a photo index the item does not hold.
    MissingPhoto {
        /// The marker text as written, e.g. `[image:7]`.
        marker: String,
    },

    /// Marker-shaped text the reference would have rejected outright.
    ///
    /// Not in contracts/core-api.md's list. The reference raises `ValueError` and abandons the
    /// document here; FR-034 requires the build to survive, and SC-007 requires the offending
    /// marker to be named, so the case needs a variant of its own. The marker's text is kept
    /// as literal prose, so nothing is lost.
    MalformedMarker {
        /// The marker exactly as written, e.g. `[image:1,2]`.
        marker: String,
        /// Why it was rejected.
        reason: String,
    },

    /// A photo no placement references. Unused photos are not uploaded.
    UnusedPhoto {
        /// The photo that goes unpublished.
        id: PhotoId,
    },

    /// A photo left out of the item, with the reason an editor can act on.
    PhotoSkipped {
        /// The offending file name.
        name: String,
        /// Why it was skipped.
        reason: String,
    },

    /// A Word construct outside the feature's scope, skipped rather than failed.
    UnsupportedDocumentFeature {
        /// The construct, e.g. `table`.
        what: String,
    },

    /// The quality search hit its floor with the photo still over budget.
    ///
    /// The [`Error`] variant of the same name is what a caller gets when this is fatal.
    /// FR-028 and FR-034 together require the build to emit it *and* keep going, so the
    /// build path needs the warning form.
    SizeBudgetUnreachable {
        /// The offending photo's published file name.
        name: String,
        /// The minimum quality that was reached.
        floor_quality: u8,
        /// The size achieved there, in bytes.
        achieved: u64,
    },

    /// A slug collided with a different item's remote folder and was suffixed.
    SlugSuffixed {
        /// The slug as derived.
        from: String,
        /// The slug actually used.
        to: String,
    },

    /// An unknown key in an appearance configuration file; ignored so older and newer
    /// preset files still load (contracts/appearance-config.md).
    UnknownAppearanceKey {
        /// The dotted key path, e.g. `styles.caption`.
        key: String,
    },
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingPhoto { marker } => {
                write!(f, "marker {marker} has no matching photo")
            }
            Self::MalformedMarker { marker, reason } => {
                write!(f, "marker {marker} was left as text: {reason}")
            }
            Self::UnusedPhoto { id } => {
                write!(
                    f,
                    "photo {id} is referenced by no placement and was not uploaded"
                )
            }
            Self::PhotoSkipped { name, reason } => {
                write!(f, "photo `{name}` was skipped: {reason}")
            }
            Self::UnsupportedDocumentFeature { what } => {
                write!(
                    f,
                    "document contains {what}, which is not supported and was skipped"
                )
            }
            Self::SizeBudgetUnreachable {
                name,
                floor_quality,
                achieved,
            } => write!(
                f,
                "photo `{name}` is still {achieved} bytes at the minimum quality of \
                 {floor_quality}; it was published anyway"
            ),
            Self::SlugSuffixed { from, to } => {
                write!(
                    f,
                    "slug `{from}` is taken by another item; using `{to}` instead"
                )
            }
            Self::UnknownAppearanceKey { key } => {
                write!(
                    f,
                    "appearance configuration key `{key}` is not recognised and was ignored"
                )
            }
        }
    }
}
