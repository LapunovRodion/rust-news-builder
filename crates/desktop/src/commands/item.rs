//! Opening a document, editing the text, and previewing (T041).

use std::collections::HashMap;
use std::path::PathBuf;

use newsbuilder_core::import::import_document;
use newsbuilder_core::model::item::{NewsItem, SourceFormat};
use newsbuilder_core::model::photo::{PhotoId, PhotoSource};
use newsbuilder_core::model::server::Slug;
use newsbuilder_core::publish::slug::slugify;
use serde::Serialize;
use tauri::State;

use crate::error::{CommandError, CommandResult};
use crate::state::{BlockInput, ItemView, Session, WarningView, blocks_from_input, warning_views};

/// What opening a document produced.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub item: ItemView,
    /// False when the document carried no detectable headline. The UI asks; it never invents
    /// one (FR-004).
    pub title_detected: bool,
    pub warnings: Vec<WarningView>,
}

/// What a preview produced.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResult {
    /// Exactly what `core::build` returned. The frontend puts this string into a sandboxed
    /// iframe and does nothing else to it — that is what makes the preview match the export
    /// (FR-022).
    pub fragment: String,
    pub warnings: Vec<WarningView>,
}

#[tauri::command]
pub fn open_document(path: String, session: State<'_, Session>) -> CommandResult<ImportResult> {
    let path = PathBuf::from(path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let format = SourceFormat::from_file_name(&name).ok_or_else(|| {
        CommandError::new(
            "unsupported_format",
            format!("`{name}` is not a .docx, .txt, or .md file"),
        )
    })?;

    let bytes = std::fs::read(&path).map_err(|error| {
        CommandError::new(
            "io",
            format!("`{}` could not be read: {error}", path.display()),
        )
    })?;

    // Import is handed bytes and never a path, so a failure it reports cannot name the file on
    // its own. This command opened it, so it attaches the name here (SC-007, T101).
    let imported = import_document(&bytes, format).map_err(|error| error.in_file(&name))?;
    let mut item = imported.item;
    item.source = Some(newsbuilder_core::model::item::SourceDocument {
        path: path.clone(),
        format,
        embedded_media: Vec::new(),
    });

    // A Word document's photos arrive as bytes inside the package, so the session can take
    // them straight from the item rather than reading anything else off disk (FR-015).
    let photo_bytes = collect_bytes(&item);

    let mut state = lock(&session)?;
    state.replace(item, photo_bytes);

    Ok(ImportResult {
        item: state.view(),
        title_detected: imported.title.is_some(),
        warnings: warning_views(&imported.warnings),
    })
}

#[tauri::command]
pub fn new_item(session: State<'_, Session>) -> CommandResult<ItemView> {
    let mut state = lock(&session)?;
    state.replace(NewsItem::new(), HashMap::new());
    Ok(state.view())
}

#[tauri::command]
pub fn set_title(title: String, session: State<'_, Session>) -> CommandResult<ItemView> {
    let mut state = lock(&session)?;
    // The slug follows the title until the editor overrides it; `set_slug` is what stops this
    // from happening again (FR-026).
    let follows = state.item.slug == slugify(&state.item.title);
    state.item.title = title;
    if follows {
        state.item.slug = slugify(&state.item.title);
    }
    state.dirty = true;
    Ok(state.view())
}

#[tauri::command]
pub fn set_body(blocks: Vec<BlockInput>, session: State<'_, Session>) -> CommandResult<ItemView> {
    let mut state = lock(&session)?;
    let body = blocks_from_input(&state.item, blocks)?;
    state.item.body = body;
    state.item.normalise_paragraph_kinds();
    state.dirty = true;
    Ok(state.view())
}

#[tauri::command]
pub fn set_slug(slug: String, session: State<'_, Session>) -> CommandResult<ItemView> {
    let parsed = Slug::parse(slug.trim()).ok_or_else(|| {
        CommandError::new(
            "invalid_slug",
            format!(
                "`{slug}` is not a slug: it must be lowercase letters, digits and hyphens, and \
                 cannot be empty"
            ),
        )
    })?;
    let mut state = lock(&session)?;
    state.item.slug = parsed;
    state.dirty = true;
    Ok(state.view())
}

#[tauri::command]
pub fn build_preview(session: State<'_, Session>) -> CommandResult<PreviewResult> {
    // `mut` for the rebuild cache the build keeps, not because the item changes (T102).
    let mut state = lock(&session)?;
    let output = state.build_preview()?;
    Ok(PreviewResult {
        fragment: output.fragment,
        warnings: warning_views(&output.warnings),
    })
}

#[tauri::command]
pub fn export_fragment(path: String, session: State<'_, Session>) -> CommandResult<()> {
    let mut state = lock(&session)?;
    let output = state.build_preview()?;
    let path = PathBuf::from(path);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|error| {
            CommandError::new(
                "io",
                format!("`{}` could not be created: {error}", parent.display()),
            )
        })?;
    }
    std::fs::write(&path, output.fragment).map_err(|error| {
        CommandError::new(
            "io",
            format!("`{}` could not be written: {error}", path.display()),
        )
    })?;
    state.dirty = false;
    Ok(())
}

/// The fragment, for the frontend to put on the clipboard.
///
/// Returning it rather than writing the clipboard here keeps the one clipboard permission in
/// the capability file doing one thing.
#[tauri::command]
pub fn copy_fragment(session: State<'_, Session>) -> CommandResult<String> {
    let mut state = lock(&session)?;
    Ok(state.build_preview()?.fragment)
}

/// Whether there are changes the editor has not exported or published (edge case: closing).
#[tauri::command]
pub fn is_dirty(session: State<'_, Session>) -> CommandResult<bool> {
    Ok(lock(&session)?.dirty)
}

/// Every photo's bytes, taken from the item itself.
fn collect_bytes(item: &NewsItem) -> HashMap<PhotoId, Vec<u8>> {
    item.photos
        .iter()
        .filter_map(|photo| match &photo.source {
            PhotoSource::Bytes(bytes) => Some((photo.id, bytes.to_vec())),
            PhotoSource::Path(path) => std::fs::read(path).ok().map(|bytes| (photo.id, bytes)),
        })
        .collect()
}

/// Takes the session lock, turning a poisoned mutex into a message rather than a panic
/// (constitution principle V).
pub fn lock<'a>(
    session: &'a State<'_, Session>,
) -> CommandResult<std::sync::MutexGuard<'a, crate::state::SessionState>> {
    session.0.lock().map_err(|_| {
        CommandError::new(
            "session_poisoned",
            "the editing session failed while another operation was in flight; reopen the \
             document",
        )
    })
}
