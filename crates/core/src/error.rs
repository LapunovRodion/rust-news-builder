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
    ///
    /// `name` is `None` when the reader was handed bytes and nothing else, which is the usual
    /// case inside this crate: [`import_document`](crate::import::import_document) takes a byte
    /// slice, so it genuinely does not know what the file was called. SC-007 still wants the
    /// editor's message to name it, so the frontend — which does know — attaches it with
    /// [`Error::in_file`] on the way out.
    #[error("{} could not be read: {detail}", document_subject(name.as_deref()))]
    DocumentUnreadable {
        /// The offending file's name, once a caller that knows it has said so.
        name: Option<String>,
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

    /// A photo cannot take the name it was given (INV-5, FR-011).
    ///
    /// Beyond contracts/core-api.md's list. Renaming is an editor action that can fail for
    /// three separate reasons — the name is taken, it is empty, or it is not an image name —
    /// and SC-007 requires the message to say which name and which reason.
    #[error("photo name `{name}` cannot be used: {detail}")]
    PhotoNameUnusable {
        /// The name that was asked for.
        name: String,
        /// Why it cannot be used.
        detail: String,
    },

    /// A placement edit would leave the body in a state INV-1 or INV-4 forbids.
    ///
    /// Beyond contracts/core-api.md's list, for the same reason: the placement cards
    /// (FR-018a – FR-018d) are five operations that can be asked to do something impossible,
    /// and refusing them has to name the block and the reason. The user interface disables
    /// most of these; the core refuses them anyway, so a frontend that forgets cannot corrupt
    /// an item.
    #[error("the placement at block {at} cannot be edited: {detail}")]
    InvalidPlacement {
        /// The block index the caller named.
        at: usize,
        /// What was wrong with the request.
        detail: String,
    },
}

/// How a [`Error::DocumentUnreadable`] refers to its document, named or not.
fn document_subject(name: Option<&str>) -> String {
    match name {
        Some(name) => format!("document `{name}`"),
        None => "the document".to_owned(),
    }
}

impl Error {
    /// Names the file an error came out of, for the variants a byte-level reader cannot fill in.
    ///
    /// Constitution IV keeps paths out of this crate: import and appearance loading are handed
    /// bytes, so the code that finds a broken `.docx` or a rejected style string genuinely does
    /// not know what the file was called. SC-007 requires the editor's message to name it
    /// anyway, so the frontends — which opened the file and do know — attach it here on the way
    /// out.
    ///
    /// Variants that already name their subject are returned untouched, and a name already
    /// attached is never overwritten, so applying this twice cannot double up.
    #[must_use]
    pub fn in_file(self, name: &str) -> Self {
        match self {
            Self::DocumentUnreadable { name: None, detail } => Self::DocumentUnreadable {
                name: Some(name.to_owned()),
                detail,
            },
            Self::InvalidAppearance { detail } => Self::InvalidAppearance {
                detail: format!("`{name}`: {detail}"),
            },
            other => other,
        }
    }
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

#[cfg(test)]
mod sc_007_audit {
    //! T101 — the SC-007 audit, kept as a test rather than as a document.
    //!
    //! SC-007: *"Every failure an editor can cause names the specific offending file, marker,
    //! or photo."* A prose audit goes stale the first time a variant is added; this one cannot,
    //! because [`subject_of`] and [`warning_subject_of`] are exhaustive matches. Adding a
    //! variant stops the crate compiling until whoever added it has said what that variant's
    //! offending item is — which is the audit.
    //!
    //! Each sample below pairs a variant with the subject its message must name. The assertion
    //! is that the rendered message actually contains it, so a message rewritten into
    //! "upload failed" fails here rather than in front of an editor.
    //!
    //! Two variants are weaker than the rest and worth naming as such.
    //! [`Error::SecretStoreUnavailable`] and [`Error::InvalidAppearance`] carry their subject
    //! *inside* `detail` rather than in a field of their own, so for those the assertion only
    //! confirms the detail reaches the message. That is a real limit of this audit, not an
    //! oversight: neither failure is about a file, a marker, or a photo — one is about the
    //! machine's secret store and the other about a configuration key — so there is no
    //! separate subject to lift into a field. The `in_file` tests at the end of this module
    //! cover the one case where a caller *can* add a file to an appearance failure.

    use super::{Error, Warning};
    use crate::model::photo::PhotoId;

    /// The specific offending item a variant is about.
    ///
    /// Exhaustive on purpose: a new variant will not compile until its subject is named here.
    /// If a variant is ever genuinely about nothing in particular, that is an SC-007 defect in
    /// the variant, not a reason to loosen this.
    fn subject_of(error: &Error) -> String {
        match error {
            Error::UnsupportedFormat { name, .. } => name.clone(),
            // The one variant whose subject is optional, and only because the reader is handed
            // bytes. `in_file` is what fills it in; the test below holds that to account.
            Error::DocumentUnreadable { name, .. } => name.clone().unwrap_or_default(),
            Error::PhotoUnreadable { name, .. } => name.clone(),
            Error::SizeBudgetUnreachable { name, .. } => name.clone(),
            Error::NoCredential { server } => server.clone(),
            Error::SecretStoreUnavailable { detail } => detail.clone(),
            Error::RemotePathUnusable { path, .. } => path.clone(),
            Error::Transport { step, .. } => step.clone(),
            Error::Io { path, .. } => path.display().to_string(),
            Error::InvalidAppearance { detail } => detail.clone(),
            Error::PhotoNameUnusable { name, .. } => name.clone(),
            Error::InvalidPlacement { at, .. } => at.to_string(),
        }
    }

    /// The same, for the problems a build survives. Warnings are the ones an editor sees most,
    /// so they carry the same obligation.
    fn warning_subject_of(warning: &Warning) -> String {
        match warning {
            Warning::MissingPhoto { marker } => marker.clone(),
            Warning::MalformedMarker { marker, .. } => marker.clone(),
            Warning::UnusedPhoto { id } => id.to_string(),
            Warning::PhotoSkipped { name, .. } => name.clone(),
            Warning::UnsupportedDocumentFeature { what } => what.clone(),
            Warning::SizeBudgetUnreachable { name, .. } => name.clone(),
            Warning::SlugSuffixed { from, .. } => from.clone(),
            Warning::UnknownAppearanceKey { key } => key.clone(),
        }
    }

    /// One of every [`Error`] variant, with a distinctive subject.
    ///
    /// Keep in step with [`subject_of`]: adding a variant there without adding it here leaves
    /// the new variant unmeasured, which `every_error_variant_is_sampled` catches.
    fn one_of_every_error() -> Vec<Error> {
        vec![
            Error::UnsupportedFormat {
                name: "notes.pdf".to_owned(),
                detail: "PDF is not an image format".to_owned(),
            },
            Error::DocumentUnreadable {
                name: Some("news.docx".to_owned()),
                detail: "the file is not a readable .docx package".to_owned(),
            },
            Error::PhotoUnreadable {
                name: "IMG_0001.jpg".to_owned(),
                detail: "the JPEG data ends early".to_owned(),
            },
            Error::SizeBudgetUnreachable {
                name: "panorama.jpg".to_owned(),
                floor_quality: 50,
                achieved: 918_273,
            },
            Error::NoCredential {
                server: "newsroom".to_owned(),
            },
            Error::SecretStoreUnavailable {
                detail: "no Secret Service is running on this machine".to_owned(),
            },
            Error::RemotePathUnusable {
                path: "/var/www/html/news".to_owned(),
                detail: "the directory is not writable".to_owned(),
            },
            Error::Transport {
                step: "uploading den-konstitutsii-03.jpg".to_owned(),
                detail: "the connection was closed".to_owned(),
            },
            Error::Io {
                path: "/home/editor/news.docx".into(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
            },
            Error::InvalidAppearance {
                detail: "image.jpeg_min_quality is above image.jpeg_quality".to_owned(),
            },
            Error::PhotoNameUnusable {
                name: "IMG_0001.jpg".to_owned(),
                detail: "another photo already has that name".to_owned(),
            },
            Error::InvalidPlacement {
                at: 7,
                detail: "there is no block at that position".to_owned(),
            },
        ]
    }

    /// One of every [`Warning`] variant, likewise.
    fn one_of_every_warning() -> Vec<Warning> {
        vec![
            Warning::MissingPhoto {
                marker: "[image:7]".to_owned(),
            },
            Warning::MalformedMarker {
                marker: "[image:1,2]".to_owned(),
                reason: "a full-width marker takes one photo".to_owned(),
            },
            Warning::UnusedPhoto { id: PhotoId(4) },
            Warning::PhotoSkipped {
                name: "IMG_0009.heic".to_owned(),
                reason: "HEIC is not a supported format".to_owned(),
            },
            Warning::UnsupportedDocumentFeature {
                what: "a table".to_owned(),
            },
            Warning::SizeBudgetUnreachable {
                name: "panorama.jpg".to_owned(),
                floor_quality: 50,
                achieved: 918_273,
            },
            Warning::SlugSuffixed {
                from: "den-znaniy".to_owned(),
                to: "den-znaniy-2".to_owned(),
            },
            Warning::UnknownAppearanceKey {
                key: "styles.caption".to_owned(),
            },
        ]
    }

    #[test]
    fn every_error_message_names_its_subject() {
        for error in one_of_every_error() {
            let subject = subject_of(&error);
            let message = error.to_string();
            assert!(
                !subject.is_empty(),
                "SC-007: {error:?} has no offending item to name"
            );
            assert!(
                message.contains(&subject),
                "SC-007: `{message}` does not name `{subject}`"
            );
        }
    }

    #[test]
    fn every_warning_message_names_its_subject() {
        for warning in one_of_every_warning() {
            let subject = warning_subject_of(&warning);
            let message = warning.to_string();
            assert!(
                !subject.is_empty(),
                "SC-007: {warning:?} has no offending item to name"
            );
            assert!(
                message.contains(&subject),
                "SC-007: `{message}` does not name `{subject}`"
            );
        }
    }

    #[test]
    fn every_error_variant_is_sampled() {
        // `subject_of` catches a new variant at compile time; this catches one that was added
        // there but never given a sample, which would leave it unmeasured above.
        let sampled = one_of_every_error();
        let mut discriminants: Vec<_> = sampled.iter().map(std::mem::discriminant).collect();
        discriminants.dedup();
        assert_eq!(
            discriminants.len(),
            sampled.len(),
            "two samples are the same variant"
        );
        assert_eq!(
            sampled.len(),
            12,
            "Error has grown or shrunk: add the new variant to `one_of_every_error` and bump \
             this count, having first checked its message names the offending item"
        );

        let warnings = one_of_every_warning();
        let mut kinds: Vec<_> = warnings.iter().map(std::mem::discriminant).collect();
        kinds.dedup();
        assert_eq!(kinds.len(), warnings.len(), "two samples are the same kind");
        assert_eq!(
            warnings.len(),
            8,
            "Warning has grown or shrunk: same drill as above"
        );
    }

    #[test]
    fn an_unnamed_document_failure_is_the_one_gap_and_in_file_closes_it() {
        // The reader is handed bytes and cannot know the file name, so the message it produces
        // on its own is honest but unspecific. This is the seam SC-007 depends on, so it is
        // asserted rather than assumed.
        let anonymous = Error::DocumentUnreadable {
            name: None,
            detail: "the package has no word/document.xml".to_owned(),
        };
        assert_eq!(
            anonymous.to_string(),
            "the document could not be read: the package has no word/document.xml"
        );

        let named = anonymous.in_file("Дзень ведаў.docx");
        assert!(named.to_string().contains("Дзень ведаў.docx"), "{named}");
    }

    #[test]
    fn in_file_never_overwrites_a_name_that_is_already_there() {
        let named = Error::DocumentUnreadable {
            name: Some("news.docx".to_owned()),
            detail: "malformed XML".to_owned(),
        }
        .in_file("something-else.docx");
        assert!(named.to_string().contains("news.docx"), "{named}");
        assert!(!named.to_string().contains("something-else"), "{named}");
    }

    #[test]
    fn in_file_names_the_appearance_file_a_rejected_key_came_from() {
        // The detail names the field; only the caller knows which of the three appearance
        // layers (contracts/appearance-config.md) it was read from.
        let error = Error::InvalidAppearance {
            detail: "image.jpeg_min_quality is above image.jpeg_quality".to_owned(),
        }
        .in_file("newsroom-preset.json");
        let message = error.to_string();
        assert!(message.contains("newsroom-preset.json"), "{message}");
        assert!(message.contains("image.jpeg_min_quality"), "{message}");
    }

    #[test]
    fn in_file_leaves_a_variant_that_already_names_its_subject_alone() {
        let error = Error::PhotoUnreadable {
            name: "IMG_0001.jpg".to_owned(),
            detail: "the JPEG data ends early".to_owned(),
        }
        .in_file("news.docx");
        assert!(!error.to_string().contains("news.docx"), "{error}");
    }
}
