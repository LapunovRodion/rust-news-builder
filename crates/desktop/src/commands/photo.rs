//! Photo intake and in-app editing (T085, T076).

use std::path::PathBuf;

use newsbuilder_core::model::item::{
    PhotoIntake, add_photos, remove_photo as core_remove, rename_photo as core_rename,
    reorder_photos as core_reorder,
};
use newsbuilder_core::model::photo::{PhotoId, PhotoOrigin};
use newsbuilder_core::photo::frame::AspectRatio;
use newsbuilder_core::photo::{quarters, rotate, set_crop as core_set_crop, suggest_crop};
use serde::Serialize;
use tauri::State;

use crate::commands::item::lock;
use crate::error::{CommandError, CommandResult};
use crate::state::{CropRectView, ItemView, Session, WarningView, warning_views};

/// An intake result: what came in, and what was turned away.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddResult {
    pub item: ItemView,
    /// One per file that could not be used, naming it (FR-011, SC-007).
    pub warnings: Vec<WarningView>,
}

/// Drag-and-drop and the file picker converge here (FR-006, FR-008).
///
/// Tauri's own `onDragDropEvent` supplies paths; HTML5 drag-and-drop is not used (research R8).
#[tauri::command]
pub fn add_photos_from_paths(
    paths: Vec<String>,
    session: State<'_, Session>,
) -> CommandResult<AddResult> {
    let mut intake = Vec::with_capacity(paths.len());
    let mut warnings = Vec::new();

    for raw in paths {
        let path = PathBuf::from(&raw);
        match std::fs::read(&path) {
            Ok(bytes) => intake.push(PhotoIntake::from_path(
                path.clone(),
                bytes,
                PhotoOrigin::Picked { path },
            )),
            Err(error) => warnings.push(newsbuilder_core::error::Warning::PhotoSkipped {
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or(raw),
                reason: format!("it could not be read: {error}"),
            }),
        }
    }

    take(intake, warnings, session)
}

/// A pasted image (FR-007).
///
/// The frontend reads the clipboard — that is where the permission is — and hands the bytes
/// over. The name is ours to invent because a pasted image has none; uniqueness is enforced by
/// `add_photos`.
#[tauri::command]
pub fn add_photo_from_clipboard(
    bytes: Vec<u8>,
    session: State<'_, Session>,
) -> CommandResult<AddResult> {
    if bytes.is_empty() {
        return Err(CommandError::new(
            "empty_clipboard",
            "there is no image on the clipboard",
        ));
    }
    let intake = vec![PhotoIntake::from_bytes(
        "pasted.png",
        bytes,
        PhotoOrigin::Pasted,
    )];
    take(intake, Vec::new(), session)
}

#[tauri::command]
pub fn remove_photo(id: String, session: State<'_, Session>) -> CommandResult<AddResult> {
    let mut state = lock(&session)?;
    let photo = state.photo_id(&id)?;
    let warnings = core_remove(&mut state.item, photo);
    state.bytes.remove(&photo);
    state.dirty = true;
    Ok(AddResult {
        item: state.view(),
        warnings: warning_views(&warnings),
    })
}

#[tauri::command]
pub fn reorder_photos(order: Vec<String>, session: State<'_, Session>) -> CommandResult<ItemView> {
    let mut state = lock(&session)?;
    let ids: Vec<PhotoId> = order
        .iter()
        .filter_map(|id| id.parse::<u64>().ok().map(PhotoId))
        .collect();
    // Placements bind to ids, so they follow without being touched (FR-009, INV-1).
    core_reorder(&mut state.item, &ids);
    state.dirty = true;
    Ok(state.view())
}

#[tauri::command]
pub fn rename_photo(
    id: String,
    name: String,
    session: State<'_, Session>,
) -> CommandResult<ItemView> {
    let mut state = lock(&session)?;
    let photo = state.photo_id(&id)?;
    core_rename(&mut state.item, photo, &name)?;
    state.dirty = true;
    Ok(state.view())
}

/// Sets or clears a crop. `None` reverts to the full frame (FR-014).
#[tauri::command]
pub fn set_crop(
    id: String,
    crop: Option<CropRectView>,
    session: State<'_, Session>,
) -> CommandResult<ItemView> {
    let mut state = lock(&session)?;
    let id = state.photo_id(&id)?;
    let Some(photo) = state.item.photo_mut(id) else {
        return Err(CommandError::new(
            "invalid_photo",
            format!("the item holds no photo {id}"),
        ));
    };
    core_set_crop(photo, crop.map(Into::into))?;
    state.dirty = true;
    Ok(state.view())
}

/// Turns a photo by quarter-turns (FR-017). The source file is never touched (FR-015).
#[tauri::command]
pub fn rotate_photo(
    id: String,
    quarters_turned: i8,
    session: State<'_, Session>,
) -> CommandResult<ItemView> {
    let mut state = lock(&session)?;
    let id = state.photo_id(&id)?;
    if let Some(photo) = state.item.photo_mut(id) {
        rotate(photo, quarters(quarters_turned).get() as i8);
    }
    state.dirty = true;
    Ok(state.view())
}

/// The headroom-biased default frame for a target shape (FR-013).
///
/// `None` means the photo already has that shape and needs no crop at all.
#[tauri::command]
pub fn suggest_crop_for(
    id: String,
    aspect_width: u32,
    aspect_height: u32,
    session: State<'_, Session>,
) -> CommandResult<Option<CropRectView>> {
    let state = lock(&session)?;
    let id = state.photo_id(&id)?;
    let target = AspectRatio::new(aspect_width, aspect_height).ok_or_else(|| {
        CommandError::new(
            "invalid_aspect",
            format!("{aspect_width}:{aspect_height} is not a shape"),
        )
    })?;
    let Some(photo) = state.item.photo(id) else {
        return Err(CommandError::new(
            "invalid_photo",
            format!("the item holds no photo {id}"),
        ));
    };
    Ok(suggest_crop(photo, target).map(CropRectView::from))
}

/// Adds the intake and keeps its bytes in the session, so nothing is re-read from disk later.
fn take(
    intake: Vec<PhotoIntake>,
    mut warnings: Vec<newsbuilder_core::error::Warning>,
    session: State<'_, Session>,
) -> CommandResult<AddResult> {
    let mut state = lock(&session)?;

    // The bytes are captured before `add_photos` consumes the intake; a photo that is refused
    // never reaches the map, because it never gets an id.
    let offered: Vec<Vec<u8>> = intake.iter().map(|one| one.bytes.to_vec()).collect();
    let names: Vec<String> = intake.iter().map(|one| one.file_name.clone()).collect();

    let added = add_photos(&mut state.item, intake);
    warnings.extend(added.warnings.iter().cloned());

    // `add_photos` returns ids in the order it accepted them; walking the offered list in the
    // same order and skipping what was refused pairs each id with its bytes.
    let mut accepted = added.ids.iter();
    for (bytes, name) in offered.into_iter().zip(names) {
        let was_refused = added.warnings.iter().any(|warning| {
            matches!(warning, newsbuilder_core::error::Warning::PhotoSkipped { name: n, .. } if *n == name)
        });
        if was_refused {
            continue;
        }
        if let Some(id) = accepted.next() {
            state.bytes.insert(*id, bytes);
        }
    }

    state.dirty = true;
    Ok(AddResult {
        item: state.view(),
        warnings: warning_views(&warnings),
    })
}
