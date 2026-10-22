//! Adversarial synthetic RGBA fixtures. These exercise the production vision
//! and Engine, not ScreenCaptureKit, a browser, or physical click delivery.
use macro_core::*;
use macro_core::vision::{extract_candidates_view_with_stats, CandidateExtraction, RgbaFrame, RgbaView};
use serde_json::json;

fn image() -> RgbaFrame {
    RgbaFrame { width: 64, height: 48, rgba: [245, 245, 245, 255].repeat(64 * 48) }
}

fn patch(frame: &mut RgbaFrame, x: usize, y: usize, width: usize, height: usize, alpha: u8) {
    let color = Profile::default().color;
    for row in y..y + height {
        for column in x..x + width {
            let at = (row * frame.width as usize + column) * 4;
            frame.rgba[at..at + 4].copy_from_slice(&[color.r, color.g, color.b, alpha]);
        }
    }
}

fn full_config() -> VisionConfig {
    VisionConfig { roi: Some(Rect { x: 0.0, y: 0.0, width: 64.0, height: 48.0 }), ..VisionConfig::default() }
}

fn extract(frame: &RgbaFrame, config: &VisionConfig, generation: u64) -> CandidateExtraction {
    extract_candidates_view_with_stats(RgbaView {
        width: frame.width, height: frame.height, stride: frame.width as usize * 4, rgba: &frame.rgba,
    }, Profile::default().color, 0, config, generation).unwrap()
}

fn first_seat(actions: &[Action]) -> Option<Point> {
    actions.iter().find_map(|action| match &action.kind {
        ActionKind::Click { target: Target::Point { point }, purpose } if purpose.starts_with("seat:") => Some(*point),
        _ => None,
    })
}

#[test]
fn p3_sampled_transient_seat_is_proposed_but_unobserved_intervals_remain_a_sampling_limit() {
    let empty = image();
    let mut visible = image();
    patch(&mut visible, 30, 20, 5, 5, 255);
    let mut rows = vec![];
    // This is a declared test sampling schedule, not a measured OS frame rate.
    for duration in [1_u64, 4, 8, 16, 40] {
        let mut detected = 0;
        let mut observable = 0;
        let mut max_proposal_delay = 0;
        for phase in 0..20 {
            let start = 20 + phase;
            let end = start + duration;
            let mut profile = Profile::default();
            profile.vision = full_config();
            profile.target_seats = 1;
            profile.refresh_interval_ms = 1000;
            profile.legacy.cancellation_click_delay_ms = 0;
            let mut engine = Engine::new(profile).unwrap();
            engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
            let mut proposal = None;
            let mut seen = false;
            for at in [0, 20, 40, 60, 80] {
                let is_visible = at >= start && at < end;
                let frame = if is_visible { &visible } else { &empty };
                let generation = engine.snapshot().generation;
                let extraction = extract(frame, &full_config(), generation);
                assert_eq!(extraction.candidates.len(), usize::from(is_visible));
                seen |= is_visible;
                let actions = engine.step(Event::CandidatesObserved { generation, candidates: extraction.candidates }, at);
                if first_seat(&actions).is_some() && proposal.is_none() { proposal = Some(at); }
            }
            assert_eq!(proposal.is_some(), seen, "duration={duration}, phase={phase}");
            if seen { observable += 1; }
            if let Some(at) = proposal {
                detected += 1;
                assert!(at >= start && at < end);
                max_proposal_delay = max_proposal_delay.max(at - start);
            }
        }
        assert_eq!(detected, duration.min(20));
        rows.push(json!({"visible_duration_ms":duration,"phase_cases":20,"sample_interval_ms":20,
            "observable_cases":observable,"proposed_click_cases":detected,"missed_sampling_cases":20-detected,
            "max_proposal_delay_ms":max_proposal_delay}));
    }
    println!("{}", json!({"scenario":"P3 transient appearance", "evidence":"synthetic RGBA + real vision/Engine; no OS input", "rows":rows}));
}

#[test]
fn p5_same_colored_legend_is_currently_a_candidate_and_roi_or_exclusion_can_remove_it() {
    let mut frame = image();
    patch(&mut frame, 2, 2, 5, 5, 255); // Ground truth: legend, not a seat.
    patch(&mut frame, 30, 20, 5, 5, 255); // Ground truth: seat.
    let broad = extract(&frame, &full_config(), 0);
    assert_eq!(broad.candidates.len(), 2);
    assert_eq!(broad.candidates[0].point, Point { x: 4.0, y: 4.0 });
    let mut profile = Profile::default();
    profile.vision = full_config();
    profile.features.seat_priority = true;
    let mut engine = Engine::new(profile).unwrap();
    engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
    let actions = engine.step(Event::CandidatesObserved { generation: 0, candidates: broad.candidates }, 1);
    assert_eq!(first_seat(&actions), Some(Point { x: 4.0, y: 4.0 }), "Current algorithm cannot semantically reject the legend");
    let cropped = VisionConfig { roi: Some(Rect { x: 20.0, y: 10.0, width: 40.0, height: 30.0 }), ..full_config() };
    let excluded = VisionConfig { exclusions: vec![Rect { x: 0.0, y: 0.0, width: 12.0, height: 12.0 }], ..full_config() };
    for config in [cropped, excluded] {
        let result = extract(&frame, &config, 0);
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].point, Point { x: 32.0, y: 22.0 });
    }
    println!("{}", json!({"scenario":"P5 legend", "broad_roi_candidates":2,"broad_roi_false_positives":1,
        "first_proposed_click_is_legend":true,"cropped_roi_false_positives":0,"excluded_legend_false_positives":0}));
}

#[test]
fn p5_touching_seats_merge_and_subthreshold_seats_are_rejected_by_current_geometry_rules() {
    let mut separated = image();
    patch(&mut separated, 20, 20, 5, 5, 255); patch(&mut separated, 26, 20, 5, 5, 255);
    assert_eq!(extract(&separated, &full_config(), 0).candidates.len(), 2);
    let mut touching = image();
    patch(&mut touching, 20, 20, 5, 5, 255); patch(&mut touching, 25, 20, 5, 5, 255);
    assert_eq!(extract(&touching, &full_config(), 0).candidates.len(), 1);
    let mut rows = vec![];
    for pixels in [1, 2, 3] {
        let mut small = image(); patch(&mut small, 20, 20, pixels, 1, 255);
        let result = extract(&small, &full_config(), 0);
        assert_eq!(result.candidates.len(), usize::from(pixels >= 3));
        rows.push(json!({"matching_pixels":pixels,"accepted":result.stats.accepted_count,"rejected_too_small":result.stats.rejected_too_small}));
    }
    println!("{}", json!({"scenario":"P5 component geometry", "truth_seats":2,"one_pixel_gap_candidates":2,"edge_touching_candidates":1,"small_seats":rows}));
}

#[test]
fn p5_partial_opacity_is_currently_accepted_by_extraction_and_is_not_semantic_visibility() {
    let mut rows = vec![];
    for alpha in [0_u8, 1, 64, 249, 250, 255] {
        let mut frame = image(); patch(&mut frame, 20, 20, 5, 5, alpha);
        let result = extract(&frame, &full_config(), 0);
        assert_eq!(result.candidates.len(), usize::from(alpha != 0));
        rows.push(json!({"alpha":alpha,"vision_candidates":result.candidates.len()}));
    }
    // The platform's click-time color_at guard uses alpha >= 250. This test
    // deliberately asserts extraction behavior only, never delivered clicks.
    println!("{}", json!({"scenario":"P5 opacity", "scope":"extraction only; no native click claim", "rows":rows}));
}

#[test]
fn p5_roi_clipping_can_turn_an_oversized_nonseat_into_a_small_accepted_component() {
    let mut frame = image(); patch(&mut frame, 10, 10, 20, 20, 255);
    let broad = VisionConfig { max_component_pixels: 25, ..full_config() };
    let crop = VisionConfig { roi: Some(Rect { x: 10.0, y: 10.0, width: 5.0, height: 5.0 }), ..broad.clone() };
    let full = extract(&frame, &broad, 0);
    let clipped = extract(&frame, &crop, 0);
    assert_eq!(full.stats.rejected_too_large, 1);
    assert!(full.candidates.is_empty());
    assert_eq!(clipped.stats.matching_pixels, 25);
    assert_eq!(clipped.candidates.len(), 1);
    println!("{}", json!({"scenario":"P5 clipped nonseat", "full_component_pixels":400,
        "max_component_pixels":25,"full_image_candidates":0,"clipped_roi_candidates":1,"false_positive_after_crop":1}));
}
