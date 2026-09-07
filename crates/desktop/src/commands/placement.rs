//! The placement cards (T114, FR-018a – FR-018d).
//!
//! Five operations, each a direct call into `core::model::item`. The card is where an editor
//! decides *where a photograph goes*, and every one of these refuses a request the core
//! considers impossible rather than trusting the UI to have disabled the control.

use newsbuilder_core::model::item::{
    Layout, add_to_placement as core_add_to, insert_placement as core_insert,
    move_placement as core_move, remove_placement as core_remove, set_layout as core_set_layout,
};
use newsbuilder_core::model::photo::PhotoId;
use tauri::State;

use crate::commands::item::lock;
use crate::error::{CommandError, CommandResult};
use crate::state::{ItemView, Session};

/// Puts a placement at a point in the text — the caret, or where a drag was dropped (FR-018a).
#[tauri::command]
pub fn insert_placement(
    at: usize,
    photo_ids: Vec<String>,
    layout: Layout,
    session: State<'_, Session>,
) -> CommandResult<ItemView> {
    let mut state = lock(&session)?;
    let mut photos = Vec::with_capacity(photo_ids.len());
    for id in &photo_ids {
        photos.push(state.photo_id(id)?);
    }
    core_insert(&mut state.item, at, photos, layout)?;
    state.dirty = true;
    Ok(state.view())
}

/// Drags a card to another point in the text (FR-018d).
#[tauri::command]
pub fn move_placement(
    from: usize,
    to: usize,
    session: State<'_, Session>,
) -> CommandResult<ItemView> {
    let mut state = lock(&session)?;
    core_move(&mut state.item, from, to)?;
    state.dirty = true;
    Ok(state.view())
}

/// The card's remove control. The photo stays in the item (FR-018b).
#[tauri::command]
pub fn remove_placement(at: usize, session: State<'_, Session>) -> CommandResult<ItemView> {
    let mut state = lock(&session)?;
    core_remove(&mut state.item, at)?;
    state.dirty = true;
    Ok(state.view())
}

/// Dropping a photo onto a card, which makes it a row (FR-018c).
#[tauri::command]
pub fn add_to_placement(
    at: usize,
    photo_id: String,
    session: State<'_, Session>,
) -> CommandResult<ItemView> {
    let mut state = lock(&session)?;
    let photo: PhotoId = state.photo_id(&photo_id)?;
    core_add_to(&mut state.item, at, photo)?;
    state.dirty = true;
    Ok(state.view())
}

/// The card's layout control (FR-018b).
#[tauri::command]
pub fn set_layout(
    at: usize,
    layout: Layout,
    session: State<'_, Session>,
) -> CommandResult<ItemView> {
    let mut state = lock(&session)?;
    core_set_layout(&mut state.item, at, layout).map_err(CommandError::from)?;
    state.dirty = true;
    Ok(state.view())
}
