//! T090 — the same item and the same options always arrange the same way.
//!
//! Constitution IV in the one place it is easiest to lose: arrangement is a heuristic, and a
//! heuristic that reads a hash map's iteration order or a clock produces a different page every
//! time the editor presses the button. Then FR-024's byte-identical output stops being true for
//! any item that was arranged automatically.

mod support;

use newsbuilder_core::arrange::{ArrangeOptions, arrange_auto};
use newsbuilder_core::model::item::{Block, Layout, NewsItem, PhotoIntake, add_photos};
use newsbuilder_core::model::photo::PhotoOrigin;

use support::jpeg_bytes;

/// A deliberately awkward item: an odd number of paragraphs, mixed orientations, and more
/// photos than boundaries, so every branch of the arrangement is exercised.
fn mixed_item() -> NewsItem {
    let mut item = NewsItem::new();
    item.title = "Determinism".to_owned();
    item.body = (1..=5)
        .map(|n| Block::paragraph(format!("Paragraph {n}.")))
        .collect();
    item.normalise_paragraph_kinds();

    let shapes = [
        (400u32, 300u32),
        (300, 400),
        (300, 400),
        (600, 200),
        (500, 500),
        (300, 400),
        (400, 300),
    ];
    let intake = shapes
        .iter()
        .enumerate()
        .map(|(index, (width, height))| {
            PhotoIntake::from_bytes(
                format!("photo{}.jpg", index + 1),
                jpeg_bytes(*width, *height, index as u32 + 1),
                PhotoOrigin::Dropped,
            )
        })
        .collect();
    let added = add_photos(&mut item, intake);
    assert_eq!(added.ids.len(), shapes.len(), "{:?}", added.warnings);
    item
}

/// The arrangement, as a value that can be compared and printed.
fn shape(item: &NewsItem) -> Vec<String> {
    item.body
        .iter()
        .map(|block| match block {
            Block::Paragraph { text, kind } => format!("{kind:?}:{text}"),
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

fn options() -> ArrangeOptions {
    ArrangeOptions {
        replace_manual: false,
    }
}

#[test]
fn two_identical_items_arrange_identically() {
    let mut first = mixed_item();
    let mut second = mixed_item();
    arrange_auto(&mut first, options());
    arrange_auto(&mut second, options());
    assert_eq!(shape(&first), shape(&second));
}

#[test]
fn arranging_the_same_item_repeatedly_converges() {
    // The second run has nothing left to place, so it must change nothing at all. An
    // arrangement that reshuffled on every press would make the button unusable.
    let mut item = mixed_item();
    let first = arrange_auto(&mut item, options());
    let after_once = shape(&item);

    let second = arrange_auto(&mut item, options());
    assert_eq!(second.placed, 0, "everything was already placed");
    assert_eq!(shape(&item), after_once);
    assert_eq!(first.placed, 7);
}

#[test]
fn a_hundred_runs_agree() {
    // Cheap insurance against an iteration-order dependency that only shows up sometimes.
    let expected = {
        let mut item = mixed_item();
        arrange_auto(&mut item, options());
        shape(&item)
    };
    for run in 0..100 {
        let mut item = mixed_item();
        arrange_auto(&mut item, options());
        assert_eq!(shape(&item), expected, "run {run} differed");
    }
}

#[test]
fn replacing_manual_placements_is_deterministic_too() {
    let build = || {
        let mut item = mixed_item();
        item.body.insert(
            2,
            Block::placement(vec![item.photos[3].id], Layout::FloatRight),
        );
        item.normalise_paragraph_kinds();
        item
    };

    let mut first = build();
    let mut second = build();
    let a = arrange_auto(
        &mut first,
        ArrangeOptions {
            replace_manual: true,
        },
    );
    let b = arrange_auto(
        &mut second,
        ArrangeOptions {
            replace_manual: true,
        },
    );
    assert_eq!(shape(&first), shape(&second));
    assert_eq!(a.placed, b.placed);
    assert_eq!(a.replaced_manual, b.replaced_manual);
}

#[test]
fn the_result_does_not_depend_on_the_order_photos_were_added_in() {
    // Only on the order they are *in*: the photo list is what the editor sees and reorders, so
    // the arrangement follows that and nothing else.
    use newsbuilder_core::model::item::reorder_photos;

    let mut forwards = mixed_item();
    let mut backwards = mixed_item();
    let order: Vec<_> = backwards.photos.iter().map(|p| p.id).rev().collect();
    reorder_photos(&mut backwards, &order);
    let restored: Vec<_> = order.into_iter().rev().collect();
    reorder_photos(&mut backwards, &restored);

    arrange_auto(&mut forwards, options());
    arrange_auto(&mut backwards, options());
    assert_eq!(shape(&forwards), shape(&backwards));
}
