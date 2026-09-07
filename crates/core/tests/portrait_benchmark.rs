//! T072 — the SC-003 benchmark: twenty portrait photographs, and the head survives in every one.
//!
//! `fixtures/inputs/portraits/heads.json` records where the subject's head is in each of the
//! twenty frames, so "the head survives" is a rectangle containment question rather than a
//! judgement call. The file is generated alongside the photographs by
//! `tools/capture-reference/make_inputs.py`, which draws the head it then records.
//!
//! Two claims are measured, because SC-003 and FR-013 make two different promises:
//!
//! 1. **Nothing is cropped without the editor's action.** A photo added to an item carries no
//!    crop, so its head is trivially intact — this is the promise FR-013 actually makes, and
//!    the one an editor relies on.
//! 2. **A suggested crop keeps the head too.** When the editor does pick a shape, the frame
//!    `suggest_crop` offers still contains the whole head, for every photograph and every shape
//!    the application offers.
//!
//! Claim 2 is what retired research R5's original "one quarter off the top" rule: at 3:2 it cut
//! the head off ten of these twenty, and at 1:1 five of them. The rule now takes the vertical
//! excess entirely off the bottom. See `crates/core/src/photo/frame.rs`.

mod support;

use newsbuilder_core::model::photo::{CropRect, PhotoSource};
use newsbuilder_core::photo::{self, AspectRatio};
use support::input_dir;

/// One annotated photograph.
#[derive(Debug, serde::Deserialize)]
struct Annotated {
    file_name: String,
    width: u32,
    height: u32,
    head: Head,
}

#[derive(Debug, serde::Deserialize)]
struct Head {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl Head {
    /// Whether the whole head lies inside `frame`.
    fn survives(&self, frame: CropRect) -> bool {
        self.x >= frame.x
            && self.y >= frame.y
            && self.x + self.width <= frame.x + frame.width
            && self.y + self.height <= frame.y + frame.height
    }
}

/// The shapes the crop control offers. A benchmark against a shape the product never asks for
/// would prove nothing.
const OFFERED_SHAPES: [AspectRatio; 3] = [
    AspectRatio::SQUARE,
    AspectRatio::LANDSCAPE_3_2,
    AspectRatio::WIDE_16_9,
];

fn benchmark_set() -> Vec<Annotated> {
    let path = input_dir("portraits").join("heads.json");
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing {}: {e}\nregenerate with `cargo run -p capture-reference`",
            path.display()
        )
    });
    serde_json::from_str(&raw).expect("heads.json is a list of annotated photographs")
}

#[test]
fn the_benchmark_set_is_twenty_portraits_with_their_heads_annotated() {
    let set = benchmark_set();
    assert_eq!(set.len(), 20, "SC-003 names twenty photographs");
    for photograph in &set {
        // One of the twenty is square. It is still a photograph of a person framed head-up,
        // which is what the benchmark is about; what it must not be is a landscape.
        assert!(
            photograph.height >= photograph.width,
            "{} is a landscape, not a portrait: {}x{}",
            photograph.file_name,
            photograph.width,
            photograph.height
        );
        let whole = CropRect {
            x: 0,
            y: 0,
            width: photograph.width,
            height: photograph.height,
        };
        assert!(
            photograph.head.survives(whole),
            "{}'s annotated head falls outside the photograph",
            photograph.file_name
        );
    }
}

#[test]
fn a_photo_added_to_an_item_is_not_cropped_at_all() {
    // Claim 1, and the reason SC-003 holds in the shipped product: layouts scale to fit, so
    // nothing crops until the editor asks (research R5).
    let mut item = support::item("portraits").expect("the portraits fixture is captured");
    assert_eq!(item.photos.len(), 20);
    for photo in &item.photos {
        assert_eq!(
            photo.adjust.crop, None,
            "{} arrived cropped",
            photo.file_name
        );
    }

    // And a build of them changes nothing about that.
    let ids: Vec<_> = item.photos.iter().map(|p| p.id).collect();
    use newsbuilder_core::model::item::{Block, Layout};
    item.body = ids
        .iter()
        .map(|id| Block::placement(vec![*id], Layout::FullWidth))
        .collect();
    let ctx = newsbuilder_core::build::BuildContext::preview(item.slug.clone());
    let out = newsbuilder_core::build::build(&item, &ctx, &newsbuilder_core::build::EmbeddedBytes)
        .expect("the item builds");
    assert_eq!(out.processed.len(), 20);

    let by_name: std::collections::BTreeMap<_, _> = benchmark_set()
        .into_iter()
        .map(|a| (a.file_name.clone(), a))
        .collect();
    for (photo, processed) in item.photos.iter().zip(&out.processed) {
        let annotated = by_name
            .get(&photo.file_name)
            .unwrap_or_else(|| panic!("{} is not in heads.json", photo.file_name));
        // Nothing was cropped, so the published photo is the whole frame, only scaled.
        assert_eq!(
            (annotated.width, annotated.height),
            (photo.dimensions.0, photo.dimensions.1),
        );
        assert_eq!(
            processed.dimensions.1 * annotated.width,
            processed.dimensions.0 * annotated.height,
            "{} changed shape: {:?} from {}x{}",
            photo.file_name,
            processed.dimensions,
            annotated.width,
            annotated.height
        );
    }
}

#[test]
fn the_suggested_crop_keeps_the_head_in_every_photograph_and_every_shape() {
    // Claim 2. This is the assertion that fails if the framing rule is ever weakened back
    // toward centring.
    let mut failures: Vec<String> = Vec::new();

    for photograph in benchmark_set() {
        for shape in OFFERED_SHAPES {
            let Some(frame) = photo::default_frame((photograph.width, photograph.height), shape)
            else {
                continue;
            };
            assert!(
                frame.fits_within((photograph.width, photograph.height)),
                "{} into {shape:?} produced {frame:?}, outside the photograph (INV-6)",
                photograph.file_name
            );
            if !photograph.head.survives(frame) {
                failures.push(format!(
                    "{} into {}:{} — head at ({}, {}) {}x{}, frame {:?}",
                    photograph.file_name,
                    shape.width,
                    shape.height,
                    photograph.head.x,
                    photograph.head.y,
                    photograph.head.width,
                    photograph.head.height,
                    frame,
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "SC-003: the default framing cut the head off {} of {} cases:\n  {}",
        failures.len(),
        20 * OFFERED_SHAPES.len(),
        failures.join("\n  ")
    );
}

#[test]
fn the_suggestion_reaches_the_photos_through_suggest_crop_too() {
    // `default_frame` is the arithmetic; `suggest_crop` is what the command calls. They must
    // not drift apart, so the benchmark is repeated through the real entry point.
    let item = support::item("portraits").expect("the portraits fixture is captured");
    let by_name: std::collections::BTreeMap<_, _> = benchmark_set()
        .into_iter()
        .map(|a| (a.file_name.clone(), a))
        .collect();

    for photo_record in &item.photos {
        let annotated = by_name
            .get(&photo_record.file_name)
            .unwrap_or_else(|| panic!("{} is not in heads.json", photo_record.file_name));
        for shape in OFFERED_SHAPES {
            let Some(frame) = photo::suggest_crop(photo_record, shape) else {
                continue;
            };
            assert!(
                annotated.head.survives(frame),
                "{} into {shape:?} gave {frame:?} and lost the head",
                photo_record.file_name
            );
        }
    }
}

#[test]
fn the_framing_rule_beats_centring_on_this_set() {
    // The comparison that justifies having a rule at all. A centred crop is what a naive
    // implementation does; it is measured here so the benefit is a number rather than a claim.
    let mut centred_failures = 0usize;
    for photograph in benchmark_set() {
        for shape in OFFERED_SHAPES {
            let Some(frame) = photo::default_frame((photograph.width, photograph.height), shape)
            else {
                continue;
            };
            let excess = photograph.height - frame.height;
            let centred = CropRect {
                y: excess / 2,
                ..frame
            };
            if !photograph.head.survives(centred) {
                centred_failures += 1;
            }
        }
    }
    assert!(
        centred_failures > 0,
        "if centring loses no heads either, this benchmark set proves nothing"
    );
}

#[test]
fn the_benchmark_photographs_are_the_ones_on_disk() {
    // Guards against heads.json drifting away from the images it annotates: a stale annotation
    // would make every assertion above vacuous.
    let item = support::item("portraits").expect("the portraits fixture is captured");
    let by_name: std::collections::BTreeMap<_, _> = benchmark_set()
        .into_iter()
        .map(|a| (a.file_name.clone(), a))
        .collect();
    assert_eq!(item.photos.len(), by_name.len());
    for photo_record in &item.photos {
        let annotated = by_name
            .get(&photo_record.file_name)
            .unwrap_or_else(|| panic!("{} is not in heads.json", photo_record.file_name));
        assert_eq!(
            photo_record.dimensions,
            (annotated.width, annotated.height),
            "{} is annotated at the wrong size",
            photo_record.file_name
        );
        assert!(
            matches!(photo_record.source, PhotoSource::Bytes(_)),
            "the fixture loader supplies bytes"
        );
    }
}
