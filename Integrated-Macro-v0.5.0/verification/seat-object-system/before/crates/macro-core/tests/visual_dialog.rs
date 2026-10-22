use macro_core::visual_dialog::*;
use macro_core::vision::RgbaView;

fn image(width: usize, height: usize, value: u8) -> Vec<u8> {
    [value, value, value, 255].repeat(width * height)
}

fn sample(pixels: &[u8], width: usize, height: usize, sequence: u64) -> VisualSample {
    sample_frame(RgbaView { width: width as u32, height: height as u32, stride: width * 4, rgba: pixels }, sequence).unwrap()
}

fn central_indices() -> Vec<usize> {
    (14..34).flat_map(|y| (19..45).map(move |x| y * 64 + x)).collect()
}

fn surrounding_indices() -> Vec<usize> {
    (0..64 * 48).filter(|index| !(19..45).contains(&(index % 64)) || !(14..34).contains(&(index / 64))).collect()
}

#[test]
fn transparent_capture_cannot_be_used_as_dialog_or_restoration_evidence() {
    // A complete native buffer can still contain transparent/unrendered pixels.
    // Their RGB bytes must not create a conflict or fake a closed dialog.
    for rgb in [0, 180, 255] {
        let rgba = [rgb, rgb, rgb, 0].repeat(64 * 48);
        assert!(sample_frame(RgbaView {
            width: 64, height: 48, stride: 64 * 4, rgba: &rgba,
        }, 2).is_err(), "transparent RGB {rgb} must not become visual evidence");
    }
}

#[test]
fn incomplete_center_is_rejected_even_when_most_of_the_whole_window_is_opaque() {
    let mut current = image(64, 48, 180);
    for index in central_indices().into_iter().take(60) {
        current[index * 4..index * 4 + 4].fill(0);
    }
    // Only 1.96% of the full image is missing, but it can resemble a popup.
    let error = sample_frame(RgbaView {
        width: 64, height: 48, stride: 64 * 4, rgba: &current,
    }, 2).unwrap_err();
    assert!(error.to_string().starts_with("[capture_incomplete]"));
}

#[test]
fn transparent_edges_are_ignored_and_comparison_needs_overlapping_opaque_samples() {
    let original = image(64, 48, 180);
    let mut edge = original.clone();
    for index in [0, 63, 47 * 64, 48 * 64 - 1] {
        edge[index * 4..index * 4 + 4].fill(0);
    }
    let report = compare_samples(&sample(&original, 64, 48, 1), &sample(&edge, 64, 48, 2),
        &VisualChangeConfig::default()).unwrap().unwrap();
    assert_eq!(report.sample_count, 64 * 48 - 4);
    assert_eq!(report.changed_samples, 0);
    assert!(!report.large_change);
    assert!(report.baseline_restored);

    let mut first = original.clone();
    let mut second = original;
    for index in central_indices().into_iter().take(6) { first[index * 4 + 3] = 0; }
    for index in central_indices().into_iter().skip(6).take(6) { second[index * 4 + 3] = 0; }
    // Each image has enough center evidence alone; their overlap does not.
    assert!(compare_samples(&sample(&first, 64, 48, 1), &sample(&second, 64, 48, 2),
        &VisualChangeConfig::default()).is_err());
}

#[test]
fn center_popup_change_alone_triggers_without_surrounding_darkening() {
    let original = image(64, 48, 180);
    let baseline = sample(&original, 64, 48, 10);
    let mut current = original.clone();
    for index in central_indices().into_iter().take(60) { current[index * 4..index * 4 + 3].fill(240); }
    let report = compare_samples(&baseline, &sample(&current, 64, 48, 11), &VisualChangeConfig::default()).unwrap().unwrap();
    assert_eq!(report.central_sample_count, 520);
    assert_eq!(report.surrounding_darkened_samples, 0);
    assert!(report.changed_basis_points < 1800);
    assert!(report.central_changed_basis_points >= 1000);
    assert!(report.large_change);
    assert_eq!(report.trigger_kind, Some(VisualTriggerKind::CentralChange));
    assert!(!report.baseline_restored);
}

#[test]
fn surrounding_darkening_alone_triggers_and_both_signals_are_optional() {
    let original = image(64, 48, 180);
    let baseline = sample(&original, 64, 48, 1);
    let mut current = original.clone();
    for index in surrounding_indices() { current[index * 4..index * 4 + 3].fill(100); }
    let dark = compare_samples(&baseline, &sample(&current, 64, 48, 2), &VisualChangeConfig::default()).unwrap().unwrap();
    assert_eq!(dark.central_changed_samples, 0);
    assert_eq!(dark.surrounding_darkened_basis_points, 10_000);
    assert_eq!(dark.trigger_kind, Some(VisualTriggerKind::SurroundingDarkening));
    assert!(dark.large_change);
    for index in central_indices() { current[index * 4..index * 4 + 3].fill(240); }
    let both = compare_samples(&baseline, &sample(&current, 64, 48, 3), &VisualChangeConfig::default()).unwrap().unwrap();
    assert_eq!(both.trigger_kind, Some(VisualTriggerKind::Both));
    assert!(both.large_change);
}

#[test]
fn minor_seat_colors_and_large_bright_outer_changes_do_not_trigger() {
    let original = image(64, 48, 180);
    let baseline = sample(&original, 64, 48, 1);
    let mut current = original.clone();
    for index in [22 * 64 + 30, 22 * 64 + 31, 23 * 64 + 30, 23 * 64 + 31] {
        current[index * 4] = 20;
    }
    let small = compare_samples(&baseline, &sample(&current, 64, 48, 2), &VisualChangeConfig::default()).unwrap().unwrap();
    assert_eq!(small.central_changed_samples, 4);
    assert!(!small.large_change);
    assert!(small.baseline_restored);
    current = original.clone();
    for index in surrounding_indices() { current[index * 4..index * 4 + 3].fill(240); }
    let bright = compare_samples(&baseline, &sample(&current, 64, 48, 3), &VisualChangeConfig::default()).unwrap().unwrap();
    assert!(bright.changed_basis_points > 8000);
    assert_eq!(bright.central_changed_basis_points, 0);
    assert_eq!(bright.surrounding_darkened_basis_points, 0);
    assert!(!bright.large_change);
    assert_eq!(bright.trigger_kind, None);
}

#[test]
fn no_longer_large_is_not_equivalent_to_baseline_restoration() {
    let original = image(64, 48, 180);
    let baseline = sample(&original, 64, 48, 1);
    let mut partially_changed = original.clone();
    for index in central_indices().into_iter().take(30) { partially_changed[index * 4] = 80; }
    let partial = compare_samples(&baseline, &sample(&partially_changed, 64, 48, 2), &VisualChangeConfig::default()).unwrap().unwrap();
    assert!(!partial.large_change);
    assert!(!partial.baseline_restored);
    partially_changed = original.clone();
    for index in surrounding_indices().into_iter().take(150) { partially_changed[index * 4..index * 4 + 3].fill(100); }
    let dim = compare_samples(&baseline, &sample(&partially_changed, 64, 48, 3), &VisualChangeConfig::default()).unwrap().unwrap();
    assert!(!dim.large_change);
    assert!(!dim.baseline_restored);
    let mut restored = original.clone();
    for index in central_indices().into_iter().take(10) { restored[index * 4] = 80; }
    for index in surrounding_indices().into_iter().take(50) { restored[index * 4..index * 4 + 3].fill(100); }
    let closed = compare_samples(&baseline, &sample(&restored, 64, 48, 4), &VisualChangeConfig::default()).unwrap().unwrap();
    assert!(closed.baseline_restored);
}

#[test]
fn thresholds_use_region_relative_fraction_and_material_delta() {
    let original = image(10, 10, 100);
    let baseline = sample(&original, 10, 10, 1);
    let config = VisualChangeConfig { central_change_basis_points: 1250, ..VisualChangeConfig::default() };
    let mut current = original.clone();
    for index in [33, 34] { current[index * 4 + 1] = 131; }
    assert_eq!(compare_samples(&baseline, &sample(&current, 10, 10, 2), &config).unwrap().unwrap().changed_samples, 0);
    for index in [33, 34] { current[index * 4 + 1] = 132; }
    assert!(compare_samples(&baseline, &sample(&current, 10, 10, 3), &config).unwrap().unwrap().large_change);
    let higher = VisualChangeConfig { central_change_basis_points: 1251, ..config };
    assert!(!compare_samples(&baseline, &sample(&current, 10, 10, 4), &higher).unwrap().unwrap().large_change);
    current = original.clone();
    for pixel in current[..30 * 4].chunks_exact_mut(4) { pixel[..3].fill(69); }
    assert!(!compare_samples(&baseline, &sample(&current, 10, 10, 5), &config).unwrap().unwrap().large_change);
    for pixel in current[..30 * 4].chunks_exact_mut(4) { pixel[..3].fill(68); }
    let dark = compare_samples(&baseline, &sample(&current, 10, 10, 6), &config).unwrap().unwrap();
    assert_eq!(dark.trigger_kind, Some(VisualTriggerKind::SurroundingDarkening));
    assert_eq!(dark.central_changed_samples, 0);
}

#[test]
fn stale_sequence_changed_geometry_invalid_config_and_corrupt_images_are_rejected() {
    let original = image(64, 48, 100);
    let baseline = sample(&original, 64, 48, 10);
    let config = VisualChangeConfig::default();
    assert!(compare_samples(&baseline, &sample(&original, 64, 48, 10), &config).unwrap().is_none());
    assert!(compare_samples(&baseline, &sample(&original, 64, 48, 9), &config).unwrap().is_none());
    assert!(compare_samples(&baseline, &sample(&image(65, 48, 100), 65, 48, 11), &config).is_err());
    let invalid = VisualChangeConfig { central_restored_basis_points: 1000, ..config };
    assert!(compare_samples(&baseline, &sample(&original, 64, 48, 11), &invalid).is_err());
    assert!(sample_frame(RgbaView { width: 64, height: 48, stride: 256, rgba: &original[..100] }, 1).is_err());
}

#[test]
fn sampling_stays_bounded_and_ignores_native_row_padding() {
    let original = image(128, 96, 100);
    let mut padded = Vec::new();
    for row in original.chunks_exact(128 * 4) {
        padded.extend_from_slice(row);
        padded.extend_from_slice(&[255; 16]);
    }
    let baseline = sample(&original, 128, 96, 1);
    let current = sample_frame(RgbaView { width: 128, height: 96, stride: 128 * 4 + 16, rgba: &padded }, 2).unwrap();
    let report = compare_samples(&baseline, &current, &VisualChangeConfig::default()).unwrap().unwrap();
    assert_eq!(report.sample_count, 64 * 48);
    assert_eq!(report.changed_samples, 0);
    assert!(report.baseline_restored);
}
