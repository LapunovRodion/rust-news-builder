//! Real implementations of the [`ports`](crate::ports).
//!
//! Everything else in this crate is pure: bytes in, data out, no filesystem and no network.
//! This module is the deliberate exception — the place where a port stops being a trait and
//! starts touching the operating system.
//!
//! It lives in `core` rather than in each frontend because both frontends need the *same*
//! implementation. FR-039 promises that an item publishes identically from the desktop
//! application and from the CLI; two copies of a credential lookup are two things to keep in
//! step by hand, and the first divergence would be silent.
//!
//! [`transport`] carries the cost of that sharing: it needs `tokio` and `russh`, which have no
//! business in a parity test's dependency graph. It is therefore behind the non-default `sftp`
//! feature, which both frontends enable and no test of the domain rules does.

pub mod files;
pub mod joomla;
pub mod keyring_store;
#[cfg(feature = "sftp")]
pub mod transport;
