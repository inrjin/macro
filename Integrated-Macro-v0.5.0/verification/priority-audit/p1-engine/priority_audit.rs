//! Synthetic RGBA -> production vision -> production Engine boundary probes.
//! Passing reproduction tests describe adapter obligations, not a fixed runtime
//! or verified browser/OS behavior. No seat-result truth is injected here.
use macro_core::*;
use macro_core::vision::{extract_candidates, RgbaFrame};

fn point(x: f64, y: f64) -> Target { Target::Point { point: Point { x, y } } }
fn pixel_frame(seat_x: usize) -> RgbaFrame {
    let mut frame = RgbaFrame { width: 64, height: 48, rgba: vec![255; 64 * 48 * 4] };
    let color = Profile::default().color;
    for y in 20..23 {
        for x in seat_x..seat_x + 3 {
            let offset = (y * 64 + x) * 4;
            frame.rgba[offset..offset + 4].copy_from_slice(&[color.r, color.g, color.b, 255]);
        }
    }
    frame
}
fn seat_input(actions: &[Action]) -> Option<Point> {
    actions.iter().find_map(|action| match &action.kind {
        ActionKind::Click { target: Target::Point { point }, purpose } if purpose.starts_with("seat:") => Some(*point),
        _ => None,
    })
}
fn after_navigation_delivery() -> Engine {
    let mut profile = Profile::default();
    profile.legacy.cancellation_navigation = CancellationNavigation::TwoRegions {
        first: point(4.0, 4.0), second: point(58.0, 4.0),
    };
    profile.legacy.region_delay_ms = 50;
    profile.vision = VisionConfig {
        roi: Some(Rect { x: 0.0, y: 8.0, width: 64.0, height: 32.0 }),
        min_component_pixels: 3, max_component_pixels: 9, ..VisionConfig::default()
    };
    let mut engine = Engine::new(profile).unwrap();
    let actions = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    let navigation = actions.iter().find(|action| matches!(&action.kind,
        ActionKind::Click { purpose, .. } if purpose == "legacy_cancellation_region")).unwrap();
    engine.step(Event::ActionAcknowledged { id: navigation.id, success: true, detail: String::new() }, 10);
    engine.step(Event::Tick, 60);
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    assert_eq!(engine.snapshot().generation, 1);
    engine
}
fn scan_pixels(engine: &mut Engine, frame: &RgbaFrame) -> Vec<Action> {
    let state = engine.snapshot();
    let mut candidates = extract_candidates(frame, engine.effective_color(),
        engine.profile().color_tolerance, engine.effective_vision(), state.generation).unwrap();
    // Match the runtime's region namespace. Generation labels alone do not
    // establish which page generated the borrowed pixels.
    if let Some(region) = state.active_region {
        for candidate in &mut candidates { candidate.id = format!("zone{region}:{}", candidate.id); }
    }
    engine.step(Event::CandidatesObserved { generation: state.generation, candidates }, 61)
}

#[test]
fn p1_reproduces_old_pixels_becoming_a_current_generation_seat_candidate() {
    let old_zone = pixel_frame(10);
    let intended_new_zone = pixel_frame(40);
    let mut stale_engine = after_navigation_delivery();
    let old_input = seat_input(&scan_pixels(&mut stale_engine, &old_zone)).unwrap();
    let mut fresh_engine = after_navigation_delivery();
    let correct_input = seat_input(&scan_pixels(&mut fresh_engine, &intended_new_zone)).unwrap();
    assert_eq!(old_input, Point { x: 11.0, y: 21.0 });
    assert_eq!(correct_input, Point { x: 41.0, y: 21.0 });
    assert_ne!(old_input, correct_input);
    println!("P1 old-frame reproduction: ACK+delay+generation accepts old pixels at {old_input:?}; new-zone pixel control selects {correct_input:?}");
}

#[test]
fn p1_new_frame_metadata_does_not_establish_that_the_zone_finished_loading() {
    let old_zone = pixel_frame(10);
    let mut loading_zone = old_zone.clone();
    // An unrelated loading indicator produces a new image, but the old map
    // remains visible. This change is outside the configured seat ROI.
    loading_zone.rgba[..4].copy_from_slice(&[12, 34, 56, 255]);
    assert_ne!(loading_zone.rgba, old_zone.rgba);
    let mut engine = after_navigation_delivery();
    assert_eq!(seat_input(&scan_pixels(&mut engine, &loading_zone)), Some(Point { x: 11.0, y: 21.0 }));
    println!("P1 new-image counterexample: fresh sequence/timestamp may belong to a loading frame that still contains old-zone candidates");
}

#[test]
fn p1_identical_static_maps_cannot_reveal_zone_completion_from_pixels_alone() {
    // Oracle contexts differ, but every observation available to a pixel-only
    // adapter can be identical: delayed old map versus completed identical map.
    let still_loading_old_zone = pixel_frame(10);
    let completed_identical_new_zone = pixel_frame(10);
    let old_metadata = (7_u64, 9_000_000_u64, 80_000_000_u64);
    let new_metadata = old_metadata; // same image sequence/capture; fresh idle observation
    assert_eq!(still_loading_old_zone.rgba, completed_identical_new_zone.rgba);
    assert_eq!(old_metadata, new_metadata);
    let engine = after_navigation_delivery();
    let old_actions = scan_pixels(&mut engine.clone(), &still_loading_old_zone);
    let new_actions = scan_pixels(&mut engine.clone(), &completed_identical_new_zone);
    assert_eq!(serde_json::to_value(&old_actions).unwrap(), serde_json::to_value(&new_actions).unwrap());
    assert_eq!(seat_input(&old_actions), Some(Point { x: 11.0, y: 21.0 }));
    println!("P1 indistinguishable contexts: accepting identical evidence permits old-map input; requiring a changed image can indefinitely block a completed static map");
}
