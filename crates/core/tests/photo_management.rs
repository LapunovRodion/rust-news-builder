//! T079 — reordering and removal carry placements with them (FR-009, INV-1).
//!
//! The property under test is the one an editor notices immediately when it breaks: a placement
//! must keep pointing at the photograph the editor put there, whatever else moves. That is why
//! a placement holds a [`PhotoId`] and not a position in the photo list — this file is what
//! stops that decision from quietly eroding.

mod support;

use newsbuilder_core::error::Warning;
use newsbuilder_core::model::item::{
    Block, Layout, NewsItem, PhotoIntake, add_photos, remove_photo, rename_photo, reorder_photos,
};
use newsbuilder_core::model::photo::{PhotoId, PhotoOrigin};

use support::jpeg_bytes;

/// An item holding `count` photos named `photo1.jpg` … and nothing else.
fn item_with(count: usize) -> NewsItem {
    let mut item = NewsItem::new();
    item.title = "Photo Management".to_owned();
    let intake = (1..=count)
        .map(|n| {
            PhotoIntake::from_bytes(
                format!("photo{n}.jpg"),
                jpeg_bytes(120, 90, n as u32),
                PhotoOrigin::Dropped,
            )
        })
        .collect();
    let added = add_photos(&mut item, intake);
    assert_eq!(added.ids.len(), count, "{:?}", added.warnings);
    item
}

/// The file names a placement refers to, resolved through the item.
fn placed_names(item: &NewsItem, at: usize) -> Vec<String> {
    match &item.body[at] {
        Block::Placement { photos, .. } => photos
            .iter()
            .map(|id| {
                item.photo(*id)
                    .map(|p| p.file_name.clone())
                    .unwrap_or_else(|| format!("<dangling {id}>"))
            })
            .collect(),
        Block::Paragraph { text, .. } => panic!("block {at} is a paragraph: {text}"),
    }
}

fn id_of(item: &NewsItem, file_name: &str) -> PhotoId {
    item.photos
        .iter()
        .find(|p| p.file_name == file_name)
        .unwrap_or_else(|| panic!("no photo named {file_name}"))
        .id
}

// -------------------------------------------------------------------------------------------
// Reordering
// -------------------------------------------------------------------------------------------

#[test]
fn reordering_the_photo_list_leaves_every_placement_pointing_where_it_did() {
    let mut item = item_with(4);
    let (a, b, c) = (
        id_of(&item, "photo1.jpg"),
        id_of(&item, "photo2.jpg"),
        id_of(&item, "photo3.jpg"),
    );
    item.body = vec![
        Block::paragraph("Lead."),
        Block::placement(vec![a], Layout::FullWidth),
        Block::paragraph("Middle."),
        Block::placement(vec![b, c], Layout::Row),
    ];
    item.normalise_paragraph_kinds();

    reorder_photos(&mut item, &[c, a, b]);

    assert_eq!(placed_names(&item, 1), vec!["photo1.jpg"]);
    assert_eq!(placed_names(&item, 3), vec!["photo2.jpg", "photo3.jpg"]);
    assert!(item.placements_resolve(), "INV-1");
}

#[test]
fn reordering_actually_reorders_the_list() {
    let mut item = item_with(3);
    let (a, b, c) = (
        id_of(&item, "photo1.jpg"),
        id_of(&item, "photo2.jpg"),
        id_of(&item, "photo3.jpg"),
    );
    reorder_photos(&mut item, &[c, b, a]);
    let names: Vec<&str> = item.photos.iter().map(|p| p.file_name.as_str()).collect();
    assert_eq!(names, vec!["photo3.jpg", "photo2.jpg", "photo1.jpg"]);
}

#[test]
fn a_partial_order_moves_what_it_names_and_keeps_the_rest_behind_it() {
    // The desktop list can hand back a selection rather than the whole list; anything it does
    // not mention must not be dropped, and must not be shuffled either.
    let mut item = item_with(5);
    let d = id_of(&item, "photo4.jpg");
    let b = id_of(&item, "photo2.jpg");

    reorder_photos(&mut item, &[d, b]);

    let names: Vec<&str> = item.photos.iter().map(|p| p.file_name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "photo4.jpg",
            "photo2.jpg",
            "photo1.jpg",
            "photo3.jpg",
            "photo5.jpg"
        ]
    );
}

#[test]
fn an_order_naming_an_unknown_photo_ignores_it_rather_than_losing_the_others() {
    let mut item = item_with(2);
    let a = id_of(&item, "photo1.jpg");
    reorder_photos(&mut item, &[PhotoId(9_999), a]);
    assert_eq!(item.photos.len(), 2);
    assert_eq!(item.photos[0].file_name, "photo1.jpg");
}

// -------------------------------------------------------------------------------------------
// Removal
// -------------------------------------------------------------------------------------------

#[test]
fn removing_a_photo_takes_it_out_of_the_placement_that_held_it() {
    let mut item = item_with(3);
    let (a, b, c) = (
        id_of(&item, "photo1.jpg"),
        id_of(&item, "photo2.jpg"),
        id_of(&item, "photo3.jpg"),
    );
    item.body = vec![Block::placement(vec![a, b, c], Layout::Row)];

    let warnings = remove_photo(&mut item, b);

    assert_eq!(placed_names(&item, 0), vec!["photo1.jpg", "photo3.jpg"]);
    assert!(item.placements_resolve(), "INV-1");
    assert!(
        warnings.is_empty(),
        "a row with photos left needs no warning: {warnings:?}"
    );
}

#[test]
fn a_row_worn_down_to_one_photo_becomes_a_full_width_placement() {
    // INV-4 as the editor experiences it: a "row" of one is not a row.
    let mut item = item_with(2);
    let (a, b) = (id_of(&item, "photo1.jpg"), id_of(&item, "photo2.jpg"));
    item.body = vec![Block::placement(vec![a, b], Layout::Row)];

    remove_photo(&mut item, a);

    match &item.body[0] {
        Block::Placement { photos, layout } => {
            assert_eq!(photos.len(), 1);
            assert_eq!(*layout, Layout::FullWidth);
        }
        other => panic!("expected a placement, got {other:?}"),
    }
    assert!(item.layouts_match_counts(), "INV-4");
}

#[test]
fn a_placement_left_with_nothing_is_dropped_and_warned_about() {
    let mut item = item_with(2);
    let (a, b) = (id_of(&item, "photo1.jpg"), id_of(&item, "photo2.jpg"));
    item.body = vec![
        Block::paragraph("Lead."),
        Block::placement(vec![a], Layout::FullWidth),
        Block::paragraph("Tail."),
        Block::placement(vec![b], Layout::FloatRight),
    ];
    item.normalise_paragraph_kinds();

    let warnings = remove_photo(&mut item, a);

    assert_eq!(
        item.body.len(),
        3,
        "the emptied placement went: {:?}",
        item.body
    );
    assert!(matches!(item.body[0], Block::Paragraph { .. }));
    assert!(matches!(item.body[1], Block::Paragraph { .. }));
    assert!(item.body[2].is_placement());
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w, Warning::PhotoSkipped { name, .. } if name == "photo1.jpg")),
        "the editor is told which photo took the placement with it: {warnings:?}"
    );
}

#[test]
fn removing_a_photo_never_re_points_a_placement_at_a_different_one() {
    // The failure this guards against: a placement that stored a *position* would silently
    // slide onto its neighbour when an earlier photo went.
    let mut item = item_with(3);
    let (a, c) = (id_of(&item, "photo1.jpg"), id_of(&item, "photo3.jpg"));
    item.body = vec![Block::placement(vec![c], Layout::FullWidth)];

    remove_photo(&mut item, a);

    assert_eq!(placed_names(&item, 0), vec!["photo3.jpg"]);
    assert_eq!(item.photos.len(), 2);
}

#[test]
fn removing_a_photo_that_is_not_there_changes_nothing() {
    let mut item = item_with(2);
    let before = item.photos.len();
    let warnings = remove_photo(&mut item, PhotoId(9_999));
    assert_eq!(item.photos.len(), before);
    assert!(warnings.is_empty());
}

#[test]
fn the_lead_paragraph_survives_a_removal_that_reshapes_the_body() {
    use newsbuilder_core::model::item::ParagraphKind;
    let mut item = item_with(1);
    let a = id_of(&item, "photo1.jpg");
    item.body = vec![
        Block::placement(vec![a], Layout::FullWidth),
        Block::paragraph("Lead."),
    ];
    item.normalise_paragraph_kinds();

    remove_photo(&mut item, a);

    assert!(matches!(
        item.body[0],
        Block::Paragraph {
            kind: ParagraphKind::Lead,
            ..
        }
    ));
}

// -------------------------------------------------------------------------------------------
// Renaming
// -------------------------------------------------------------------------------------------

#[test]
fn renaming_keeps_the_identity_and_therefore_the_placement() {
    let mut item = item_with(2);
    let a = id_of(&item, "photo1.jpg");
    item.body = vec![Block::placement(vec![a], Layout::FullWidth)];

    rename_photo(&mut item, a, "opening-shot.jpg").expect("a free name is accepted");

    assert_eq!(placed_names(&item, 0), vec!["opening-shot.jpg"]);
    assert_eq!(
        item.photo(a).map(|p| p.file_name.as_str()),
        Some("opening-shot.jpg")
    );
}

#[test]
fn renaming_onto_a_name_another_photo_holds_is_refused() {
    let mut item = item_with(2);
    let a = id_of(&item, "photo1.jpg");
    let error = rename_photo(&mut item, a, "photo2.jpg").expect_err("INV-5 forbids it");
    assert!(
        error.to_string().contains("photo2.jpg"),
        "the message names the collision: {error}"
    );
    assert_eq!(
        item.photo(a).map(|p| p.file_name.as_str()),
        Some("photo1.jpg"),
        "a refused rename changes nothing"
    );
    assert!(item.file_names_are_unique(), "INV-5");
}

#[test]
fn renaming_a_photo_to_the_name_it_already_has_is_allowed() {
    let mut item = item_with(1);
    let a = id_of(&item, "photo1.jpg");
    rename_photo(&mut item, a, "photo1.jpg").expect("renaming to itself is a no-op, not a clash");
}

#[test]
fn renaming_to_something_that_is_not_an_image_is_refused() {
    let mut item = item_with(1);
    let a = id_of(&item, "photo1.jpg");
    let error = rename_photo(&mut item, a, "photo1.pdf").expect_err("FR-011 lists the formats");
    assert!(error.to_string().contains("photo1.pdf"), "{error}");
}

#[test]
fn renaming_a_photo_that_is_not_there_is_refused_by_name() {
    let mut item = item_with(1);
    let error = rename_photo(&mut item, PhotoId(9_999), "x.jpg").expect_err("no such photo");
    assert!(
        error.to_string().contains("9999") || error.to_string().contains("#9999"),
        "{error}"
    );
}
