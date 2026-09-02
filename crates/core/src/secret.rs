//! A credential that cannot be printed by accident.
//!
//! [`Secret`] renders as `[redacted]` through both `Debug` and `Display`, so a stray `{:?}` in
//! a log line or an error message cannot leak a password. The bytes come out through exactly
//! one accessor, [`Secret::expose`], which is called in one place: the transport adapter.
//! This is the structural half of FR-032 and SC-010 — the guarantee holds without reviewer
//! vigilance.

/// A password or private-key passphrase.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wraps a credential.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Yields the credential itself.
    ///
    /// The name is deliberately alarming: every call site is a place a secret can escape, and
    /// there should be exactly one of them outside this module.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether the credential is empty, without revealing it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The redaction. Both formatters go through it, so neither can be the one that leaks.
const REDACTED: &str = "[redacted]";

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(REDACTED)
    }
}

impl std::fmt::Display for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(REDACTED)
    }
}

#[cfg(test)]
mod tests {
    use super::Secret;

    const SENTINEL: &str = "hunter2-sentinel-value";

    #[test]
    fn debug_redacts() {
        let secret = Secret::new(SENTINEL);
        let rendered = format!("{secret:?}");
        assert_eq!(rendered, "[redacted]");
        assert!(!rendered.contains(SENTINEL));
    }

    #[test]
    fn display_redacts() {
        let secret = Secret::new(SENTINEL);
        let rendered = format!("{secret}");
        assert_eq!(rendered, "[redacted]");
        assert!(!rendered.contains(SENTINEL));
    }

    #[test]
    fn redaction_survives_nesting() {
        // The common leak is a secret inside a struct someone derived Debug on.
        #[derive(Debug)]
        #[allow(dead_code)]
        struct Holder {
            user: &'static str,
            password: Secret,
        }

        let rendered = format!(
            "{:?}",
            Holder {
                user: "editor",
                password: Secret::new(SENTINEL)
            }
        );
        assert!(
            !rendered.contains(SENTINEL),
            "nested Debug leaked the secret: {rendered}"
        );
        assert!(rendered.contains("[redacted]"));
    }

    #[test]
    fn expose_returns_the_credential() {
        assert_eq!(Secret::new(SENTINEL).expose(), SENTINEL);
    }
}
