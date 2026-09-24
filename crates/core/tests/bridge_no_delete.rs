//! The bridge cannot remove an article (002 T013, FR-008, INV-S3).
//!
//! `Transport` enforces "no deletion" by having no method for it. The bridge is PHP running
//! inside Joomla, where every deleting call is one line away, so the same guarantee is kept here
//! by refusing to compile — well, to pass — a bridge that contains one.
//!
//! Do not weaken this list to make a change fit. A change that needs one of these calls needs
//! the spec changed first.

const BRIDGE: &str = include_str!("../src/adapters/joomla/bridge.php");

/// Calls and values that delete, trash, archive, or lock an article.
const FORBIDDEN: &[&str] = &[
    "->delete(",
    "unlink(",
    "DELETE FROM",
    "TRUNCATE",
    "DROP ",
    "->trash(",
    // The model's state-change method, which is how an article is trashed or archived.
    "->publish(",
    // The trashed state, as a value assigned to anything.
    "=> -2",
    "= -2",
    "checkin",
    "checkout",
];

#[test]
fn the_bridge_contains_no_deleting_call() {
    let lowered = BRIDGE.to_lowercase();
    for pattern in FORBIDDEN {
        assert!(
            !lowered.contains(&pattern.to_lowercase()),
            "bridge.php contains `{pattern}`, which could remove or hide an article (FR-008)"
        );
    }
}
