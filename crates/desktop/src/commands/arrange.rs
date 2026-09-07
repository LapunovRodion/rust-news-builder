//! Automatic arrangement (T095, FR-019 – FR-021).

use newsbuilder_core::arrange::{ArrangeOptions, arrange_auto as core_arrange, set_placement};
use newsbuilder_core::model::item::{Layout, Placement};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::item::lock;
use crate::error::CommandResult;
use crate::state::{ItemView, Session};

/// What one arrangement did, and what it would cost to go further.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrangeResult {
    pub item: ItemView,
    /// How many photographs were given a placement.
    pub placed: usize,
    /// How many existing placements were discarded. Always 0 on the first call.
    pub replaced_manual: usize,
    /// How many the run worked around. This is the number the confirmation asks about: call
    /// with `replaceManual: false`, show this, and only call again with `true` if the editor
    /// agrees (FR-020).
    pub preserved_manual: usize,
}

/// One placement's contents, on the way in.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementInput {
    pub photo_ids: Vec<String>,
    pub layout: Layout,
}

#[tauri::command]
pub fn arrange_auto(
    replace_manual: bool,
    session: State<'_, Session>,
) -> CommandResult<ArrangeResult> {
    let mut state = lock(&session)?;
    let report = core_arrange(&mut state.item, ArrangeOptions { replace_manual });
    state.dirty = true;
    Ok(ArrangeResult {
        item: state.view(),
        placed: report.placed,
        replaced_manual: report.replaced_manual,
        preserved_manual: report.preserved_manual,
    })
}

/// Changes exactly one placement and leaves the others alone (FR-021).
#[tauri::command]
pub fn set_one_placement(
    at: usize,
    placement: PlacementInput,
    session: State<'_, Session>,
) -> CommandResult<ItemView> {
    let mut state = lock(&session)?;
    let mut photos = Vec::with_capacity(placement.photo_ids.len());
    for id in &placement.photo_ids {
        photos.push(state.photo_id(id)?);
    }
    set_placement(
        &mut state.item,
        at,
        Placement {
            photos,
            layout: placement.layout,
        },
    )?;
    state.dirty = true;
    Ok(state.view())
}
