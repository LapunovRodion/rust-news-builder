//! Plain data: no I/O, no async, no dependency on the CLI or the desktop shell.
//!
//! Every field is owned, so a [`item::NewsItem`] can be snapshotted, compared, and rendered
//! without touching the filesystem.

pub mod appearance;
pub mod appearance_config;
pub mod item;
pub mod photo;
pub mod server;
pub mod server_store;
pub mod site;

pub use appearance::{Appearance, ImageBudget, StyleSet};
pub use item::{
    Block, BlockIndex, EmbeddedMedia, Layout, NewsItem, ParagraphKind, SourceDocument, SourceFormat,
};
pub use photo::{
    Adjustments, CropRect, NaturalKey, Orientation, Photo, PhotoId, PhotoOrigin, PhotoSource,
    Quarters,
};
pub use server::{CredentialRef, ServerConfig, ServerConfigRef, Slug};
