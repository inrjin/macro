//! Independent CoreText-rendered inputs. No production template bitmap is used
//! to generate these fixtures. Correct rejection and recognition coverage are
//! separate assertions; returning Unreadable for everything cannot pass.
use count_scope_experiment::{selection_count::{read_selection_count, CountRead}, vision::RgbaView};
use serde::Deserialize;
use std::{fs, path::PathBuf, time::Instant};

#[derive(Deserialize)]
struct Manifest { fixtures: Vec<Fixture> }

#[derive(Deserialize)]
struct Fixture {
    name: String,
    file: String,
    width: u32,
    height: u32,
    stride: usize,
    expectation: String,
    count: Option<usize>,
    group: String,
}

fn directory() -> PathBuf {
    PathBuf::from("/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/crates/macro-core")
        .join("../../verification/seat-object-system/reader-review")
}

fn fixtures() -> Vec<Fixture> {
    serde_json::from_slice::<Manifest>(&fs::read(directory().join("manifest.json")).unwrap())
        .unwrap().fixtures
}

fn inspect(fixture: &Fixture) -> CountRead {
    let rgba = fs::read(directory().join(&fixture.file)).unwrap();
    assert_eq!(rgba.len(), fixture.stride * fixture.height as usize, "{}", fixture.name);
    let started = Instant::now();
    let result = read_selection_count(RgbaView {
        width: fixture.width, height: fixture.height, stride: fixture.stride, rgba: &rgba,
    });
    let (count, heading, evidence) = match &result {
        CountRead::Confirmed { count, heading, evidence, .. } => {
            assert!(evidence.valid() && evidence.x >= 0.0 && evidence.y >= 0.0
                && evidence.x + evidence.width <= fixture.width as f64
                && evidence.y + evidence.height <= fixture.height as f64,
                "{} returned an invalid evidence rectangle: {evidence:?}", fixture.name);
            assert!(evidence.x <= heading.x && evidence.y <= heading.y
                && evidence.x + evidence.width >= heading.x + heading.width
                && evidence.y + evidence.height >= heading.y + heading.height,
                "{} evidence omits the label that establishes the count's meaning", fixture.name);
            (Some(*count), Some(*heading), Some(*evidence))
        }
        CountRead::Unreadable => (None, None, None),
    };
    println!("{}", serde_json::json!({
        "fixture": fixture.name, "group": fixture.group,
        "expectation": fixture.expectation, "expectedCount": fixture.count,
        "observedCount": count, "heading": heading, "evidence": evidence,
        "elapsedUs": started.elapsed().as_micros() as u64,
        "timingScope": "synthetic debug reader call; no capture, browser or OS input",
    }));
    result
}

#[test]
fn independent_positive_counts_positions_scales_and_empty_are_read() {
    let mut failures = vec![];
    let mut tested = 0;
    for fixture in fixtures().iter().filter(|case| case.expectation == "confirmed") {
        tested += 1;
        match inspect(fixture) {
            CountRead::Confirmed { count, .. } if Some(count) == fixture.count => {}
            result => failures.push(format!("{}: expected {:?}, got {result:?}", fixture.name, fixture.count)),
        }
    }
    assert!(tested >= 12, "must exercise recognition, not just conservative rejection");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn missing_ambiguous_clipped_or_unrelated_text_never_confirms_a_count() {
    let mut failures = vec![];
    for fixture in fixtures().iter().filter(|case| case.expectation == "unreadable") {
        if let result @ CountRead::Confirmed { .. } = inspect(fixture) {
            failures.push(format!("{}: expected Unreadable, got {result:?}", fixture.name));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn alternative_fonts_and_unsupported_variants_may_reject_but_never_change_the_count() {
    let mut failures = vec![];
    let (mut recognized, mut unreadable) = (0, 0);
    for fixture in fixtures().iter().filter(|case| case.expectation == "correct_or_unreadable") {
        match inspect(fixture) {
            CountRead::Confirmed { count, .. } if Some(count) == fixture.count => recognized += 1,
            CountRead::Unreadable => unreadable += 1,
            result => failures.push(format!("{}: expected {:?} or Unreadable, got {result:?}", fixture.name, fixture.count)),
        }
    }
    println!("{}", serde_json::json!({"alternativeFontAndVariantCoverage": {
        "correct": recognized, "unreadable": unreadable, "wrong": failures.len(),
        "note": "Unreadable is a support limitation, not successful recognition"
    }}));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn malformed_buffers_never_become_zero_or_a_confirmed_count() {
    for view in [
        RgbaView { width: 0, height: 0, stride: 0, rgba: &[] },
        RgbaView { width: 1, height: 1, stride: 3, rgba: &[0; 3] },
        RgbaView { width: 2, height: 1, stride: 8, rgba: &[0; 4] },
        RgbaView { width: u32::MAX, height: u32::MAX, stride: usize::MAX, rgba: &[] },
    ] {
        assert_eq!(read_selection_count(view), CountRead::Unreadable);
    }
}

#[test]
fn missing_pixels_outside_the_numeric_crop_do_not_prove_global_label_uniqueness() {
    let fixture = fixtures().into_iter().find(|case| case.name == "digit_2").unwrap();
    let mut rgba = fs::read(directory().join(&fixture.file)).unwrap();
    let read = |bytes: &[u8], stride| read_selection_count(RgbaView {
        width: fixture.width, height: fixture.height, stride, rgba: bytes,
    });
    let CountRead::Confirmed { count: 2, evidence, .. } = read(&rgba, fixture.stride) else {
        panic!("the fully observed control must actually recognize count2");
    };
    let (x, y) = (fixture.width as usize - 1, fixture.height as usize - 1);
    assert!(x as f64 >= evidence.x + evidence.width || y as f64 >= evidence.y + evidence.height,
        "the missing pixel must lie outside the locally returned evidence");
    let alpha = y * fixture.stride + x * 4 + 3;
    for missing_alpha in [0, 249] {
        rgba[alpha] = missing_alpha;
        assert_eq!(read(&rgba, fixture.stride), CountRead::Unreadable,
            "a missing global-search pixel cannot be treated as a known blank background");
    }
    rgba[alpha] = 250;
    assert!(matches!(read(&rgba, fixture.stride), CountRead::Confirmed { count: 2, .. }));

    // Storage padding is not a part of the captured viewport. Zeros here must
    // not be mistaken for unobserved visible pixels.
    let padded_stride = fixture.stride + 16;
    let mut padded = vec![0; padded_stride * fixture.height as usize];
    for row in 0..fixture.height as usize {
        padded[row * padded_stride..row * padded_stride + fixture.width as usize * 4]
            .copy_from_slice(&rgba[row * fixture.stride..row * fixture.stride + fixture.width as usize * 4]);
    }
    assert!(matches!(read(&padded, padded_stride), CountRead::Confirmed { count: 2, .. }));
}
