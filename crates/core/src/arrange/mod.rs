//! One action that distributes photographs through the text and picks a layout for each
//! (T094, FR-019 – FR-021).
//!
//! The work is split three ways so each part can be read and tested on its own:
//!
//! - [`distribute`] answers *where*, from paragraph and group counts alone.
//! - [`layout`] answers *which photographs share a placement* and *what shape it takes*.
//! - This module is the orchestration between them, plus the one policy decision neither of
//!   them can make: what to do about placements the editor put there by hand.
//!
//! That last point is FR-020, and it is deliberately a two-step conversation rather than a
//! confirmation dialog bolted on afterwards. The first call arranges *around* manual work and
//! reports how much of it stands in the way; the interface asks; only a second call, with
//! `replace_manual: true`, takes it out. An editor therefore never loses a decision to a button
//! press, and the count that makes the question answerable comes from the run that changed
//! nothing.
//!
//! Everything here is a function of the item and the options — no clock, no hash iteration, no
//! randomness — because FR-024 promises byte-identical output and an arranged item is still an
//! item (constitution IV).

pub mod distribute;
pub mod layout;

use crate::error::{Error, Result};
use crate::model::item::{Block, BlockIndex, Layout, NewsItem, Placement};
use crate::model::photo::PhotoId;

use layout::{MAX_AUTOMATIC_ROW, Shape};

/// How [`arrange_auto`] treats placements that are already in the body.
#[derive(Debug, Clone, Copy, Default)]
pub struct ArrangeOptions {
    /// Whether to discard existing placements and arrange every photo from scratch.
    ///
    /// `false` — the default, and what the interface calls first — leaves them exactly as they
    /// are and arranges only what is unplaced (FR-020).
    pub replace_manual: bool,
}

/// What one run of [`arrange_auto`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArrangeReport {
    /// How many photographs this run gave a placement to.
    pub placed: usize,
    /// How many existing placements it discarded. Always 0 without `replace_manual`.
    pub replaced_manual: usize,
    /// How many existing placements it worked around and left alone.
    ///
    /// This is the number the interface needs in order to ask FR-020's question, so it is
    /// reported by the run that changes nothing rather than by the one that already has.
    pub preserved_manual: usize,
}

/// Distributes every unplaced photograph through the text and picks a layout for each (FR-019).
///
/// Nothing is ever dropped: where the text has fewer boundaries than there are photographs, the
/// surplus groups into rows rather than going missing. Photographs the body already places are
/// left where they are unless `replace_manual` says otherwise.
pub fn arrange_auto(item: &mut NewsItem, opts: ArrangeOptions) -> ArrangeReport {
    let mut report = ArrangeReport::default();

    let existing = item.body.iter().filter(|b| b.is_placement()).count();
    if opts.replace_manual {
        item.body.retain(|block| !block.is_placement());
        report.replaced_manual = existing;
    } else {
        report.preserved_manual = existing;
    }

    let placed_already = item.placed_photo_ids();
    let unplaced: Vec<(PhotoId, Shape)> = item
        .photos
        .iter()
        .filter(|photo| !placed_already.contains(&photo.id))
        .map(|photo| (photo.id, Shape::of(photo)))
        .collect();

    if unplaced.is_empty() {
        item.normalise_paragraph_kinds();
        return report;
    }

    // How many points in the text the groups may occupy. One per paragraph, but never so few
    // that the surplus has to pile into a row wider than the automatic maximum.
    let paragraphs = item
        .body
        .iter()
        .filter(|block| matches!(block, Block::Paragraph { .. }))
        .count();
    let slots = paragraphs.max(unplaced.len().div_ceil(MAX_AUTOMATIC_ROW));

    let groups = layout::plan(&unplaced, slots);
    let at = distribute::boundaries(paragraphs, groups.len());

    // Floats alternate across the whole page, not just across this run's own placements, so a
    // card the editor floated left is not immediately followed by another one.
    let mut floats_so_far = item
        .body
        .iter()
        .filter(|block| match block {
            Block::Placement { layout, .. } => layout.floats(),
            Block::Paragraph { .. } => false,
        })
        .count();

    for (group, after_paragraph) in groups.iter().zip(at) {
        let layout = layout::layout_for(group, floats_so_far);
        if layout.floats() {
            floats_so_far += 1;
        }
        let index = insertion_index(&item.body, after_paragraph);
        report.placed += group.photos.len();
        item.body.insert(
            index,
            Block::Placement {
                photos: group.photos.clone(),
                layout,
            },
        );
    }

    item.normalise_paragraph_kinds();
    report
}

/// Replaces the contents of exactly one placement, leaving every other one untouched (FR-021).
///
/// This is the override that makes an automatic arrangement a starting point rather than a
/// verdict: the editor changes the one card that is wrong without the rest of the page moving.
pub fn set_placement(item: &mut NewsItem, at: BlockIndex, placement: Placement) -> Result<()> {
    match item.body.get(at) {
        Some(Block::Placement { .. }) => {}
        Some(Block::Paragraph { .. }) => {
            return Err(Error::InvalidPlacement {
                at,
                detail: "it is a paragraph, not a placement".to_owned(),
            });
        }
        None => {
            return Err(Error::InvalidPlacement {
                at,
                detail: format!("the body holds {} blocks", item.body.len()),
            });
        }
    }

    if placement.photos.is_empty() {
        return Err(Error::InvalidPlacement {
            at,
            detail: "a placement must hold at least one photo".to_owned(),
        });
    }
    let mut seen: Vec<PhotoId> = Vec::with_capacity(placement.photos.len());
    for id in &placement.photos {
        if item.photo(*id).is_none() {
            return Err(Error::InvalidPlacement {
                at,
                detail: format!("the item holds no photo {id}"),
            });
        }
        if seen.contains(id) {
            return Err(Error::InvalidPlacement {
                at,
                detail: format!("photo {id} appears twice in the same placement"),
            });
        }
        seen.push(*id);
    }
    if placement.photos.len() > 1 && !placement.layout.holds_many() {
        return Err(Error::InvalidPlacement {
            at,
            detail: format!(
                "it would hold {} photos, and `{}` takes exactly one",
                placement.photos.len(),
                placement.layout.marker_keyword()
            ),
        });
    }

    if let Some(slot) = item.body.get_mut(at) {
        *slot = Block::Placement {
            photos: placement.photos,
            layout: placement.layout,
        };
    }
    Ok(())
}

/// Where in the body a group belongs, given how many paragraphs precede it.
///
/// Placements already sitting at that boundary are stepped over rather than pushed past, so a
/// run of groups sharing one boundary lands in group order and a manual card keeps its position
/// relative to the paragraph it was attached to.
fn insertion_index(body: &[Block], after_paragraph: usize) -> usize {
    let skip_placements = |mut at: usize| {
        while matches!(body.get(at), Some(Block::Placement { .. })) {
            at += 1;
        }
        at
    };

    if after_paragraph == 0 {
        return skip_placements(0);
    }

    let mut seen = 0usize;
    for (index, block) in body.iter().enumerate() {
        if matches!(block, Block::Paragraph { .. }) {
            seen += 1;
            if seen == after_paragraph {
                return skip_placements(index + 1);
            }
        }
    }
    body.len()
}

/// The layout a single photograph would get on its own, for a frontend that wants to show the
/// suggestion before committing to it.
#[must_use]
pub fn suggested_layout(item: &NewsItem, id: PhotoId) -> Layout {
    match item.photo(id) {
        Some(photo) if photo.is_portrait() => Layout::FloatRight,
        _ => Layout::FullWidth,
    }
}

#[cfg(test)]
mod tests {
    use super::insertion_index;
    use crate::model::item::{Block, Layout};
    use crate::model::photo::PhotoId;

    fn body() -> Vec<Block> {
        vec![
            Block::paragraph("one"),
            Block::placement(vec![PhotoId(1)], Layout::FullWidth),
            Block::paragraph("two"),
            Block::paragraph("three"),
        ]
    }

    #[test]
    fn the_front_of_an_empty_body_is_zero() {
        assert_eq!(insertion_index(&[], 0), 0);
    }

    #[test]
    fn a_boundary_lands_after_its_paragraph() {
        assert_eq!(insertion_index(&body(), 2), 3);
    }

    #[test]
    fn an_existing_placement_at_the_boundary_is_stepped_over() {
        // After paragraph one there is already a card; the new group goes after it, not between
        // the paragraph and the card the editor put there.
        assert_eq!(insertion_index(&body(), 1), 2);
    }

    #[test]
    fn a_boundary_past_the_end_lands_at_the_end() {
        assert_eq!(insertion_index(&body(), 9), 4);
    }
}
