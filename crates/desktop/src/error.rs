//! What a failed command looks like on the TypeScript side.
//!
//! `{ kind, detail }`, mirroring `core::Error` exactly as contracts/desktop-commands.md
//! requires. The frontend renders `detail` as it arrives: the core has already made every
//! message name the offending file, marker or photo (SC-007), and rewording it in the UI would
//! throw that away.

use newsbuilder_core::error::Error;
use serde::Serialize;

/// A command failure, as the frontend sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    /// A stable machine-readable name, for the few places the UI branches on the cause.
    pub kind: String,
    /// The human-readable text, straight from the core.
    pub detail: String,
}

impl CommandError {
    /// A failure the desktop layer raised itself, where no core error fits.
    pub fn new(kind: &str, detail: impl Into<String>) -> Self {
        Self {
            kind: kind.to_owned(),
            detail: detail.into(),
        }
    }
}

impl From<Error> for CommandError {
    fn from(error: Error) -> Self {
        let kind = match &error {
            Error::UnsupportedFormat { .. } => "unsupported_format",
            Error::DocumentUnreadable { .. } => "document_unreadable",
            Error::PhotoUnreadable { .. } => "photo_unreadable",
            Error::SizeBudgetUnreachable { .. } => "size_budget_unreachable",
            Error::NoCredential { .. } => "no_credential",
            Error::SecretStoreUnavailable { .. } => "secret_store_unavailable",
            Error::RemotePathUnusable { .. } => "remote_path_unusable",
            Error::Transport { .. } => "transport",
            Error::Io { .. } => "io",
            Error::InvalidAppearance { .. } => "invalid_appearance",
            Error::PhotoNameUnusable { .. } => "photo_name_unusable",
            Error::InvalidPlacement { .. } => "invalid_placement",
            _ => "error",
        };

        // `Io` keeps the reason in `source`, so displaying the error alone would name the file
        // without saying what went wrong with it.
        let mut detail = error.to_string();
        let mut cause = std::error::Error::source(&error);
        while let Some(inner) = cause {
            detail.push_str(": ");
            detail.push_str(&inner.to_string());
            cause = std::error::Error::source(inner);
        }

        Self {
            kind: kind.to_owned(),
            detail,
        }
    }
}

/// Every command returns this.
pub type CommandResult<T> = std::result::Result<T, CommandError>;
