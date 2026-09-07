//! T089 — automatic arrangement places every photo, groups the surplus, and follows shape.
//!
//! FR-019 is one action: distribute the photos through the text and pick a layout for each.
//! Three things make it trustworthy enough to offer as one button — nothing is dropped, the
//! result respects paragraph boundaries rather than landing mid-sentence, and the layout it
//! picks has a reason (a landscape fills the width; portraits pair up rather than floating a
//! column of narrow images down the page).

mod support;

use newsbuilder_core::arrange::{ArrangeOptions, arrange_auto};
use newsbuilder_core::model::item::{
    Block, Layout, NewsItem, PhotoIntake, add_photos, insert_placement,
};
use newsbuilder_core::model::photo::{PhotoId, PhotoOrigin};

use support::jpeg_bytes;

/// Shapes, so a test can say what it means rather than repeating pixel counts.
#[derive(Clone, Copy)]
enum Shape {
    Landscape,
    Portrait,
}

impl Shape {
    fn dims(self) -> (u32, u32) {
        match self {
            Self::Landscape => (400, 300),
            Self::Portrait => (300, 400),
        }
    }
}

fn item_with(paragraphs: usize, shapes: &[Shape]) -> (NewsItem, Vec<PhotoId>) {
    let mut item = NewsItem::new();
    item.title = "Arrangement".to_owned();
    item.body = (1..=paragraphs)
        .map(|n| Block::paragraph(format!("Paragraph {n}.")))
        .collect();
    item.normalise_paragraph_kinds();

    let intake = shapes
        .iter()
        .enumerate()
        .map(|(index, shape)| {
            let (width, height) = shape.dims();
            PhotoIntake::from_bytes(
                format!("photo{}.jpg", index + 1),
                jpeg_bytes(width, height, index as u32 + 1),
                PhotoOrigin::Dropped,
            )
        })
        .collect();
    let added = add_photos(&mut item, intake);
    assert_eq!(added.ids.len(), shapes.len(), "{:?}", added.warnings);
    let ids = added.ids.clone();
    (item, ids)
}

/// Every photo id any placement holds, in body order, with repeats kept so a double placement
/// is visible.
fn placed(item: &NewsItem) -> Vec<PhotoId> {
    item.body
        .iter()
        .flat_map(|block| match block {
            Block::Placement { photos, .. } => photos.clone(),
            Block::Paragraph { .. } => Vec::new(),
        })
        .collect()
}

fn layouts(item: &NewsItem) -> Vec<Layout> {
    item.body
        .iter()
        .filter_map(|block| match block {
            Block::Placement { layout, .. } => Some(*layout),
            Block::Paragraph { .. } => None,
        })
        .collect()
}

fn arrange(item: &mut NewsItem) -> newsbuilder_core::arrange::ArrangeReport {
    arrange_auto(
        item,
        ArrangeOptions {
            replace_manual: false,
        },
    )
}

// -------------------------------------------------------------------------------------------
// Nothing is dropped
// -------------------------------------------------------------------------------------------

#[test]
fn every_photo_receives_a_placement() {
    let (mut item, ids) = item_with(3, &[Shape::Landscape; 3]);
    let report = arrange(&mut item);

    let mut got = placed(&item);
    got.sort();
    let mut want = ids.clone();
    want.sort();
    assert_eq!(got, want, "every photo, exactly once");
    assert_eq!(report.placed, 3);
    assert!(item.unused_photo_ids().is_empty());
    assert!(item.placements_resolve(), "INV-1");
    assert!(item.layouts_match_counts(), "INV-4");
}

#[test]
fn far_more_photos_than_paragraphs_still_places_all_of_them() {
    // The surplus case FR-019's edge case names: nine photos and one paragraph. They group
    // into rows rather than being dropped or stacked one per line down the page.
    let (mut item, ids) = item_with(1, &[Shape::Landscape; 9]);
    let report = arrange(&mut item);

    let mut got = placed(&item);
    got.sort();
    let mut want = ids.clone();
    want.sort();
    assert_eq!(got, want);
    assert_eq!(report.placed, 9);
    assert!(
        layouts(&item).contains(&Layout::Row),
        "the surplus grouped into rows: {:?}",
        layouts(&item)
    );
    assert!(item.layouts_match_counts(), "INV-4");
}

#[test]
fn an_item_with_no_paragraphs_at_all_still_places_its_photos() {
    let (mut item, ids) = item_with(0, &[Shape::Landscape; 2]);
    let report = arrange(&mut item);
    assert_eq!(report.placed, 2);
    let mut got = placed(&item);
    got.sort();
    let mut want = ids;
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn an_item_with_no_photos_is_left_alone() {
    let (mut item, _) = item_with(3, &[]);
    let before = item.body.clone();
    let report = arrange(&mut item);
    assert_eq!(report.placed, 0);
    assert_eq!(item.body, before);
}

// -------------------------------------------------------------------------------------------
// Paragraph boundaries are respected
// -------------------------------------------------------------------------------------------

#[test]
fn placements_land_between_paragraphs_and_never_split_the_text() {
    let (mut item, _) = item_with(5, &[Shape::Landscape; 3]);
    arrange(&mut item);

    // Every paragraph is still whole and still in order — arrangement inserts, it never edits.
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
            "Paragraph 4.",
            "Paragraph 5."
        ]
    );
}

#[test]
fn the_text_opens_with_a_paragraph_rather_than_a_photo() {
    // An item that starts with an image and no lead reads as an untitled picture. Where there
    // is text to lead with, the arrangement leads with it.
    let (mut item, _) = item_with(3, &[Shape::Landscape; 2]);
    arrange(&mut item);
    assert!(
        matches!(item.body[0], Block::Paragraph { .. }),
        "{:?}",
        item.body
    );
}

#[test]
fn the_lead_paragraph_is_still_the_lead_afterwards() {
    use newsbuilder_core::model::item::ParagraphKind;
    let (mut item, _) = item_with(3, &[Shape::Landscape; 2]);
    arrange(&mut item);
    let first_paragraph = item
        .body
        .iter()
        .find_map(|block| match block {
            Block::Paragraph { kind, .. } => Some(*kind),
            Block::Placement { .. } => None,
        })
        .expect("there are paragraphs");
    assert_eq!(first_paragraph, ParagraphKind::Lead);
}

#[test]
fn photos_are_spread_rather_than_piled_at_one_boundary() {
    let (mut item, _) = item_with(6, &[Shape::Landscape; 3]);
    arrange(&mut item);

    let positions: Vec<usize> = item
        .body
        .iter()
        .enumerate()
        .filter(|(_, block)| block.is_placement())
        .map(|(index, _)| index)
        .collect();
    assert_eq!(positions.len(), 3);
    for pair in positions.windows(2) {
        assert!(
            pair[1] > pair[0] + 1,
            "two placements ended up adjacent: {positions:?}"
        );
    }
}

// -------------------------------------------------------------------------------------------
// Layout follows shape
// -------------------------------------------------------------------------------------------

#[test]
fn a_landscape_photo_fills_the_width() {
    let (mut item, _) = item_with(3, &[Shape::Landscape]);
    arrange(&mut item);
    assert_eq!(layouts(&item), vec![Layout::FullWidth]);
}

#[test]
fn a_lone_portrait_floats_rather_than_filling_the_width() {
    // A portrait stretched to the container width is a column of face with text pushed off the
    // screen; floating it lets the text wrap alongside.
    let (mut item, _) = item_with(3, &[Shape::Portrait]);
    arrange(&mut item);
    let got = layouts(&item);
    assert!(
        got == vec![Layout::FloatLeft] || got == vec![Layout::FloatRight],
        "{got:?}"
    );
}

#[test]
fn neighbouring_portraits_pair_into_a_row() {
    let (mut item, _) = item_with(4, &[Shape::Portrait, Shape::Portrait]);
    arrange(&mut item);
    assert_eq!(layouts(&item), vec![Layout::Row]);
}

#[test]
fn layout_varies_with_orientation_within_one_item() {
    let (mut item, _) = item_with(
        6,
        &[
            Shape::Landscape,
            Shape::Portrait,
            Shape::Portrait,
            Shape::Landscape,
        ],
    );
    arrange(&mut item);
    let got = layouts(&item);
    assert!(
        got.contains(&Layout::FullWidth),
        "the landscapes fill the width: {got:?}"
    );
    assert!(
        got.contains(&Layout::Row),
        "the two portraits pair up: {got:?}"
    );
    assert!(item.layouts_match_counts(), "INV-4");
}

#[test]
fn successive_floats_alternate_sides() {
    // Three separated portraits all floated left would make a ragged column down one edge.
    let (mut item, _) = item_with(
        8,
        &[
            Shape::Portrait,
            Shape::Landscape,
            Shape::Portrait,
            Shape::Landscape,
            Shape::Portrait,
        ],
    );
    arrange(&mut item);
    let floats: Vec<Layout> = layouts(&item)
        .into_iter()
        .filter(|layout| layout.floats())
        .collect();
    assert!(floats.len() >= 2, "{floats:?}");
    for pair in floats.windows(2) {
        assert_ne!(pair[0], pair[1], "two floats in a row on the same side");
    }
}

// -------------------------------------------------------------------------------------------
// Photos that are already placed
// -------------------------------------------------------------------------------------------

#[test]
fn a_photo_that_is_already_placed_is_not_placed_a_second_time() {
    let (mut item, ids) = item_with(4, &[Shape::Landscape; 3]);
    insert_placement(&mut item, 1, vec![ids[0]], Layout::FloatLeft).expect("manual placement");

    let report = arrange(&mut item);

    assert_eq!(report.placed, 2, "only the two unplaced photos were placed");
    let got = placed(&item);
    assert_eq!(
        got.iter().filter(|id| **id == ids[0]).count(),
        1,
        "the manually placed photo appears once: {got:?}"
    );
}
