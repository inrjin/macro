//! Independent analytic scene truth, actual extractor and actual pre-click
//! validation. These tests do not execute native input or infer site ownership.
#[path = "support/seat_object_shapes.rs"]
mod fixtures;
use fixtures::*;
use macro_core::{seat_objects::{extract_seat_objects_view_with_stats, seat_object_candidate_supported}, *};
use std::collections::BTreeSet;

fn inspect(name: &str, scene: &Scene, config: &VisionConfig, expected: &[usize]) -> Result<(), String> {
    inspect_view(name, scene, scene.view(), config, expected)
}

fn inspect_view(name: &str, scene: &Scene, view: macro_core::vision::RgbaView<'_>, config: &VisionConfig, expected: &[usize]) -> Result<(), String> {
    let began = std::time::Instant::now();
    let output = extract_seat_objects_view_with_stats(view, AVAILABLE, 12, config, 7)
        .map_err(|error| format!("{name}: {error}"))?;
    let mut hits = vec![];
    let mut problems = vec![];
    for candidate in &output.candidates {
        let owners = scene.hits(candidate.point);
        if owners.len() == 1 { hits.push(owners[0]); }
        if owners.len() != 1 || !expected.contains(&owners[0]) {
            problems.push(format!("click {:?} hits {:?}, expected {:?}", candidate.point, owners, expected));
        }
        if !seat_object_candidate_supported(view, candidate, AVAILABLE, 12, config) {
            problems.push(format!("extractor proposed {} but real pre-click rejects it", candidate.id));
        }
    }
    let unique: BTreeSet<_> = hits.iter().copied().collect();
    if hits.len() != expected.len() || unique != expected.iter().copied().collect() {
        problems.push(format!("object hits={hits:?}; expected each once {expected:?}"));
    }
    println!("{}", serde_json::json!({"case":name,"expectedObjects":expected,"actualHits":hits,
        "candidateCount":output.candidates.len(),"objectStats":output.object_stats,
        "elapsedUs":began.elapsed().as_micros() as u64,
        "scope":"independent synthetic pixels -> actual extractor/pre-click; no browser/OS",
        "diagnosticObjects":if problems.is_empty() { serde_json::Value::Null } else { serde_json::to_value(&output.objects).unwrap() }}));
    if problems.is_empty() { Ok(()) } else { Err(format!("{name}: {}", problems.join("; "))) }
}

fn reference_scene(shape: Shape, angle: f64) -> Scene {
    let mut scene = repeated(shape, angle);
    for id in [8, 9] { scene.paint_object(id, WHITE); }
    scene.objects.truncate(8);
    scene
}

#[test]
fn required_nonround_and_round_families_keep_positive_coverage() {
    let mut failures = vec![];
    let families = families();
    assert_eq!(families.len(), 9, "required positives cannot be reclassified as all-unknown");
    for (name, shape, angle) in families {
        let scene = repeated(shape, angle);
        if let Err(error) = inspect(name, &scene, &VisionConfig::default(), &[8, 9]) { failures.push(error); }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn dark_text_splits_color_but_not_the_independent_full_object() {
    let mut failures = vec![];
    for (name, shape, angle) in families().into_iter().filter(|(name, _, _)| *name != "small_square7") {
        for cross in [false, true] {
            let mut scene = repeated(shape, angle);
            if cross { scene.dark_cross(8); } else { scene.dark_vertical_mark(8); }
            let label = format!("{name}:dark_{}", if cross { "cross" } else { "line" });
            if let Err(error) = inspect(&label, &scene, &VisionConfig::default(), &[8, 9]) { failures.push(error); }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn separated_neighbors_and_narrow_bridges_keep_two_distinct_hit_targets() {
    let mut failures = vec![];
    for (name, shape, angle) in families().into_iter().filter(|(_, _, angle)| *angle == 0.0) {
        for bridge in [false, true] {
            let mut scene = reference_scene(shape, angle);
            let first = scene.add(shape, Point { x: 65.5, y: 125.5 }, angle, true);
            let second = scene.add(shape, Point { x: 65.5 + shape.size().0 + 1.0, y: 125.5 }, angle, true);
            if bridge { scene.bridge(first, second, 1, AVAILABLE); }
            let label = format!("{name}:{}", if bridge { "narrow_bridge" } else { "one_pixel_gap" });
            if let Err(error) = inspect(&label, &scene, &VisionConfig::default(), &[first, second]) { failures.push(error); }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn unobserved_or_excluded_object_does_not_poison_its_observed_neighbor() {
    let mut failures = vec![];
    for (name, shape, angle) in families() {
        let scene = repeated(shape, angle);
        let mut missing = scene.clone();
        missing.alpha(65, 125, 0);
        if let Err(error) = inspect(&format!("{name}:missing_center"), &missing, &VisionConfig::default(), &[9]) {
            failures.push(error);
        }
        let excluded = VisionConfig { exclusions: vec![Rect { x: 65.0, y: 125.0, width: 1.0, height: 1.0 }],
            ..VisionConfig::default() };
        if let Err(error) = inspect(&format!("{name}:excluded_center"), &scene, &excluded, &[9]) { failures.push(error); }
        let clipped = VisionConfig { roi: Some(Rect { x: 65.0, y: 0.0, width: 255.0, height: 180.0 }),
            ..VisionConfig::default() };
        // The ROI removes two of eight references; six complete references and
        // the right-hand positive control remain. The cut left object is unknown.
        if let Err(error) = inspect(&format!("{name}:clipped_left"), &scene, &clipped, &[9]) { failures.push(error); }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn selected_fill_and_large_white_holes_do_not_gain_extra_object_clicks() {
    let mut failures = vec![];
    for (name, shape, angle) in families().into_iter().filter(|(name, _, _)| *name != "small_square7") {
        let mut selected = repeated(shape, angle); selected.selected(8);
        if let Err(error) = inspect(&format!("{name}:selected_dark"), &selected, &VisionConfig::default(), &[9]) { failures.push(error); }
        let mut hole = repeated(shape, angle);
        for y in 122..=128 { for x in 62..=68 { hole.paint(x, y, WHITE); } }
        if let Err(error) = inspect(&format!("{name}:large_white_hole"), &hole, &VisionConfig::default(), &[9]) { failures.push(error); }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_fully_filled_bridge_is_not_proof_of_two_separate_objects() {
    // The same filled rectangle can be one wide object or two adjoining sites.
    // Independent truth cannot be recovered from these pixels alone. Repeated
    // narrow references must not manufacture two independently bounded objects.
    let shape = Shape::Rectangle { width: 15.0, height: 15.0 };
    let mut scene = reference_scene(shape, 0.0);
    let first = scene.add(shape, Point { x: 65.5, y: 125.5 }, 0.0, true);
    let second = scene.add(shape, Point { x: 81.5, y: 125.5 }, 0.0, true);
    scene.bridge(first, second, 15, AVAILABLE);
    inspect("wide_bridge_ambiguous", &scene, &VisionConfig::default(), &[]).unwrap();
}

#[test]
fn antialiasing_subpixel_offsets_and_dark_background_retain_positive_controls() {
    let mut failures = vec![];
    for (name, shape, angle) in families().into_iter().filter(|(name, _, _)| *name != "small_square7") {
        for (label, bg, phase) in [("subpixel_aa", WHITE, 0.25), ("dark_aa", Color { r: 24, g: 26, b: 30 }, 0.75)] {
            let mut scene = Scene::with_background(320, 180, bg);
            for row in 0..2 { for col in 0..4 {
                scene.add_aa(shape, Point { x: 30.5 + col as f64 * 60.0, y: 25.5 + row as f64 * 36.0 }, angle, false, bg);
            }}
            scene.add_aa(shape, Point { x: 65.0 + phase, y: 125.0 + phase }, angle, true, bg);
            scene.add_aa(shape, Point { x: 125.5, y: 125.5 }, angle, true, bg);
            if let Err(error) = inspect(&format!("{name}:{label}"), &scene, &VisionConfig::default(), &[8, 9]) { failures.push(error); }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn one_pixel_size_variation_and_multiple_families_are_not_silently_discarded() {
    let mut failures = vec![];
    for (name, reference, varied) in [
        ("rectangle_plus_one", Shape::Rectangle { width: 19.0, height: 11.0 }, Shape::Rectangle { width: 20.0, height: 12.0 }),
        ("ellipse_minus_one", Shape::Ellipse { width: 21.0, height: 13.0 }, Shape::Ellipse { width: 20.0, height: 12.0 }),
        ("diamond_plus_one", Shape::Diamond { width: 19.0, height: 19.0 }, Shape::Diamond { width: 20.0, height: 20.0 }),
    ] {
        let mut scene = reference_scene(reference, 0.0);
        scene.add(varied, Point { x: 65.5, y: 125.5 }, 0.0, true);
        scene.add(reference, Point { x: 125.5, y: 125.5 }, 0.0, true);
        if let Err(error) = inspect(name, &scene, &VisionConfig::default(), &[8, 9]) { failures.push(error); }
    }
    let mut scene = Scene::new(480, 240); let mut ids = vec![];
    for (family, shape) in [Shape::Ellipse { width: 15.0, height: 15.0 },
        Shape::Rectangle { width: 19.0, height: 11.0 }, Shape::Diamond { width: 19.0, height: 19.0 }].into_iter().enumerate() {
        let origin = 25.5 + family as f64 * 160.0;
        for row in 0..2 { for col in 0..4 {
            scene.add(shape, Point { x: origin + col as f64 * 30.0, y: 25.5 + row as f64 * 35.0 }, 0.0, false);
        }}
        for x in [origin + 25.0, origin + 60.0] { ids.push(scene.add(shape, Point { x, y: 150.5 }, 0.0, true)); }
    }
    if let Err(error) = inspect("mixed_three_families", &scene, &VisionConfig::default(), &ids) { failures.push(error); }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn padded_scanlines_ignore_poisoned_padding_bytes() {
    for (name, shape, angle) in families() {
        let scene = repeated(shape, angle); let row = scene.frame.width as usize * 4;
        let stride = row + 28;
        let mut rgba = vec![0_u8; stride * scene.frame.height as usize];
        for y in 0..scene.frame.height as usize {
            rgba[y * stride..y * stride + row].copy_from_slice(&scene.frame.rgba[y * row..(y + 1) * row]);
            for pixel in rgba[y * stride + row..(y + 1) * stride].chunks_exact_mut(4) {
                pixel.copy_from_slice(&[AVAILABLE.r, AVAILABLE.g, AVAILABLE.b, 0]);
            }
        }
        let view = macro_core::vision::RgbaView { width: scene.frame.width, height: scene.frame.height, stride, rgba: &rgba };
        inspect_view(&format!("{name}:padding"), &scene, view, &VisionConfig::default(), &[8, 9]).unwrap();
    }
}

#[test]
#[ignore = "manual optimized static-image latency, not an app/OS measurement"]
fn performance_hundreds_and_thousand_seats() {
    for total in [200, 1000] {
        for (name, shape, angle) in [
            ("rounded", Shape::RoundedRectangle { width: 15.0, height: 11.0, radius: 3.0 }, 0.0),
            ("slanted", Shape::Rectangle { width: 19.0, height: 9.0 }, 30.0),
        ] {
            let mut scene = Scene::new(1280, 800); let mut expected = vec![];
            for id in 0..total {
                let available = id == total / 2 || id == total / 2 + 1 || id == total - 1;
                scene.add(shape, Point { x: 25.5 + (id % 40) as f64 * 30.0,
                    y: 25.5 + (id / 40) as f64 * 26.0 }, angle, available);
                if available { expected.push(id); }
            }
            scene.bridge(total / 2, total / 2 + 1, 1, AVAILABLE);
            // Warm-up outside measurements. Proposal truth is checked in every run.
            inspect("warmup", &scene, &VisionConfig::default(), &expected).unwrap();
            let mut samples = vec![];
            for index in 0..5 {
                let start = std::time::Instant::now();
                inspect(&format!("{name}:{total}:sample{index}"), &scene, &VisionConfig::default(), &expected).unwrap();
                samples.push(start.elapsed().as_micros() as u64);
            }
            samples.sort_unstable();
            println!("{}", serde_json::json!({"benchmark":name,"seatObjects":total,
                "frame":[1280,800],"available":3,"narrowBridge":true,"samplesUs":samples,
                "medianUs":samples[2],"scope":"optimized static synthetic extraction plus pre-click and independent oracle; no OS/browser"}));
        }
    }
}

#[test]
fn multicolor_and_block_letter_fills_keep_object_identity() {
    let mut failures = vec![];
    for (name, shape, angle) in families().into_iter().filter(|(name, _, _)| *name != "small_square7") {
        for checker in [false, true] {
            let mut scene = repeated(shape, angle);
            for (x, y) in scene.objects[8].pixels.clone() {
                let i = (y * scene.frame.width as usize + x) * 4;
                let available = scene.frame.rgba[i..i + 3] == [AVAILABLE.r, AVAILABLE.g, AVAILABLE.b];
                if available && if checker { (x / 3 + y / 3) % 2 == 0 } else { x < 65 } {
                    scene.paint(x, y, Color { r: 53, g: 85, b: 154 });
                }
            }
            let label = format!("{name}:{}", if checker { "checker_3px" } else { "multicolor_half" });
            if let Err(error) = inspect(&label, &scene, &VisionConfig::default(), &[8, 9]) { failures.push(error); }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn diagonal_disjoint_square_neighbors_do_not_need_a_forced_spacing_radius() {
    let shape = Shape::Rectangle { width: 15.0, height: 15.0 };
    let mut scene = reference_scene(shape, 0.0);
    let first = scene.add(shape, Point { x: 65.5, y: 125.5 }, 0.0, true);
    let second = scene.add(shape, Point { x: 80.5, y: 140.5 }, 0.0, true);
    assert!(scene.objects[first].pixels.iter().all(|p| !scene.objects[second].pixels.contains(p)));
    inspect("diagonal_corner_neighbors", &scene, &VisionConfig::default(), &[first, second]).unwrap();
}

#[test]
fn repeated_selected_holes_do_not_become_the_learned_available_family() {
    let mut failures = vec![];
    for (name, shape, angle) in families().into_iter().filter(|(name, _, _)| *name != "small_square7") {
        let mut scene = Scene::new(420, 240);
        for col in 0..6 { scene.add(shape, Point { x: 30.5 + col as f64 * 55.0, y: 30.5 }, angle, false); }
        for col in 0..6 {
            let center = Point { x: 30.5 + col as f64 * 55.0, y: 90.5 };
            let id = scene.add(shape, center, angle, true);
            scene.objects[id].available = false;
            for y in 87..=93 { for x in center.x as usize - 3..=center.x as usize + 3 { scene.paint(x, y, WHITE); }}
        }
        let first = scene.add(shape, Point { x: 65.5, y: 170.5 }, angle, true);
        let second = scene.add(shape, Point { x: 125.5, y: 170.5 }, angle, true);
        if let Err(error) = inspect(&format!("{name}:six_repeated_selected_holes"), &scene,
            &VisionConfig::default(), &[first, second]) { failures.push(error); }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn an_incomplete_reference_family_alone_is_not_authority_to_click_white_marked_objects() {
    // This is an ambiguity rejection, not positive support coverage: pixels can
    // equally be lettering on available seats. Without a complete reference,
    // holding the whole family is permitted; the previous test requires that
    // adding complete references preserves the separate normal positives.
    for shape in [Shape::Ellipse { width: 15.0, height: 15.0 },
        Shape::Rectangle { width: 15.0, height: 15.0 }, Shape::Diamond { width: 19.0, height: 19.0 }] {
        let mut scene = Scene::new(420, 240);
        for col in 0..6 {
            let center = Point { x: 30.5 + col as f64 * 55.0, y: 90.5 };
            let id = scene.add(shape, center, 0.0, true); scene.objects[id].available = false;
            for y in 87..=93 { for x in center.x as usize - 3..=center.x as usize + 3 { scene.paint(x, y, WHITE); }}
        }
        scene.add(shape, Point { x: 65.5, y: 170.5 }, 0.0, true);
        scene.add(shape, Point { x: 125.5, y: 170.5 }, 0.0, true);
        let output = extract_seat_objects_view_with_stats(scene.view(), AVAILABLE, 12, &VisionConfig::default(), 0).unwrap();
        let hits = output.candidates.iter().map(|candidate| scene.hits(candidate.point)).collect::<Vec<_>>();
        println!("{}", serde_json::json!({"case":"only_white_marked_references", "shape":format!("{shape:?}"),
            "actualHits":hits,"required":"no selected-mark hits; normal support not claimed without complete references"}));
        assert!(hits.iter().all(|ids| ids.len() == 1 && ids[0] >= 6), "learned marks became click targets: {hits:?}");
    }
}
