//! The eight invariants from data-model.md, asserted on real fixture items (T018).
//!
//! Each is a property an item must hold after any operation. They are checked here as
//! predicates on the model rather than as a side effect of some other test, so a failure names
//! the invariant that broke.

mod support;

use newsbuilder_core::import::{attach_photos, import_document};
use newsbuilder_core::model::appearance::ImageBudget;
use newsbuilder_core::model::item::NewsItem;
use newsbuilder_core::model::photo::CropRect;
use newsbuilder_core::model::server::{CredentialRef, Slug};

/// Every fixture case, imported and populated the way the CLI would.
///
/// Word cases take their photos from the package, so the `images/` folder — which exists only
/// so the reference had something to render — is not attached to them.
fn imported_items() -> Vec<(String, NewsItem, Vec<newsbuilder_core::Warning>)> {
    support::all_cases()
        .into_iter()
        .filter_map(|case| {
            let (bytes, format) = support::document(&case)?;
            let mut imported = import_document(&bytes, format).ok()?;
            let mut warnings = imported.warnings.clone();
            if !matches!(format, newsbuilder_core::model::item::SourceFormat::Docx) {
                warnings.extend(attach_photos(&mut imported.item, support::images(&case)));
            }
            Some((case, imported.item, warnings))
        })
        .collect()
}

#[test]
fn inv_1_every_unresolved_placement_is_accounted_for_by_a_warning() {
    // INV-1 is a validation target, not an unconditional guarantee: data-model.md reports a
    // marker naming a photo that does not exist as a warning, not a failure (FR-034). What
    // must never happen is an unresolved placement nobody was told about.
    for (case, item, warnings) in imported_items() {
        if item.placements_resolve() {
            continue;
        }
        assert!(
            warnings
                .iter()
                .any(|w| matches!(w, newsbuilder_core::Warning::MissingPhoto { .. })),
            "INV-1 broken in `{case}` with no MissingPhoto warning to explain it"
        );
    }
}

#[test]
fn inv_1_holds_outright_wherever_the_document_is_consistent() {
    // `missing-photo` is the one fixture written to break it, so everything else must hold.
    for (case, item, _) in imported_items() {
        if case == "missing-photo" || item.photos.is_empty() {
            continue;
        }
        assert!(
            item.placements_resolve(),
            "INV-1 broken in `{case}`: a placement references a photo the item does not hold"
        );
    }
}

#[test]
fn inv_2_every_slug_is_well_formed() {
    for (case, item, _) in imported_items() {
        assert!(
            Slug::is_well_formed(item.slug.as_str()),
            "INV-2 broken in `{case}`: slug {:?}",
            item.slug.as_str()
        );
    }
}

#[test]
fn inv_3_photo_ids_are_unique() {
    for (case, item, _) in imported_items() {
        assert!(item.photo_ids_are_unique(), "INV-3 broken in `{case}`");
    }
}

#[test]
fn inv_4_every_layout_agrees_with_its_photo_count() {
    for (case, item, _) in imported_items() {
        assert!(
            item.layouts_match_counts(),
            "INV-4 broken in `{case}`: a single-photo layout holds several photos"
        );
    }
}

#[test]
fn inv_5_file_names_are_unique() {
    for (case, item, _) in imported_items() {
        assert!(
            item.file_names_are_unique(),
            "INV-5 broken in `{case}`: {:?}",
            item.photos.iter().map(|p| &p.file_name).collect::<Vec<_>>()
        );
    }
}

#[test]
fn inv_5_a_word_document_with_repeated_part_names_still_yields_unique_names() {
    // Word packages reuse `image1.png` shapes across parts; two photos must not collide.
    let (bytes, _) = support::document("word-den-znaniy").expect("the case is present");
    let imported = import_document(&bytes, newsbuilder_core::model::item::SourceFormat::Docx)
        .expect("imports");
    assert!(imported.item.file_names_are_unique());
    assert!(imported.item.photo_ids_are_unique());
}

#[test]
fn inv_6_every_crop_lies_inside_its_photo() {
    for (case, item, _) in imported_items() {
        assert!(item.crops_are_within_bounds(), "INV-6 broken in `{case}`");
    }
}

#[test]
fn inv_6_is_enforced_on_assignment() {
    let mut item = NewsItem::new();
    attach_photos(&mut item, support::images("portraits"));
    let photo = item
        .photos
        .first_mut()
        .expect("the portraits case has photos");
    let width = photo.dimensions.0;

    let outside = CropRect {
        x: width,
        y: 0,
        width: 10,
        height: 10,
    };
    assert!(
        newsbuilder_core::photo::set_crop(photo, Some(outside)).is_err(),
        "a crop outside the photo must be refused rather than recorded"
    );
    assert!(item.crops_are_within_bounds());
}

#[test]
fn inv_7_the_quality_bounds_hold_for_the_built_in_appearance() {
    let budget = ImageBudget::built_in();
    assert!(budget.jpeg_min_quality <= budget.jpeg_quality);
    assert!(budget.webp_min_quality <= budget.webp_quality);
    for quality in [
        budget.jpeg_quality,
        budget.jpeg_min_quality,
        budget.webp_quality,
        budget.webp_min_quality,
    ] {
        assert!(
            (1..=100).contains(&quality),
            "quality {quality} is out of range"
        );
    }
}

#[test]
fn inv_7_is_enforced_when_loading_a_configuration() {
    use newsbuilder_core::model::appearance_config::load;
    assert!(load(r#"{"image": {"jpeg_quality": 40, "jpeg_min_quality": 80}}"#).is_err());
    assert!(load(r#"{"image": {"webp_quality": 40, "webp_min_quality": 80}}"#).is_err());
}

#[test]
fn inv_8_a_credential_reference_always_names_its_server() {
    // Publishing is refused before any photo is processed when a credential does not resolve;
    // the reference has to name the server for that message to be actionable (SC-007).
    let key = CredentialRef::Key {
        path: "/home/editor/.ssh/id_ed25519".into(),
        server: "cms".into(),
    };
    let password = CredentialRef::Password {
        server: "cms".into(),
    };
    assert_eq!(key.server(), "cms");
    assert_eq!(password.server(), "cms");
    assert_ne!(
        key.store_entry(),
        password.store_entry(),
        "the two credential kinds must not share a secret-store entry"
    );
}

#[test]
fn an_item_with_no_body_still_satisfies_every_invariant() {
    let item = NewsItem::new();
    assert!(item.placements_resolve());
    assert!(item.photo_ids_are_unique());
    assert!(item.file_names_are_unique());
    assert!(item.layouts_match_counts());
    assert!(item.crops_are_within_bounds());
    assert!(Slug::is_well_formed(item.slug.as_str()));
}
