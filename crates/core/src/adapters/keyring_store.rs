//! The OS secret store (T061, FR-040 – FR-042).
//!
//! Passwords and key passphrases live in the platform's own vault — the Secret Service on
//! Linux, the Credential Manager on Windows, the Keychain on macOS — and never in a file this
//! application writes (deviation D-5).
//!
//! The interesting case is the one FR-041 names. On Linux the Secret Service is a daemon, and a
//! headless or minimal desktop may simply not run one. [`KeyringStore::available`] detects
//! that, and the answer sends the caller to per-session entry — never to a file on disk. That
//! is why `available` probes rather than assuming: a store that cannot be reached must be
//! distinguishable from a store that is empty, because the two call for different questions.

use crate::error::{Error, Result};
use crate::model::server::CredentialRef;
use crate::ports::SecretStore;
use crate::secret::Secret;

/// The service name every entry is filed under.
///
/// Stable across versions on purpose: changing it would strand every credential an editor has
/// already stored, with no error to explain where they went.
pub const SERVICE: &str = "newsbuilder";

/// The entry used to ask whether the store answers at all.
///
/// Reading a name nothing was ever written under is the cheapest question that still exercises
/// the whole path — connect, authenticate, look up — and `NoEntry` is a perfectly good answer
/// to it.
const PROBE_ENTRY: &str = "__availability_probe__";

/// [`SecretStore`] backed by the operating system's vault.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyringStore;

impl KeyringStore {
    /// A handle to the platform store.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    fn entry(&self, name: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(SERVICE, name).map_err(|error| Error::SecretStoreUnavailable {
            detail: format!("the entry `{name}` could not be addressed: {error}"),
        })
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, r: &CredentialRef) -> Result<Option<Secret>> {
        let name = r.store_entry();
        match self.entry(&name)?.get_password() {
            Ok(password) => Ok(Some(Secret::new(password))),
            // The store answered, and it holds nothing. That is not a failure: it is what an
            // editor who has not yet entered a credential looks like.
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(Error::SecretStoreUnavailable {
                detail: format!(
                    "the credential for `{}` could not be read: {error}",
                    r.server()
                ),
            }),
        }
    }

    fn set(&self, r: &CredentialRef, s: &Secret) -> Result<()> {
        let name = r.store_entry();
        // The one place in this module that touches the credential itself.
        self.entry(&name)?
            .set_password(s.expose())
            .map_err(|error| Error::SecretStoreUnavailable {
                detail: format!(
                    "the credential for `{}` could not be stored: {error}",
                    r.server()
                ),
            })
    }

    fn delete(&self, r: &CredentialRef) -> Result<()> {
        let name = r.store_entry();
        match self.entry(&name)?.delete_credential() {
            // Deleting what is not there is what the caller wanted either way.
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(Error::SecretStoreUnavailable {
                detail: format!(
                    "the credential for `{}` could not be removed: {error}",
                    r.server()
                ),
            }),
        }
    }

    fn available(&self) -> bool {
        let Ok(entry) = keyring::Entry::new(SERVICE, PROBE_ENTRY) else {
            return false;
        };
        match entry.get_password() {
            // Either answer means a store replied.
            Ok(_) | Err(keyring::Error::NoEntry) => true,
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyringStore, SERVICE};
    use crate::model::server::CredentialRef;
    use crate::ports::SecretStore;

    #[test]
    fn the_service_name_is_stable() {
        // Changing this strands every credential already stored, silently.
        assert_eq!(SERVICE, "newsbuilder");
    }

    #[test]
    fn entries_are_named_per_server_and_per_kind() {
        // Two servers must not share an entry, and a key passphrase must not overwrite a
        // password for the same server.
        let password = CredentialRef::Password {
            server: "bsu".to_owned(),
        };
        let key = CredentialRef::Key {
            path: "/home/editor/.ssh/id_ed25519".into(),
            server: "bsu".to_owned(),
        };
        let other = CredentialRef::Password {
            server: "staging".to_owned(),
        };

        assert_ne!(password.store_entry(), key.store_entry());
        assert_ne!(password.store_entry(), other.store_entry());
    }

    #[test]
    fn availability_is_answerable_without_panicking() {
        // The value depends on whether this machine runs a Secret Service, so the assertion is
        // about the call being total — which is the property FR-041 relies on to choose
        // between the stored credential and the per-session prompt.
        let store = KeyringStore::new();
        let _: bool = store.available();
    }

    #[test]
    fn a_lookup_never_reports_an_absent_credential_as_a_failure() {
        // On a machine with a working store this must be `Ok(None)`; on one without, an
        // explicit `SecretStoreUnavailable`. What it must never be is `Ok(Some(_))` for an
        // entry nothing was written to.
        let store = KeyringStore::new();
        let reference = CredentialRef::Password {
            server: "a-server-no-test-ever-writes-to".to_owned(),
        };
        if let Ok(found) = store.get(&reference) {
            assert!(found.is_none(), "an entry nothing wrote to held a value");
        }
    }
}
