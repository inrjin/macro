use macro_core::{Color, Point, Rect, VisionConfig};
use macro_core::vision::{extract_candidates_view_with_stats, extract_seat_candidates_view_with_stats,
    seat_point_supported, CandidateExtraction, RgbaFrame, RgbaView};

const COLOR: Color = Color { r: 185, g: 224, b: 180 };

fn image(width: u32, height: u32) -> RgbaFrame {
    RgbaFrame { width, height, rgba: [255, 255, 255, 255].repeat(width as usize * height as usize) }
}
fn view(image: &RgbaFrame) -> RgbaView<'_> {
    RgbaView { width: image.width, height: image.height, stride: image.width as usize * 4, rgba: &image.rgba }
}
fn put(image: &mut RgbaFrame, x: usize, y: usize, alpha: u8) {
    let offset = (y * image.width as usize + x) * 4;
    image.rgba[offset..offset + 4].copy_from_slice(&[COLOR.r, COLOR.g, COLOR.b, alpha]);
}
fn block(image: &mut RgbaFrame, x: usize, y: usize, width: usize, height: usize) {
    for y in y..y + height { for x in x..x + width { put(image, x, y, 255); } }
}
fn seats(image: &RgbaFrame, config: &VisionConfig) -> CandidateExtraction {
    extract_seat_candidates_view_with_stats(view(image), COLOR, 12, config, 0).unwrap()
}

#[test]
fn logged_three_pixel_shape_is_rejected_only_by_the_seat_extractor() {
    let mut image = image(130, 925);
    block(&mut image, 122, 919, 1, 3);
    let config = VisionConfig::default();
    let generic = extract_candidates_view_with_stats(view(&image), COLOR, 12, &config, 0).unwrap();
    assert_eq!(generic.candidates.len(), 1);
    assert_eq!(generic.candidates[0].id, "pixel:122:919");
    assert_eq!(generic.candidates[0].point, Point { x: 122.0, y: 920.0 });
    let geometry = generic.candidates[0].geometry.as_ref().unwrap();
    assert_eq!(geometry.bounds, Rect { x: 122.0, y: 919.0, width: 1.0, height: 3.0 });
    assert_eq!(geometry.pixels, 3);
    assert_eq!(generic.stats.rejected_unsupported, 0);
    let seat = seats(&image, &config);
    assert!(seat.candidates.is_empty());
    assert_eq!(seat.stats.matching_pixels, 3);
    assert_eq!(seat.stats.rejected_unsupported, 1);
    assert!(!seat_point_supported(view(&image), generic.candidates[0].point, COLOR, 12, &config));
}

#[test]
fn solid_small_seats_and_close_neighbors_remain_separate() {
    let mut image = image(12, 8);
    block(&mut image, 1, 1, 2, 2);
    block(&mut image, 4, 1, 2, 2); // one-pixel gap
    block(&mut image, 8, 4, 3, 3);
    let config = VisionConfig::default();
    let result = seats(&image, &config);
    assert_eq!(result.candidates.len(), 3);
    assert_eq!(result.stats.rejected_unsupported, 0);
    assert_eq!(result.candidates.iter().map(|c| c.geometry.as_ref().unwrap().pixels).collect::<Vec<_>>(), vec![4, 4, 9]);
    assert!(result.candidates.iter().all(|c| seat_point_supported(view(&image), c.point, COLOR, 12, &config)));
    assert_eq!(result.candidates[0].geometry.as_ref().unwrap().bounds, Rect { x: 1.0, y: 1.0, width: 2.0, height: 2.0 });
    assert_eq!(result.candidates[1].geometry.as_ref().unwrap().bounds, Rect { x: 4.0, y: 1.0, width: 2.0, height: 2.0 });
}

#[test]
fn click_uses_a_supported_pixel_but_geometry_keeps_the_whole_component() {
    let mut image = image(16, 5);
    block(&mut image, 1, 1, 2, 2);
    block(&mut image, 3, 1, 10, 1); // thin tail pulls the centroid out of the 2x2 body
    let config = VisionConfig::default();
    let generic = extract_candidates_view_with_stats(view(&image), COLOR, 0, &config, 0).unwrap();
    assert!(!seat_point_supported(view(&image), generic.candidates[0].point, COLOR, 0, &config));
    let result = seats(&image, &config);
    assert_eq!(result.candidates.len(), 1);
    let candidate = &result.candidates[0];
    assert!(seat_point_supported(view(&image), candidate.point, COLOR, 0, &config));
    assert_eq!(candidate.point, Point { x: 2.0, y: 1.0 });
    assert_eq!(candidate.id, generic.candidates[0].id);
    assert_eq!(candidate.geometry, generic.candidates[0].geometry);
    assert_eq!(candidate.geometry.as_ref().unwrap().bounds, Rect { x: 1.0, y: 1.0, width: 12.0, height: 2.0 });
    assert_eq!(candidate.geometry.as_ref().unwrap().pixels, 14);
}

#[test]
fn pre_click_requires_the_patch_even_if_the_target_pixel_still_matches() {
    let mut image = image(4, 4);
    block(&mut image, 1, 1, 2, 2);
    let point = Point { x: 1.0, y: 1.0 };
    let config = VisionConfig::default();
    assert!(seat_point_supported(view(&image), point, COLOR, 12, &config));
    for alpha in [0, 249] {
        put(&mut image, 2, 2, alpha);
        assert!(!seat_point_supported(view(&image), point, COLOR, 12, &config));
    }
    put(&mut image, 2, 2, 250);
    assert!(seat_point_supported(view(&image), point, COLOR, 12, &config));
    let restricted = VisionConfig { roi: Some(Rect { x: 1.0, y: 1.0, width: 1.0, height: 2.0 }), ..config.clone() };
    assert!(!seat_point_supported(view(&image), point, COLOR, 12, &restricted));
    let excluded = VisionConfig { exclusions: vec![Rect { x: 2.0, y: 2.0, width: 1.0, height: 1.0 }], ..config };
    assert!(!seat_point_supported(view(&image), point, COLOR, 12, &excluded));
}

#[test]
fn opacity_changes_seat_eligibility_without_changing_generic_alpha_policy() {
    let mut image = image(5, 5);
    for y in 1..3 { for x in 1..3 { put(&mut image, x, y, 249); } }
    let config = VisionConfig::default();
    assert_eq!(extract_candidates_view_with_stats(view(&image), COLOR, 0, &config, 0).unwrap().candidates.len(), 1);
    assert_eq!(seats(&image, &config).stats.matching_pixels, 0);
    for (x, y) in [(1, 1), (2, 1), (1, 2)] { put(&mut image, x, y, 255); }
    let result = seats(&image, &config);
    assert_eq!(result.stats.matching_pixels, 3);
    assert_eq!(result.stats.rejected_unsupported, 1);
    put(&mut image, 2, 2, 250);
    assert_eq!(seats(&image, &config).candidates.len(), 1);
}

#[test]
fn thin_outline_and_three_pixel_l_shape_are_explicit_seat_filter_limitations() {
    let mut image = image(15, 12);
    for (x, y) in [(1, 1), (2, 1), (1, 2)] { put(&mut image, x, y, 255); }
    for x in 5..13 { put(&mut image, x, 3, 255); put(&mut image, x, 10, 255); }
    for y in 4..10 { put(&mut image, 5, y, 255); put(&mut image, 12, y, 255); }
    let config = VisionConfig::default();
    assert_eq!(extract_candidates_view_with_stats(view(&image), COLOR, 0, &config, 0).unwrap().candidates.len(), 2);
    let result = seats(&image, &config);
    assert!(result.candidates.is_empty());
    assert_eq!(result.stats.rejected_unsupported, 2);
}

#[test]
fn support_validation_handles_edges_stride_and_invalid_inputs() {
    let rgba = [COLOR.r, COLOR.g, COLOR.b, 255, COLOR.r, COLOR.g, COLOR.b, 255, 0, 0, 0, 0,
                COLOR.r, COLOR.g, COLOR.b, 255, COLOR.r, COLOR.g, COLOR.b, 255, 0, 0, 0, 0];
    let padded = RgbaView { width: 2, height: 2, stride: 12, rgba: &rgba };
    let config = VisionConfig::default();
    for point in [Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 }] {
        assert!(seat_point_supported(padded, point, COLOR, 0, &config));
    }
    for point in [Point { x: -1.0, y: 0.0 }, Point { x: 2.0, y: 1.0 },
        Point { x: 0.5, y: 0.0 }, Point { x: f64::NAN, y: 0.0 }] {
        assert!(!seat_point_supported(padded, point, COLOR, 0, &config));
    }
    assert!(!seat_point_supported(RgbaView { rgba: &rgba[..20], ..padded }, Point { x: 0.0, y: 0.0 }, COLOR, 0, &config));
    assert!(!seat_point_supported(RgbaView { stride: 4, ..padded }, Point { x: 0.0, y: 0.0 }, COLOR, 0, &config));
    assert!(!seat_point_supported(padded, Point { x: 0.0, y: 0.0 }, COLOR, 0,
        &VisionConfig { min_component_pixels: 0, ..config }));
}

#[test]
fn clipped_large_shape_can_still_pass_so_patch_support_does_not_prove_seat_identity() {
    let mut image = image(8, 8);
    block(&mut image, 1, 1, 6, 6);
    let config = VisionConfig { max_component_pixels: 5, ..VisionConfig::default() };
    assert_eq!(seats(&image, &config).stats.rejected_too_large, 1);
    let clipped = VisionConfig { roi: Some(Rect { x: 2.0, y: 2.0, width: 2.0, height: 2.0 }), ..config };
    let result = seats(&image, &clipped);
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].geometry.as_ref().unwrap().pixels, 4);
    assert_eq!(result.candidates[0].geometry.as_ref().unwrap().bounds, clipped.roi.unwrap());
}
