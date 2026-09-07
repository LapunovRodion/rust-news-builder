//! T091 — automatic arrangement never quietly discards a decision the editor made.
//!
//! FR-020 is a two-step conversation: the application arranges without replacing anything and
//! reports how much manual work is in the way; the editor is asked; only then does it arrange
//! again with permission. This file pins both halves, plus the single-placement override of
//! FR-021 that makes the result adjustable afterwards.

mod support;

use newsbuilder_core::arrange::{ArrangeOptions, arrange_auto, set_placement};
use newsbuilder_core::model::item::{
    Block, Layout, NewsItem, PhotoIntake, Placement, add_photos, insert_placement,
};
use newsbuilder_core::model::photo::{PhotoId, PhotoOrigin};

use support::jpeg_bytes;

fn item_with(paragraphs: usize, photos: usize) -> (NewsItem, Vec<PhotoId>) {
    let mut item = NewsItem::new();
    item.title = "Manual Guard".to_owned();
    item.body = (1..=paragraphs)
        .map(|n| Block::paragraph(format!("Paragraph {n}.")))
        .collect();
    item.normalise_paragraph_kinds();
    let intake = (1..=photos)
        .map(|n| {
            PhotoIntake::from_bytes(
                format!("photo{n}.jpg"),
                jpeg_bytes(400, 300, n as u32),
                PhotoOrigin::Dropped,
            )
        })
        .collect();
    let added = add_photos(&mut item, intake);
    assert_eq!(added.ids.len(), photos, "{:?}", added.warnings);
    let ids = added.ids.clone();
    (item, ids)
}

/// The placement at a body index, as `(photos, layout)`.
fn placement_at(item: &NewsItem, at: usize) -> (Vec<PhotoId>, Layout) {
    match &item.body[at] {
        Block::Placement { photos, layout } => (photos.clone(), *layout),
        Block::Paragraph { text, .. } => panic!("block {at} is a paragraph: {text}"),
    }
}

fn placements(item: &NewsItem) -> Vec<(Vec<PhotoId>, Layout)> {
    item.body
        .iter()
        .filter_map(|block| match block {
            Block::Placement { photos, layout } => Some((photos.clone(), *layout)),
            Block::Paragraph { .. } => None,
        })
        .collect()
}

// -------------------------------------------------------------------------------------------
// The first call: arrange around what is already there
// -------------------------------------------------------------------------------------------

#[test]
fn without_permission_a_manual_placement_is_left_exactly_as_it_was() {
    let (mut item, ids) = item_with(5, 3);
    insert_placement(&mut item, 1, vec![ids[0]], Layout::FloatRight).expect("manual placement");
    let manual = placement_at(&item, 1);

    let report = arrange_auto(
        &mut item,
        ArrangeOptions {
            replace_manual: false,
        },
    );

    assert_eq!(report.replaced_manual, 0, "nothing was replaced");
    assert!(
        placements(&item).contains(&manual),
        "the editor's own placement survived: {:?}",
        placements(&item)
    );
}

#[test]
fn the_first_call_reports_how_much_manual_work_is_in_the_way() {
    // This is the number the interface needs in order to ask the question FR-020 requires. It
    // is reported by the run that changes nothing, so the editor is asked before, not after.
    let (mut item, ids) = item_with(6, 4);
    insert_placement(&mut item, 1, vec![ids[0]], Layout::FloatRight).expect("manual");
    insert_placement(&mut item, 3, vec![ids[1]], Layout::FloatLeft).expect("manual");

    let report = arrange_auto(
        &mut item,
        ArrangeOptions {
            replace_manual: false,
        },
    );

    assert_eq!(report.replaced_manual, 0);
    assert_eq!(
        report.preserved_manual, 2,
        "two placements would be lost if the editor said yes"
    );
}

#[test]
fn the_unplaced_photos_are_still_arranged_around_the_manual_ones() {
    let (mut item, ids) = item_with(5, 3);
    insert_placement(&mut item, 1, vec![ids[0]], Layout::FloatRight).expect("manual");

    let report = arrange_auto(
        &mut item,
        ArrangeOptions {
            replace_manual: false,
        },
    );

    assert_eq!(report.placed, 2);
    assert!(item.unused_photo_ids().is_empty(), "everything is placed");
    assert!(item.placements_resolve(), "INV-1");
}

#[test]
fn an_item_with_no_manual_placements_reports_none() {
    let (mut item, _) = item_with(4, 2);
    let report = arrange_auto(
        &mut item,
        ArrangeOptions {
            replace_manual: false,
        },
    );
    assert_eq!(report.preserved_manual, 0);
    assert_eq!(report.replaced_manual, 0);
    assert_eq!(report.placed, 2);
}

// -------------------------------------------------------------------------------------------
// The second call: with permission
// -------------------------------------------------------------------------------------------

#[test]
fn with_permission_the_manual_placements_go_and_are_counted() {
    let (mut item, ids) = item_with(6, 4);
    insert_placement(&mut item, 1, vec![ids[0]], Layout::FloatRight).expect("manual");
    insert_placement(&mut item, 3, vec![ids[1]], Layout::FloatLeft).expect("manual");

    let report = arrange_auto(
        &mut item,
        ArrangeOptions {
            replace_manual: true,
        },
    );

    assert_eq!(report.replaced_manual, 2);
    assert_eq!(report.placed, 4, "all four were arranged from scratch");
    assert!(item.unused_photo_ids().is_empty());
    assert!(item.placements_resolve(), "INV-1");
    assert!(item.layouts_match_counts(), "INV-4");
}

#[test]
fn replacing_keeps_every_photo_and_every_paragraph() {
    let (mut item, ids) = item_with(4, 3);
    insert_placement(&mut item, 1, vec![ids[0], ids[1]], Layout::Row).expect("manual");

    arrange_auto(
        &mut item,
        ArrangeOptions {
            replace_manual: true,
        },
    );

    assert_eq!(
        item.photos.len(),
        3,
        "replacing placements is not removing photos"
    );
    let texts: Vec<String> = item
        .body
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph { text, .. } => Some(text.clone()),
            Block::Placement { .. } => None,
        })
        .collect();
    assert_eq!(
        texts,
        vec![
            "Paragraph 1.",
            "Paragraph 2.",
            "Paragraph 3.",
            "Paragraph 4."
        ]
    );
}

// -------------------------------------------------------------------------------------------
// set_placement — the per-placement override of FR-021
// -------------------------------------------------------------------------------------------

#[test]
fn set_placement_changes_exactly_one_placement() {
    let (mut item, ids) = item_with(5, 3);
    insert_placement(&mut item, 1, vec![ids[0]], Layout::FullWidth).expect("placed");
    insert_placement(&mut item, 3, vec![ids[1]], Layout::FullWidth).expect("placed");
    let before = placements(&item);

    set_placement(
        &mut item,
        1,
        Placement {
            photos: vec![ids[2]],
            layout: Layout::FloatLeft,
        },
    )
    .expect("a photo the item holds, under a layout that fits it");

    let after = placements(&item);
    assert_eq!(after.len(), before.len(), "no placement was added or lost");
    let changed = before.iter().zip(&after).filter(|(a, b)| a != b).count();
    assert_eq!(
        changed, 1,
        "exactly one placement moved:\n{before:?}\n{after:?}"
    );
    assert_eq!(placement_at(&item, 1), (vec![ids[2]], Layout::FloatLeft));
    assert_eq!(
        placement_at(&item, 3),
        (vec![ids[1]], Layout::FullWidth),
        "the other placement is untouched (FR-021)"
    );
}

#[test]
fn set_placement_refuses_a_photo_the_item_does_not_hold() {
    let (mut item, ids) = item_with(3, 1);
    insert_placement(&mut item, 1, vec![ids[0]], Layout::FullWidth).expect("placed");
    let error = set_placement(
        &mut item,
        1,
        Placement {
            photos: vec![PhotoId(42)],
            layout: Layout::FullWidth,
        },
    )
    .expect_err("INV-1 forbids it");
    assert!(error.to_string().contains("42"), "{error}");
    assert_eq!(placement_at(&item, 1), (vec![ids[0]], Layout::FullWidth));
}

#[test]
fn set_placement_refuses_a_layout_that_does_not_fit_the_count() {
    let (mut item, ids) = item_with(3, 2);
    insert_placement(&mut item, 1, vec![ids[0]], Layout::FullWidth).expect("placed");
    assert!(
        set_placement(
            &mut item,
            1,
            Placement {
                photos: vec![ids[0], ids[1]],
                layout: Layout::FloatLeft,
            },
        )
        .is_err(),
        "INV-4: two photos are not a float"
    );
}

#[test]
fn set_placement_refuses_a_paragraph() {
    let (mut item, ids) = item_with(3, 1);
    assert!(
        set_placement(
            &mut item,
            0,
            Placement {
                photos: vec![ids[0]],
                layout: Layout::FullWidth,
            },
        )
        .is_err()
    );
}
