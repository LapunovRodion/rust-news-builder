//! Exit codes and the `--json` result document (contracts/cli.md).
//!
//! The CLI's user is a script (FR-036), and a script reads two things: the status and, when it
//! asks for it, one JSON object on stdout. Both are defined here rather than at each call site
//! so a new failure path cannot invent a seventh exit code or a second document shape.
//!
//! Nothing in this module can print a credential. It is handed errors and warnings, and
//! constitution principle V already keeps key material out of both.

use std::fmt::Write as _;

use newsbuilder_core::error::{Error, Warning};

/// The six statuses of contracts/cli.md, and the only ones the binary returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    /// Finished. Warnings are permitted unless `--strict`.
    Success = 0,
    /// Bad flags, or a missing required argument.
    Usage = 1,
    /// An unreadable document, an unsupported format, a missing photo.
    Input = 2,
    /// The run needs a decision a script cannot make, or no credential resolved.
    Refusal = 3,
    /// A photo could not be brought within the size budget.
    Processing = 4,
    /// Connection, authentication, or upload failed.
    Transport = 5,
}

impl Exit {
    /// The process status.
    #[must_use]
    pub fn code(self) -> i32 {
        self as i32
    }

    /// Which status a core error reports as.
    ///
    /// The grouping is by *what the operator has to do next*, which is the only thing an exit
    /// code is useful for: fix the input, make a decision, fix the photo, or fix the network.
    #[must_use]
    pub fn of(error: &Error) -> Self {
        match error {
            Error::UnsupportedFormat { .. }
            | Error::DocumentUnreadable { .. }
            | Error::PhotoUnreadable { .. }
            | Error::PhotoNameUnusable { .. }
            | Error::InvalidAppearance { .. }
            | Error::Io { .. } => Self::Input,

            Error::NoCredential { .. }
            | Error::SecretStoreUnavailable { .. }
            | Error::RemotePathUnusable { .. }
            | Error::InvalidPlacement { .. }
            | Error::SiteIncomplete { .. }
            | Error::CategoryMissing { .. }
            | Error::AliasTaken { .. }
            | Error::ArticleAmbiguous { .. }
            | Error::InvalidArticleSetting { .. } => Self::Refusal,

            Error::SizeBudgetUnreachable { .. } => Self::Processing,

            Error::Transport { .. } | Error::SiteUnsupported { .. } | Error::SiteBridge { .. } => {
                Self::Transport
            }

            // `Error` is `#[non_exhaustive]`. A variant added later is an input problem until
            // someone classifies it, which is the answer that makes a script retry rather than
            // treat a new failure as success.
            _ => Self::Input,
        }
    }
}

/// A stable machine-readable name for an error, so a script can branch without parsing prose.
#[must_use]
pub fn error_kind(error: &Error) -> &'static str {
    match error {
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
        Error::SiteIncomplete { .. } => "site_incomplete",
        Error::SiteUnsupported { .. } => "site_unsupported",
        Error::CategoryMissing { .. } => "category_missing",
        Error::AliasTaken { .. } => "alias_taken",
        Error::ArticleAmbiguous { .. } => "article_ambiguous",
        Error::InvalidArticleSetting { .. } => "invalid_article_setting",
        Error::SiteBridge { .. } => "site_bridge",
        _ => "error",
    }
}

/// The same, for a warning.
#[must_use]
pub fn warning_kind(warning: &Warning) -> &'static str {
    match warning {
        Warning::MissingPhoto { .. } => "missing_photo",
        Warning::MalformedMarker { .. } => "malformed_marker",
        Warning::UnusedPhoto { .. } => "unused_photo",
        Warning::PhotoSkipped { .. } => "photo_skipped",
        Warning::UnsupportedDocumentFeature { .. } => "unsupported_document_feature",
        Warning::SizeBudgetUnreachable { .. } => "size_budget_unreachable",
        Warning::SlugSuffixed { .. } => "slug_suffixed",
        Warning::UnknownAppearanceKey { .. } => "unknown_appearance_key",
        _ => "warning",
    }
}

/// A refusal the CLI itself raises, where no core error fits.
///
/// The one case contracts/cli.md names is an item that still needs a human arrangement
/// decision. It is not a core [`Error`] because the core is right to accept such an item — the
/// desktop application opens exactly this state and asks the editor to resolve it.
#[derive(Debug, Clone)]
pub struct Refusal {
    /// The machine-readable name.
    pub kind: &'static str,
    /// What the operator has to do, in a sentence.
    pub detail: String,
}

/// Anything that ends a command.
#[derive(Debug)]
pub enum Failure {
    /// A failure from the domain crate.
    Core(Error),
    /// A refusal the CLI raised on its own.
    Refused(Refusal),
    /// A flag problem. Reported as [`Exit::Usage`] rather than clap's own status, so the six
    /// codes in contracts/cli.md are the complete set.
    Usage(String),
}

impl Failure {
    /// The status this failure exits with.
    #[must_use]
    pub fn exit(&self) -> Exit {
        match self {
            Self::Core(error) => Exit::of(error),
            Self::Refused(_) => Exit::Refusal,
            Self::Usage(_) => Exit::Usage,
        }
    }

    /// The machine-readable name.
    #[must_use]
    pub fn kind(&self) -> &str {
        match self {
            Self::Core(error) => error_kind(error),
            Self::Refused(refusal) => refusal.kind,
            Self::Usage(_) => "usage",
        }
    }

    /// The human-readable text, for stderr.
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::Core(error) => {
                // `Io` keeps its cause in `source`, so displaying the error alone would say
                // which file failed but not why.
                let mut text = error.to_string();
                let mut cause = std::error::Error::source(error);
                while let Some(inner) = cause {
                    let _ = write!(text, ": {inner}");
                    cause = std::error::Error::source(inner);
                }
                text
            }
            Self::Refused(refusal) => refusal.detail.clone(),
            Self::Usage(text) => text.clone(),
        }
    }
}

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self::Core(error)
    }
}

/// The `{"ok": false, ...}` document.
#[must_use]
pub fn failure_json(failure: &Failure) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": {
            "kind": failure.kind(),
            "detail": failure.detail(),
            "exit": failure.exit().code(),
        },
    })
}

/// The `warnings` array shared by every result document.
#[must_use]
pub fn warnings_json(warnings: &[Warning]) -> serde_json::Value {
    serde_json::Value::Array(
        warnings
            .iter()
            .map(|warning| {
                serde_json::json!({
                    "kind": warning_kind(warning),
                    "detail": warning.to_string(),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::{Exit, Failure, Refusal, error_kind, failure_json, warning_kind, warnings_json};
    use newsbuilder_core::error::{Error, Warning};
    use newsbuilder_core::model::photo::PhotoId;

    #[test]
    fn the_six_codes_are_the_ones_the_contract_lists() {
        assert_eq!(Exit::Success.code(), 0);
        assert_eq!(Exit::Usage.code(), 1);
        assert_eq!(Exit::Input.code(), 2);
        assert_eq!(Exit::Refusal.code(), 3);
        assert_eq!(Exit::Processing.code(), 4);
        assert_eq!(Exit::Transport.code(), 5);
    }

    #[test]
    fn an_unreadable_document_is_an_input_error() {
        let error = Error::DocumentUnreadable {
            name: Some("news.docx".to_owned()),
            detail: "not a zip".to_owned(),
        };
        assert_eq!(Exit::of(&error), Exit::Input);
    }

    #[test]
    fn a_missing_credential_is_a_refusal_not_a_transport_failure() {
        // The operator has to store a credential; retrying the connection will never help.
        let error = Error::NoCredential {
            server: "bsu".to_owned(),
        };
        assert_eq!(Exit::of(&error), Exit::Refusal);
    }

    #[test]
    fn a_photo_over_budget_is_its_own_code() {
        let error = Error::SizeBudgetUnreachable {
            name: "photo-01.jpg".to_owned(),
            floor_quality: 40,
            achieved: 900_000,
        };
        assert_eq!(Exit::of(&error), Exit::Processing);
    }

    #[test]
    fn an_upload_failure_is_a_transport_error() {
        let error = Error::Transport {
            step: "put photo-01.jpg".to_owned(),
            detail: "connection reset".to_owned(),
        };
        assert_eq!(Exit::of(&error), Exit::Transport);
    }

    #[test]
    fn a_refusal_exits_three_and_says_what_to_do() {
        let failure = Failure::Refused(Refusal {
            kind: "needs_arrangement",
            detail: "open it in the desktop application".to_owned(),
        });
        assert_eq!(failure.exit(), Exit::Refusal);
        assert_eq!(failure.kind(), "needs_arrangement");
        assert!(failure.detail().contains("desktop"));
    }

    #[test]
    fn a_flag_problem_exits_one() {
        let failure = Failure::Usage("--input is required".to_owned());
        assert_eq!(failure.exit(), Exit::Usage);
        assert_eq!(failure.kind(), "usage");
    }

    #[test]
    fn the_failure_document_has_the_shape_the_contract_shows() {
        let failure = Failure::Core(Error::NoCredential {
            server: "bsu".to_owned(),
        });
        let json = failure_json(&failure);
        assert_eq!(json["ok"], serde_json::Value::Bool(false));
        assert_eq!(json["error"]["kind"], "no_credential");
        assert_eq!(json["error"]["exit"], 3);
        assert!(
            json["error"]["detail"]
                .as_str()
                .is_some_and(|d| d.contains("bsu"))
        );
    }

    #[test]
    fn an_io_failure_reports_its_cause_as_well_as_its_path() {
        let failure = Failure::Core(Error::Io {
            path: "/nowhere/news.docx".into(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
        });
        let detail = failure.detail();
        assert!(detail.contains("/nowhere/news.docx"), "{detail}");
        assert!(detail.contains("no such file"), "{detail}");
    }

    #[test]
    fn every_warning_has_a_kind_and_a_detail() {
        let warnings = vec![
            Warning::UnusedPhoto { id: PhotoId(7) },
            Warning::MissingPhoto {
                marker: "[image:3]".to_owned(),
            },
        ];
        let json = warnings_json(&warnings);
        let array = json.as_array().expect("an array");
        assert_eq!(array.len(), 2);
        assert_eq!(array[0]["kind"], "unused_photo");
        assert_eq!(array[1]["kind"], "missing_photo");
        assert!(array[1]["detail"].as_str().is_some_and(|d| d.contains('3')));
    }

    #[test]
    fn kinds_are_snake_case_so_a_script_can_match_on_them() {
        assert_eq!(
            error_kind(&Error::DocumentUnreadable {
                name: None,
                detail: String::new()
            }),
            "document_unreadable"
        );
        assert_eq!(
            warning_kind(&Warning::SlugSuffixed {
                from: "a".to_owned(),
                to: "a-2".to_owned()
            }),
            "slug_suffixed"
        );
    }
}
