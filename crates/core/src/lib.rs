//! Domain rules for News Builder.
//!
//! Everything the product does — reading documents, parsing markers, transliterating slugs,
//! processing photos, rendering the HTML fragment, orchestrating uploads — lives here, so the
//! CLI and the desktop application stay thin adapters (constitution principle II).
//!
//! Side effects only ever enter through the traits in [`ports`], which is what makes the crate
//! testable with no filesystem, no network, and no clock (constitution principle IV).

// Constitution principle V: library code returns errors, it never raises them.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::exit)]
// Tests are not a reachable path in a shipped binary, and an assertion that cannot fail is
// exactly what `expect` is for. The denials above stay in force for everything else.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::exit)
)]
#![warn(missing_debug_implementations)]

pub mod error;
pub mod model;
pub mod photo;
pub mod ports;
pub mod secret;

pub use error::{Error, Result, Warning};
pub use secret::Secret;
