use macro_core::*;
use macro_core::clock::*;
use macro_core::vision::*;

fn c(id: &str, x: f64, y: f64, color: Color) -> Candidate {
    Candidate { id: id.into(), point: Point { x, y }, color, target: Target::Point { point: Point { x, y } }, selected: false, geometry: None }
}
fn put(frame: &mut RgbaFrame, x: usize, y: usize, color: Color) {
    let i = (y * frame.width as usize + x) * 4;
    frame.rgba[i..i + 4].copy_from_slice(&[color.r, color.g, color.b, 255]);
}
#[test]
fn central_region_always_beats_a_closer_side_region() {
    let color = Profile::default().color;
    let candidates = vec![
        RegionCandidate { candidate: c("side", 0.0, 0.0, color), center_rank: 1, distance_to_stage: 1.0 },
        RegionCandidate { candidate: c("center-back", 0.0, 0.0, color), center_rank: 0, distance_to_stage: 100.0 },
        RegionCandidate { candidate: c("center-front", 0.0, 0.0, color), center_rank: 0, distance_to_stage: 50.0 },
        RegionCandidate { candidate: c("wrong-color", 0.0, 0.0, Color { r: 0, g: 0, b: 0 }), center_rank: 0, distance_to_stage: 0.0 },
    ];
    let ranked = rank_regions(candidates, color, 0);
    assert_eq!(ranked.iter().map(|r| r.candidate.id.as_str()).collect::<Vec<_>>(), ["center-front", "center-back", "side"]);
}

#[test]
fn top_first_seat_priority_is_distinct_from_stage_region_priority() {
    let color = Profile::default().color;
    let mut seats = vec![c("bottom", 0.0, 50.0, color), c("top-right", 50.0, 1.0, color), c("top-left", 2.0, 1.0, color)];
    rank_seats(&mut seats);
    assert_eq!(seats.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["top-left", "top-right", "bottom"]);
}

#[test]
fn excludes_legend_and_outside_roi_before_extracting_components() {
    let color = Profile::default().color;
    let mut frame = RgbaFrame { width: 12, height: 10, rgba: vec![0; 12 * 10 * 4] };
    for (x, y) in [(1, 1), (2, 1), (1, 2), (7, 5), (8, 5), (7, 6), (10, 8)] { put(&mut frame, x, y, color); }
    let config = VisionConfig {
        roi: Some(Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 }),
        exclusions: vec![Rect { x: 0.0, y: 0.0, width: 4.0, height: 4.0 }],
        min_component_pixels: 3, max_component_pixels: 20,
    };
    let candidates = extract_candidates(&frame, color, 0, &config, 4).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, "pixel:7:5");
    let refreshed = extract_candidates(&frame, color, 0, &config, 5).unwrap();
    assert_eq!(candidates[0].id, refreshed[0].id);
    let cropped = VisionConfig { roi: Some(Rect { x: 6.0, y: 4.0, width: 4.0, height: 4.0 }), ..config.clone() };
    let cropped_candidates = extract_candidates(&frame, color, 0, &cropped, 5).unwrap();
    assert_eq!(cropped_candidates[0].id, candidates[0].id);
    assert_eq!(cropped_candidates[0].point, candidates[0].point);
}

#[test]
fn concave_component_click_is_on_color_not_empty_centroid() {
    let color = Profile::default().color;
    let mut frame = RgbaFrame { width: 5, height: 5, rgba: vec![0; 5 * 5 * 4] };
    for x in 1..4 { for y in 1..4 { if x != 2 || y != 2 { put(&mut frame, x, y, color); } } }
    let candidates = extract_candidates(&frame, color, 0, &VisionConfig::default(), 0).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_ne!(candidates[0].point, Point { x: 2.0, y: 2.0 });
    let i = (candidates[0].point.y as usize * 5 + candidates[0].point.x as usize) * 4;
    assert_eq!(frame.rgba[i], color.r);
}

#[test]
fn oversized_component_is_fully_consumed_without_hiding_a_separate_seat() {
    let color = Profile::default().color;
    let mut frame = RgbaFrame { width: 140, height: 100, rgba: vec![0; 140 * 100 * 4] };
    for y in 0..100 { for x in 0..100 { put(&mut frame, x, y, color); } }
    for y in 10..16 { for x in 120..126 { put(&mut frame, x, y, color); } }
    let config = VisionConfig { max_component_pixels: 100, ..VisionConfig::default() };
    let extraction = extract_candidates_view_with_stats(RgbaView {
        width: frame.width, height: frame.height, stride: frame.width as usize * 4, rgba: &frame.rgba,
    }, color, 0, &config, 0).unwrap();
    assert_eq!(extraction.stats.matching_pixels, 10_036);
    assert_eq!(extraction.stats.components_found, 2);
    assert_eq!(extraction.stats.rejected_too_large, 1);
    assert_eq!(extraction.candidates.len(), 1);
    assert_eq!(extraction.candidates[0].id, "pixel:120:10");
}

#[test]
fn extraction_diagnostics_recognize_separate_small_round_seat_components() {
    let color = Color { r: 185, g: 224, b: 180 };
    let mut frame = RgbaFrame { width: 16, height: 10, rgba: vec![0; 16 * 10 * 4] };
    for center_x in [3, 11] {
        for (dx, dy) in [(0, -1), (-1, 0), (0, 0), (1, 0), (0, 1)] {
            put(&mut frame, (center_x + dx) as usize, (4 + dy) as usize, color);
        }
    }
    let view = RgbaView { width: frame.width, height: frame.height, stride: frame.width as usize * 4, rgba: &frame.rgba };
    let extraction = extract_candidates_view_with_stats(view, color, 12, &VisionConfig::default(), 0).unwrap();
    assert_eq!(extraction.stats, ExtractionStats {
        matching_pixels: 10, components_found: 2, accepted_count: 2,
        rejected_too_small: 0, rejected_too_large: 0, rejected_unsupported: 0,
    });
    assert_eq!(extraction.candidates.iter().map(|c| c.point).collect::<Vec<_>>(),
        vec![Point { x: 3.0, y: 4.0 }, Point { x: 11.0, y: 4.0 }]);
    let original = extract_candidates_view(view, color, 12, &VisionConfig::default(), 0).unwrap();
    assert_eq!(serde_json::to_value(&extraction.candidates).unwrap(), serde_json::to_value(original).unwrap());
}

#[test]
fn extraction_diagnostics_distinguish_color_mismatch_from_component_size_rejection() {
    let color = Color { r: 185, g: 224, b: 180 };
    let mut frame = RgbaFrame { width: 12, height: 8, rgba: vec![0; 12 * 8 * 4] };
    put(&mut frame, 1, 1, color);
    for y in 3..6 { for x in 6..9 { put(&mut frame, x, y, color); } }
    let config = VisionConfig { min_component_pixels: 3, max_component_pixels: 8, ..VisionConfig::default() };
    let view = RgbaView { width: frame.width, height: frame.height, stride: frame.width as usize * 4, rgba: &frame.rgba };
    let no_color = extract_candidates_view_with_stats(view, Color { r: 29, g: 168, b: 21 }, 12, &config, 0).unwrap();
    assert!(no_color.candidates.is_empty());
    assert_eq!(no_color.stats, ExtractionStats::default());
    let filtered = extract_candidates_view_with_stats(view, color, 12, &config, 0).unwrap();
    assert!(filtered.candidates.is_empty());
    assert_eq!(filtered.stats, ExtractionStats {
        matching_pixels: 10, components_found: 2, accepted_count: 0,
        rejected_too_small: 1, rejected_too_large: 1, rejected_unsupported: 0,
    });
}

#[test]
fn extraction_diagnostics_count_only_visible_matching_pixels_in_effective_roi() {
    let color = Profile::default().color;
    let mut frame = RgbaFrame { width: 10, height: 8, rgba: vec![0; 10 * 8 * 4] };
    for (x, y) in [(3, 3), (4, 3), (3, 4), (0, 0), (1, 0), (6, 3), (7, 3), (6, 4), (4, 4)] {
        put(&mut frame, x, y, color);
    }
    frame.rgba[(4 * 10 + 4) * 4 + 3] = 0;
    let config = VisionConfig {
        roi: Some(Rect { x: 2.0, y: 2.0, width: 6.0, height: 4.0 }),
        exclusions: vec![Rect { x: 6.0, y: 3.0, width: 2.0, height: 2.0 }],
        ..VisionConfig::default()
    };
    let view = RgbaView { width: frame.width, height: frame.height, stride: frame.width as usize * 4, rgba: &frame.rgba };
    let extraction = extract_candidates_view_with_stats(view, color, 0, &config, 0).unwrap();
    assert_eq!(extraction.stats, ExtractionStats {
        matching_pixels: 3, components_found: 1, accepted_count: 1,
        rejected_too_small: 0, rejected_too_large: 0, rejected_unsupported: 0,
    });
    let outside = VisionConfig { roi: Some(Rect { x: 20.0, y: 20.0, width: 4.0, height: 4.0 }), ..config };
    let empty = extract_candidates_view_with_stats(view, color, 0, &outside, 0).unwrap();
    assert!(empty.candidates.is_empty());
    assert_eq!(empty.stats, ExtractionStats::default());
}

#[test]
fn borrowed_padded_native_capture_matches_packed_capture_without_copying() {
    let color = Profile::default().color;
    let rgba = [color.r, color.g, color.b, 255, 0, 0, 0, 0,
        color.r, color.g, color.b, 255, 0, 0, 0, 0,
        color.r, color.g, color.b, 255, 0, 0, 0, 0];
    let candidates = extract_candidates_view(RgbaView { width: 1, height: 3, stride: 8, rgba: &rgba }, color, 0, &VisionConfig::default(), 0).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].point, Point { x: 0.0, y: 1.0 });
    let extraction = extract_candidates_view_with_stats(RgbaView { width: 1, height: 3, stride: 8, rgba: &rgba }, color, 0, &VisionConfig::default(), 0).unwrap();
    assert_eq!(extraction.stats.matching_pixels, 3);
    assert_eq!(extraction.stats.components_found, 1);
    assert_eq!(extraction.stats.accepted_count, 1);
}

#[test]
fn capture_pixels_map_to_bound_content_with_os_scale() {
    let content = Rect { x: 100.0, y: 200.0, width: 800.0, height: 600.0 };
    let point = frame_to_content(Point { x: 400.0, y: 200.0 }, 1600, 1200, content).unwrap();
    assert_eq!(point, Point { x: 300.0, y: 300.0 });
    assert!(frame_to_content(Point { x: 1600.0, y: 1.0 }, 1600, 1200, content).is_err());
}

fn sample(rtt: u64, server: i64) -> HttpClockSample {
    HttpClockSample { sent_wall_ms: 100_000, sent_monotonic_ms: 1000, received_monotonic_ms: 1000 + rtt, server_date_ms: server, precision_ms: 1000 }
}

#[test]
fn http_date_second_precision_is_explicit_not_millisecond_accuracy() {
    let estimate = estimate_clock(&[sample(20, 100_000)]).unwrap();
    assert_eq!(estimate.offset_ms, 490);
    assert_eq!(estimate.uncertainty_ms, 510);
    assert_eq!(estimate.best_round_trip_ms, 20);
    assert!(estimate.consistent);
}

#[test]
fn three_fastest_samples_exclude_one_slow_outlier() {
    let estimate = estimate_clock(&[sample(20, 100_000), sample(30, 100_000), sample(40, 100_000), sample(2000, 150_000)]).unwrap();
    assert_eq!(estimate.sample_count, 3);
    assert!(estimate.consistent);
    assert_eq!(estimate.offset_ms, 490);
}

#[test]
fn inconsistent_server_samples_expand_uncertainty() {
    let estimate = estimate_clock(&[sample(10, 100_000), sample(20, 110_000)]).unwrap();
    assert!(!estimate.consistent);
    assert!(estimate.uncertainty_ms > 5000);
}

#[test]
fn monotonic_sale_deadline_is_armed_once_without_wall_clock_polling() {
    let estimate = ClockEstimate { offset_ms: 200, uncertainty_ms: 500, best_round_trip_ms: 20, sample_count: 3, consistent: true };
    assert_eq!(monotonic_deadline(110_000, 100_000, 1000, &estimate).unwrap(), 10_800);
    assert_eq!(monotonic_deadline(99_000, 100_000, 1000, &estimate).unwrap(), 1000);
}

#[test]
fn invalid_clock_samples_and_corrupt_pixel_buffers_are_rejected() {
    let mut invalid = sample(20, 100_000);
    invalid.received_monotonic_ms = 999;
    assert!(estimate_clock(&[invalid]).is_err());
    assert!(extract_candidates(&RgbaFrame { width: 5, height: 5, rgba: vec![0; 3] }, Color::default(), 0, &VisionConfig::default(), 0).is_err());
}
