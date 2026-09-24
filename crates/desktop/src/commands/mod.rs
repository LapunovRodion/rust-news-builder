//! The IPC surface (contracts/desktop-commands.md).
//!
//! Every command here is a thin call into `newsbuilder-core`: take ids and values, apply them
//! to the item the session owns, hand back a fresh view. Constitution principle II is the rule
//! being kept — if a behaviour cannot be expressed as one of these, it does not belong in the
//! TypeScript either.

pub mod arrange;
pub mod item;
pub mod photo;
pub mod placement;
pub mod publish;
pub mod site;
