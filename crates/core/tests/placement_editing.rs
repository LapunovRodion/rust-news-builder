//! T112 — the five operations the placement cards call into (FR-018a – FR-018d).
//!
//! Deviation D-9 replaced typed markers with inline cards, so the editor never writes
//! `[image-left:3]` and never sees it. What they do instead is insert a card at the cursor,
//! drag it somewhere else, drop a second photo onto it, change its layout, or throw it away.
//! Those five gestures are these five functions, and the invariants they must not break are
//! INV-1 (a placement never names a photo the item does not hold) and INV-4 (a layout agrees
//! with how many photos it holds).

mod support;

use newsbuilder_core::model::item::{
    Block, Layout, NewsItem, PhotoIntake, add_photos, add_to_placement, insert_placement,
    move_placement, remove_placement, set_layout,
};
use newsbuilder_core::model::photo::{PhotoId, PhotoOrigin};

use support::jpeg_bytes;

fn item_with(count: usize) -> (NewsItem, Vec<PhotoId>) {
    let mut item = NewsItem::new();
    item.title = "Placement Editing".to_owned();
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
    let ids = added.ids.clone();
    item.body = vec![
        Block::paragraph("First."),
        Block::paragraph("Second."),
        Block::paragraph("Third."),
    ];
    item.normalise_paragraph_kinds();
    (item, ids)
}

/// A compact picture of the body, so a failure message shows the shape rather than a page of
/// `Debug`.
fn shape(item: &NewsItem) -> Vec<String> {
    item.body
        .iter()
        .map(|block| match block {
            Block::Paragraph { text, .. } => text.clone(),
            Block::Placement { photos, layout } => format!(
                "{}({})",
                layout.marker_keyword(),
                photos
                    .iter()
                    .map(|id| id.0.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        })
        .collect()
}

fn invariants_hold(item: &NewsItem) {
    assert!(item.placements_resolve(), "INV-1: {:?}", shape(item));
    assert!(item.layouts_match_counts(), "INV-4: {:?}", shape(item));
}

// -------------------------------------------------------------------------------------------
// insert_placement (FR-018a)
// -------------------------------------------------------------------------------------------

#[test]
fn a_placement_goes_in_where_the_cursor_was() {
    let (mut item, ids) = item_with(1);
    insert_placement(&mut item, 1, vec![ids[0]], Layout::FullWidth).expect("between paragraphs");
    assert_eq!(
        shape(&item),
        vec!["First.", "image(1)", "Second.", "Third."]
    );
    invariants_hold(&item);
}

#[test]
fn a_placement_can_go_at_the_very_front_and_the_very_end() {
    let (mut item, ids) = item_with(2);
    insert_placement(&mut item, 0, vec![ids[0]], Layout::FullWidth).expect("the front");
    let end = item.body.len();
    insert_placement(&mut item, end, vec![ids[1]], Layout::FloatRight).expect("the end");
    assert_eq!(
        shape(&item),
        vec!["image(1)", "First.", "Second.", "Third.", "image-right(2)"]
    );
    invariants_hold(&item);
}

#[test]
fn inserting_past_the_end_is_refused_rather_than_clamped() {
    let (mut item, ids) = item_with(1);
    let error = insert_placement(&mut item, 99, vec![ids[0]], Layout::FullWidth)
        .expect_err("99 is not a position in a four-block body");
    assert!(error.to_string().contains("99"), "{error}");
    assert_eq!(item.body.len(), 3, "nothing was inserted");
}

#[test]
fn inserting_a_photo_the_item_does_not_hold_is_refused() {
    let (mut item, _) = item_with(1);
    let error = insert_placement(&mut item, 0, vec![PhotoId(42)], Layout::FullWidth)
        .expect_err("INV-1 forbids it");
    assert!(error.to_string().contains("42"), "{error}");
    invariants_hold(&item);
}

#[test]
fn inserting_nothing_is_refused() {
    let (mut item, _) = item_with(1);
    assert!(insert_placement(&mut item, 0, vec![], Layout::Row).is_err());
}

#[test]
fn several_photos_under_a_single_photo_layout_become_a_row() {
    // The drag gesture can deliver a multi-selection onto a point in the text. Refusing it
    // would be pedantry; a row is plainly what was meant (INV-4).
    let (mut item, ids) = item_with(3);
    insert_placement(&mut item, 1, ids.clone(), Layout::FloatLeft).expect("promoted");
    assert_eq!(shape(&item)[1], "images(1,2,3)");
    invariants_hold(&item);
}

#[test]
fn a_row_of_one_is_left_as_a_row() {
    // Deviation D-12: the reference does not normalise `[images:1]`, and neither does the
    // editor. What the editor asked for is what they get.
    let (mut item, ids) = item_with(1);
    insert_placement(&mut item, 0, vec![ids[0]], Layout::Row).expect("accepted");
    assert_eq!(shape(&item)[0], "images(1)");
    invariants_hold(&item);
}

#[test]
fn the_same_photo_twice_in_one_placement_is_kept_once() {
    let (mut item, ids) = item_with(2);
    insert_placement(&mut item, 0, vec![ids[0], ids[1], ids[0]], Layout::Row).expect("accepted");
    assert_eq!(shape(&item)[0], "images(1,2)");
}

// -------------------------------------------------------------------------------------------
// move_placement (FR-018d)
// -------------------------------------------------------------------------------------------

#[test]
fn a_card_dragged_down_the_page_carries_its_photos_and_its_layout() {
    let (mut item, ids) = item_with(2);
    insert_placement(&mut item, 1, vec![ids[0], ids[1]], Layout::Row).expect("accepted");
    assert_eq!(
        shape(&item),
        vec!["First.", "images(1,2)", "Second.", "Third."]
    );

    move_placement(&mut item, 1, 3).expect("moved");

    assert_eq!(
        shape(&item),
        vec!["First.", "Second.", "Third.", "images(1,2)"]
    );
    invariants_hold(&item);
}

#[test]
fn a_card_dragged_up_the_page_lands_where_it_was_dropped() {
    let (mut item, ids) = item_with(1);
    insert_placement(&mut item, 3, vec![ids[0]], Layout::FloatRight).expect("accepted");
    move_placement(&mut item, 3, 0).expect("moved");
    assert_eq!(
        shape(&item),
        vec!["image-right(1)", "First.", "Second.", "Third."]
    );
}

#[test]
fn moving_a_card_onto_its_own_position_changes_nothing() {
    let (mut item, ids) = item_with(1);
    insert_placement(&mut item, 1, vec![ids[0]], Layout::FullWidth).expect("accepted");
    let before = shape(&item);
    move_placement(&mut item, 1, 1).expect("a no-op is not an error");
    assert_eq!(shape(&item), before);
}

#[test]
fn moving_a_paragraph_is_refused() {
    // Only cards are draggable. Text moves by being typed somewhere else.
    let (mut item, _) = item_with(1);
    let error = move_placement(&mut item, 0, 2).expect_err("block 0 is a paragraph");
    assert!(error.to_string().contains('0'), "{error}");
}

#[test]
fn the_lead_paragraph_follows_the_body_after_a_move() {
    use newsbuilder_core::model::item::ParagraphKind;
    let (mut item, ids) = item_with(1);
    insert_placement(&mut item, 0, vec![ids[0]], Layout::FullWidth).expect("accepted");
    move_placement(&mut item, 0, 3).expect("moved");
    assert!(matches!(
        item.body[0],
        Block::Paragraph {
            kind: ParagraphKind::Lead,
            ..
        }
    ));
}

// -------------------------------------------------------------------------------------------
// remove_placement (FR-018b)
// -------------------------------------------------------------------------------------------

#[test]
fn removing_a_card_leaves_the_photos_in_the_item() {
    // The card goes; the photograph is still there to be placed again. Anything else would
    // make the remove control a data-loss button.
    let (mut item, ids) = item_with(2);
    insert_placement(&mut item, 1, vec![ids[0], ids[1]], Layout::Row).expect("accepted");
    remove_placement(&mut item, 1).expect("removed");

    assert_eq!(shape(&item), vec!["First.", "Second.", "Third."]);
    assert_eq!(item.photos.len(), 2);
    assert_eq!(item.unused_photo_ids().len(), 2);
    invariants_hold(&item);
}

#[test]
fn removing_a_paragraph_through_this_door_is_refused() {
    let (mut item, _) = item_with(1);
    assert!(remove_placement(&mut item, 0).is_err());
    assert_eq!(item.body.len(), 3);
}

// -------------------------------------------------------------------------------------------
// add_to_placement (FR-018c) — promotion
// -------------------------------------------------------------------------------------------

#[test]
fn dropping_a_photo_onto_a_full_width_card_promotes_it_to_a_row() {
    let (mut item, ids) = item_with(2);
    insert_placement(&mut item, 1, vec![ids[0]], Layout::FullWidth).expect("accepted");

    add_to_placement(&mut item, 1, ids[1]).expect("promoted");

    assert_eq!(shape(&item)[1], "images(1,2)");
    invariants_hold(&item);
}

#[test]
fn dropping_a_photo_onto_a_floated_card_promotes_it_too() {
    let (mut item, ids) = item_with(2);
    insert_placement(&mut item, 1, vec![ids[0]], Layout::FloatLeft).expect("accepted");
    add_to_placement(&mut item, 1, ids[1]).expect("promoted");
    assert_eq!(shape(&item)[1], "images(1,2)");
}

#[test]
fn a_row_simply_grows() {
    let (mut item, ids) = item_with(3);
    insert_placement(&mut item, 1, vec![ids[0], ids[1]], Layout::Row).expect("accepted");
    add_to_placement(&mut item, 1, ids[2]).expect("grown");
    assert_eq!(shape(&item)[1], "images(1,2,3)");
}

#[test]
fn a_photo_already_in_the_card_is_not_added_twice() {
    let (mut item, ids) = item_with(2);
    insert_placement(&mut item, 1, vec![ids[0], ids[1]], Layout::Row).expect("accepted");
    add_to_placement(&mut item, 1, ids[0]).expect("a no-op, not an error");
    assert_eq!(shape(&item)[1], "images(1,2)");
}

#[test]
fn adding_a_photo_the_item_does_not_hold_is_refused() {
    let (mut item, ids) = item_with(1);
    insert_placement(&mut item, 1, vec![ids[0]], Layout::FullWidth).expect("accepted");
    let error = add_to_placement(&mut item, 1, PhotoId(42)).expect_err("INV-1 forbids it");
    assert!(error.to_string().contains("42"), "{error}");
    invariants_hold(&item);
}

#[test]
fn adding_to_a_paragraph_is_refused() {
    let (mut item, ids) = item_with(1);
    assert!(add_to_placement(&mut item, 0, ids[0]).is_err());
}

// -------------------------------------------------------------------------------------------
// set_layout (FR-018b) — demotion
// -------------------------------------------------------------------------------------------

#[test]
fn the_layout_control_switches_a_single_photo_card_between_all_four() {
    let (mut item, ids) = item_with(1);
    insert_placement(&mut item, 1, vec![ids[0]], Layout::FullWidth).expect("accepted");
    for layout in [
        Layout::FloatLeft,
        Layout::FloatRight,
        Layout::Row,
        Layout::FullWidth,
    ] {
        set_layout(&mut item, 1, layout).expect("a single photo suits every layout");
        match &item.body[1] {
            Block::Placement { layout: got, .. } => assert_eq!(*got, layout),
            other => panic!("expected a placement, got {other:?}"),
        }
    }
    invariants_hold(&item);
}

#[test]
fn a_row_of_several_cannot_be_made_into_a_single_photo_layout() {
    // INV-4. The card's control disables these; the core refuses them anyway, because a UI
    // that forgets must not be able to corrupt the item.
    let (mut item, ids) = item_with(3);
    insert_placement(&mut item, 1, ids.clone(), Layout::Row).expect("accepted");
    for layout in [Layout::FullWidth, Layout::FloatLeft, Layout::FloatRight] {
        let error = set_layout(&mut item, 1, layout).expect_err("three photos are not one");
        assert!(error.to_string().contains('3'), "{error}");
    }
    assert_eq!(shape(&item)[1], "images(1,2,3)");
    invariants_hold(&item);
}

#[test]
fn setting_the_layout_a_card_already_has_is_a_no_op() {
    let (mut item, ids) = item_with(2);
    insert_placement(&mut item, 1, ids.clone(), Layout::Row).expect("accepted");
    set_layout(&mut item, 1, Layout::Row).expect("no change is not an error");
    assert_eq!(shape(&item)[1], "images(1,2)");
}

#[test]
fn setting_a_layout_on_a_paragraph_is_refused() {
    let (mut item, _) = item_with(1);
    assert!(set_layout(&mut item, 0, Layout::Row).is_err());
}

// -------------------------------------------------------------------------------------------
// The invariants, over a long sequence
// -------------------------------------------------------------------------------------------

#[test]
fn a_long_editing_session_never_breaks_an_invariant() {
    let (mut item, ids) = item_with(5);

    insert_placement(&mut item, 1, vec![ids[0]], Layout::FullWidth).expect("ok");
    add_to_placement(&mut item, 1, ids[1]).expect("ok");
    add_to_placement(&mut item, 1, ids[2]).expect("ok");
    invariants_hold(&item);

    insert_placement(&mut item, 0, vec![ids[3]], Layout::FloatRight).expect("ok");
    invariants_hold(&item);

    move_placement(&mut item, 0, 4).expect("ok");
    invariants_hold(&item);

    set_layout(&mut item, 4, Layout::FloatLeft).expect("still one photo");
    invariants_hold(&item);

    remove_placement(&mut item, 4).expect("ok");
    invariants_hold(&item);

    let end = item.body.len();
    insert_placement(&mut item, end, vec![ids[4]], Layout::Row).expect("ok");
    invariants_hold(&item);

    // Every photo is still in the item, whatever happened to the cards.
    assert_eq!(item.photos.len(), 5);
    assert!(item.photo_ids_are_unique(), "INV-3");
    assert!(item.file_names_are_unique(), "INV-5");
}
