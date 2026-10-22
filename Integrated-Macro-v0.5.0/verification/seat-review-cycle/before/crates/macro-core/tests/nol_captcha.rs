use macro_core::{Engine, Event, Phase, Profile, RunMode};
use macro_core::nol_captcha::{NolCaptchaDetector, NolCaptchaStatus, NolCaptchaGateReason};
use macro_core::vision::RgbaView;

struct Image { width: u32, height: u32, pixels: Vec<u8> }
impl Image {
    fn plain(width: u32, height: u32, gray: u8) -> Self {
        let mut image = Self { width, height, pixels: vec![255; width as usize * height as usize * 4] };
        image.rect(0, 0, width, height, [gray; 3]);
        image
    }
    fn view(&self) -> RgbaView<'_> {
        RgbaView { width: self.width, height: self.height, stride: self.width as usize * 4, rgba: &self.pixels }
    }
    fn rect(&mut self, x: u32, y: u32, width: u32, height: u32, color: [u8; 3]) {
        for row in y..(y + height).min(self.height) {
            for column in x..(x + width).min(self.width) {
                let offset = (row as usize * self.width as usize + column as usize) * 4;
                self.pixels[offset..offset + 3].copy_from_slice(&color);
            }
        }
    }
    fn patch_points(&mut self, count: usize, color: [u8; 3]) {
        let (x, y, width, height) = (self.width * 3 / 100, self.height * 88 / 100,
            (self.width * 4 / 100).max(1), (self.height * 4 / 100).max(1));
        for index in 0..count.min(25) {
            self.rect(x + (index as u32 % 5 * 2 + 1) * width / 10,
                y + (index as u32 / 5 * 2 + 1) * height / 10, 1, 1, color);
        }
    }
    fn modal(width: u32, height: u32, x: u32, y: u32, panel_width: u32, markings: bool) -> Self {
        let mut image = Self::plain(width, height, 118);
        image.rect(x, y, panel_width, panel_width * 108 / 100, [255; 3]);
        if markings { image.markings(x, y, panel_width); }
        image
    }
    fn markings(&mut self, x: u32, y: u32, panel_width: u32) {
        // Arbitrary shapes only: no title template, CAPTCHA or input contents.
        self.rect(x + panel_width / 10, y + panel_width / 10, panel_width / 3, 12, [30; 3]);
        self.rect(x + panel_width / 10, y + panel_width / 3, panel_width / 2, 20, [80, 30, 170]);
    }
}

fn visible() -> Image { Image::modal(1280, 1000, 380, 180, 520, true) }
fn closed() -> Image { Image::plain(1280, 1000, 255) }
fn observe(detector: &mut NolCaptchaDetector, image: &Image, sequence: u64, at_ms: u64) -> macro_core::nol_captcha::NolCaptchaObservation {
    detector.observe(image.view(), sequence, sequence * 1_000_000, sequence * 1_000_000, at_ms).unwrap()
}
fn arm(detector: &mut NolCaptchaDetector, image: &Image, sequence: u64, at_ms: u64) -> macro_core::nol_captcha::NolCaptchaObservation {
    detector.arm_on_input(image.view(), sequence, sequence * 1_000_000, sequence * 1_000_000, at_ms).unwrap()
}

#[test]
fn first_input_arms_without_text_or_a_panel_at_multiple_frame_sizes() {
    for (width, height, color) in [
        (1280, 1000, [118; 3]),
        (1800, 1400, [5; 3]),
        (2200, 1600, [35, 160, 210]),
        (1000, 800, [255; 3]),
    ] {
        let mut image = Image::plain(width, height, 0);
        image.rect(0, 0, width, height, color);
        let mut detector = NolCaptchaDetector::default();
        let result = arm(&mut detector, &image, 1, 0);
        assert_eq!(result.status, NolCaptchaStatus::Visible, "color {color:?}");
        assert!(!result.dismissed);
        assert!(detector.has_reference());
        assert_eq!(result.backdrop.baseline_samples, 25);
        assert_eq!(result.backdrop.baseline_mean_rgb, Some(color));
        assert_eq!(result.panel, None);
        assert_eq!(result.title_score_basis_points, None);
        assert!(!observe(&mut detector, &image, 1, 60_000).dismissed);
    }
}

#[test]
fn loading_and_modal_color_changes_before_input_never_arm_or_complete() {
    let mut detector = NolCaptchaDetector::default();
    for (sequence, image) in [(1, closed()), (2, visible()), (3, Image::plain(1280, 1000, 0)), (4, closed())] {
        let result = observe(&mut detector, &image, sequence, sequence * 200);
        assert_eq!(result.status, NolCaptchaStatus::Searching);
        assert_eq!(result.backdrop.gate_reason, NolCaptchaGateReason::WaitingForInput);
        assert_eq!(result.backdrop.baseline_samples, 0);
        assert!(!result.dismissed);
        assert!(!detector.has_reference());
    }
}

#[test]
fn first_input_saves_the_current_frame_even_after_unarmed_observation_in_the_same_poll() {
    let mut detector = NolCaptchaDetector::default();
    observe(&mut detector, &closed(), 1, 0);
    let current = Image::plain(1280, 1000, 137);
    assert_eq!(observe(&mut detector, &current, 2, 40).status, NolCaptchaStatus::Searching);
    let first_input = arm(&mut detector, &current, 2, 40);
    assert_eq!(first_input.status, NolCaptchaStatus::Visible);
    assert_eq!(first_input.backdrop.baseline_mean_rgb, Some([137; 3]));
    assert_eq!(first_input.backdrop.changed_samples, 0);
    assert!(!first_input.dismissed);
    assert!(!observe(&mut detector, &current, 2, 60_000).dismissed);
}

#[test]
fn repeated_or_wrong_answer_input_never_replaces_the_reference_or_completes() {
    let mut detector = NolCaptchaDetector::default();
    arm(&mut detector, &visible(), 1, 0);
    for (sequence, color) in [(2, [137; 3]), (3, [5, 70, 240]), (4, [255; 3])] {
        let mut image = visible();
        image.patch_points(25, color);
        image.rect(420, 320, 430, 260, color);
        let result = arm(&mut detector, &image, sequence, sequence * 200);
        assert_eq!(result.status, NolCaptchaStatus::Visible);
        assert!(!result.dismissed);
        assert_eq!(result.backdrop.baseline_mean_rgb, Some([118; 3]));
    }
    assert_eq!(observe(&mut detector, &closed(), 5, 900).status, NolCaptchaStatus::Closing);
    let repeated = arm(&mut detector, &closed(), 5, 950);
    assert_eq!(repeated.status, NolCaptchaStatus::Visible);
    assert!(!repeated.dismissed);
    assert!(!observe(&mut detector, &closed(), 5, 1000).dismissed);
    assert!(observe(&mut detector, &closed(), 5, 1100).dismissed);
}

#[test]
fn stale_or_invalid_input_frames_cannot_create_a_reference() {
    let mut detector = NolCaptchaDetector::default();
    observe(&mut detector, &visible(), 10, 100);
    let stale = arm(&mut detector, &closed(), 9, 140);
    assert_eq!(stale.status, NolCaptchaStatus::Searching);
    assert_eq!(stale.backdrop.gate_reason, NolCaptchaGateReason::StaleObservation);
    assert!(!detector.has_reference());
    assert!(detector.arm_on_input(RgbaView { width: 100, height: 100, stride: 400, rgba: &[] },
        11, 11_000_000, 11_000_000, 180).is_err());
    assert!(!detector.has_reference());
    let retry = arm(&mut detector, &visible(), 12, 220);
    assert_eq!(retry.status, NolCaptchaStatus::Visible);
    assert!(!retry.dismissed);
    assert!(detector.has_reference());
}

#[test]
fn initially_closed_screen_never_reports_a_dismissal() {
    let mut detector = NolCaptchaDetector::default();
    for time in [0, 40, 140, 1000] {
        let result = observe(&mut detector, &closed(), 1, time);
        assert_eq!(result.status, NolCaptchaStatus::Searching);
        assert!(!result.dismissed);
    }
}

#[test]
fn typing_wrong_answer_challenge_change_and_button_color_are_ignored() {
    let mut detector = NolCaptchaDetector::default();
    let mut image = visible();
    arm(&mut detector, &image, 1, 0);
    for (sequence, color) in [(2, [0, 180, 70]), (3, [220, 0, 0]), (4, [80, 30, 240])] {
        // Only the central input area changes. These are synthetic blocks,
        // not an actual challenge, response or accepted-answer fixture.
        image.rect(420, 320, 430, 260, color);
        image.rect(420, 640, 430, 60, color);
        let result = observe(&mut detector, &image, sequence, sequence * 40);
        assert_eq!(result.status, NolCaptchaStatus::Visible);
        assert!(!result.dismissed);
    }
}

#[test]
fn modal_moving_or_growing_after_an_error_is_still_visible() {
    let mut detector = NolCaptchaDetector::default();
    arm(&mut detector, &visible(), 1, 0);
    let mut error_layout = Image::plain(1280, 1000, 118);
    error_layout.rect(300, 100, 520, 650, [255; 3]);
    error_layout.markings(300, 100, 520);
    error_layout.rect(340, 540, 430, 30, [210, 30, 40]);
    let result = observe(&mut detector, &error_layout, 2, 40);
    assert_eq!(result.status, NolCaptchaStatus::Visible);
    assert!(!result.dismissed);
    assert!(!observe(&mut detector, &error_layout, 2, 240).dismissed);
}

#[test]
fn dismissal_requires_new_pixels_and_a_successful_stable_native_poll_interval() {
    let mut detector = NolCaptchaDetector::default();
    arm(&mut detector, &visible(), 1, 0);
    assert_eq!(observe(&mut detector, &closed(), 2, 40).status, NolCaptchaStatus::Closing);
    assert!(!observe(&mut detector, &closed(), 2, 139).dismissed);
    // Native validity polling succeeded, but SCK delivered just one idle
    // callback: pixel sequence AND all capture metadata remain unchanged.
    assert!(observe(&mut detector, &closed(), 2, 140).dismissed);
    assert_eq!(detector.status(), NolCaptchaStatus::Dismissed);
    assert!(!observe(&mut detector, &closed(), 2, 240).dismissed);
}

#[test]
fn central_markings_disappearing_or_small_color_noise_cannot_complete() {
    let mut detector = NolCaptchaDetector::default();
    arm(&mut detector, &visible(), 1, 0);
    let replacement = Image::modal(1280, 1000, 380, 180, 520, false);
    for time in [40, 240] { assert!(!observe(&mut detector, &replacement, 2, time).dismissed); }
    let noise = Image::plain(1280, 1000, 162); // Every channel differs by only 44.
    for time in [280, 480] { assert!(!observe(&mut detector, &noise, 3, time).dismissed); }
    assert_ne!(detector.status(), NolCaptchaStatus::Closing);
}

#[test]
fn patch_policy_checks_only_background_color_after_input_arming() {
    let mut detector = NolCaptchaDetector::default();
    arm(&mut detector, &visible(), 1, 0);
    let mut bright = closed();
    bright.markings(380, 180, 520);
    assert_eq!(observe(&mut detector, &bright, 2, 40).status, NolCaptchaStatus::Closing);
    assert!(observe(&mut detector, &bright, 2, 140).dismissed);
    // Central markings do not enter either the input-arming or close decision.
}

#[test]
fn stale_sequences_timestamps_or_repeated_poll_time_cannot_complete() {
    let mut detector = NolCaptchaDetector::default();
    arm(&mut detector, &visible(), 10, 0);
    assert!(!observe(&mut detector, &closed(), 9, 100).dismissed);
    assert_eq!(detector.status(), NolCaptchaStatus::Visible);
    assert!(!detector.observe(closed().view(), 11, 10_000_000, 10_000_000, 120).unwrap().dismissed);
    assert_eq!(detector.status(), NolCaptchaStatus::Visible);
    assert_eq!(observe(&mut detector, &closed(), 11, 140).status, NolCaptchaStatus::Closing);
    assert!(!observe(&mut detector, &closed(), 11, 140).dismissed);
    assert!(!observe(&mut detector, &closed(), 9, 300).dismissed);
    assert!(!detector.observe(closed().view(), 11, 11_000_000, 10_999_999, 320).is_ok());
    assert_eq!(detector.status(), NolCaptchaStatus::Searching);
}

#[test]
fn returning_panel_cancels_pending_close_and_restarts_stability() {
    let mut detector = NolCaptchaDetector::default();
    arm(&mut detector, &visible(), 1, 0);
    observe(&mut detector, &closed(), 2, 40);
    assert_eq!(observe(&mut detector, &visible(), 3, 120).status, NolCaptchaStatus::Visible);
    assert!(!observe(&mut detector, &closed(), 4, 160).dismissed);
    assert!(observe(&mut detector, &closed(), 4, 260).dismissed);
}

#[test]
fn stale_capture_during_closing_breaks_the_stability_interval() {
    let mut detector = NolCaptchaDetector::default();
    arm(&mut detector, &visible(), 1, 0);
    observe(&mut detector, &closed(), 2, 40);
    assert!(!observe(&mut detector, &closed(), 1, 90).dismissed);
    assert_eq!(detector.status(), NolCaptchaStatus::Visible);
    assert!(!observe(&mut detector, &closed(), 2, 140).dismissed);
    assert!(observe(&mut detector, &closed(), 2, 240).dismissed);
}

#[test]
fn interruption_retracts_pending_or_undelivered_dismissal_but_keeps_presence() {
    for interrupt_after_confirmation in [false, true] {
        let mut detector = NolCaptchaDetector::default();
        arm(&mut detector, &visible(), 1, 0);
        observe(&mut detector, &closed(), 2, 40);
        if interrupt_after_confirmation { assert!(observe(&mut detector, &closed(), 2, 140).dismissed); }
        detector.interrupt();
        assert_eq!(detector.status(), NolCaptchaStatus::Visible);
        assert!(!observe(&mut detector, &closed(), 2, 1000).dismissed);
        assert!(!observe(&mut detector, &closed(), 2, 1099).dismissed);
        assert!(observe(&mut detector, &closed(), 2, 1100).dismissed);
    }
}

#[test]
fn resize_invalid_capture_and_reset_discard_old_presence() {
    for change in 0..3 {
        let mut detector = NolCaptchaDetector::default();
        arm(&mut detector, &visible(), 1, 0);
        match change {
            0 => { observe(&mut detector, &Image::plain(1281, 1000, 255), 2, 40); },
            1 => {
                assert!(detector.observe(RgbaView { width: 100, height: 100, stride: 400, rgba: &[] }, 2, 2, 2, 40).is_err());
            },
            _ => detector.reset(),
        }
        assert_eq!(detector.status(), NolCaptchaStatus::Searching);
        assert!(!observe(&mut detector, &closed(), 3, 240).dismissed);
    }
}

#[test]
fn visual_dismissal_drives_the_engine_once_then_empty_seat_search_continues() {
    let mut profile = Profile::default();
    profile.features.nol_captcha_detection = true;
    let mut engine = Engine::new(profile).unwrap();
    engine.step(Event::Start { mode: RunMode::Normal }, 0);
    assert!(engine.awaits_nol_captcha());
    let mut detector = NolCaptchaDetector::default();
    let mut typed = visible();
    for (sequence, time) in [(1, 10), (2, 40)] {
        if sequence == 2 { typed.rect(420, 540, 430, 40, [60; 3]); }
        let result = if sequence == 1 { arm(&mut detector, &typed, sequence, time) }
            else { observe(&mut detector, &typed, sequence, time) };
        assert!(!result.dismissed);
        engine.step(Event::Tick, time);
        assert!(engine.awaits_nol_captcha());
    }
    for time in [80, 180, 220] {
        if observe(&mut detector, &closed(), 3, time).dismissed {
            engine.step(Event::NolCaptchaDismissed, time);
        }
    }
    assert_eq!(engine.snapshot().phase, Phase::Seats);
    for time in [240, 400, 1000] {
        engine.step(Event::CandidatesObserved { generation: engine.snapshot().generation, candidates: vec![] }, time);
        engine.step(Event::Tick, time);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
    }
}

#[test]
fn input_changes_and_an_enlarged_clipped_panel_do_not_replace_patch_reference() {
    let mut detector = NolCaptchaDetector::default();
    let first = arm(&mut detector, &visible(), 1, 0);
    assert_eq!(first.backdrop.baseline_mean_rgb, Some([118; 3]));
    let enlarged = Image::modal(1280, 1000, 80, 30, 1120, true);
    for at in [40, 200] {
        let result = observe(&mut detector, &enlarged, 2, at);
        assert_eq!(result.status, NolCaptchaStatus::Visible);
        assert!(!result.dismissed);
        assert_eq!(result.backdrop.baseline_mean_rgb, Some([118; 3]));
        assert!(result.backdrop.changed_samples < 20);
    }
}

#[test]
fn input_arming_does_not_require_or_classify_the_patch_as_a_modal_background() {
    let mut detector = NolCaptchaDetector::default();
    let overlaps = Image::modal(1280, 1500, 20, 840, 520, true);
    let result = arm(&mut detector, &overlaps, 1, 0);
    assert_eq!(result.status, NolCaptchaStatus::Visible);
    assert_eq!(result.backdrop.gate_reason, NolCaptchaGateReason::BackgroundUnchanged);
    assert_eq!(result.backdrop.baseline_samples, 25);
    assert_eq!(result.backdrop.baseline_mean_rgb, Some([255; 3]));
    assert!(!result.dismissed);
    // Without panel recognition the fixed patch may land on a white panel or
    // page decoration. Correct placement remains an operational assumption.
}

#[test]
fn nineteen_changed_points_wait_and_twenty_changed_points_start_stability() {
    let mut detector = NolCaptchaDetector::default();
    arm(&mut detector, &visible(), 1, 0);
    let mut partial = visible();
    partial.patch_points(19, [163, 118, 118]); // Exactly 45 in one channel.
    let result = observe(&mut detector, &partial, 2, 40);
    assert_eq!(result.backdrop.changed_samples, 19);
    assert_eq!(result.backdrop.gate_reason, NolCaptchaGateReason::BackgroundPartial);
    assert!(!observe(&mut detector, &partial, 2, 240).dismissed);
    partial.patch_points(20, [163, 118, 118]);
    let result = observe(&mut detector, &partial, 3, 280);
    assert_eq!(result.backdrop.changed_samples, 20);
    assert_eq!(result.status, NolCaptchaStatus::Closing);
    assert!(!observe(&mut detector, &partial, 3, 379).dismissed);
    assert!(observe(&mut detector, &partial, 3, 380).dismissed);
}

#[test]
fn brighter_darker_and_equal_luminance_color_changes_use_the_same_close_and_recheck_rule() {
    // The last color has the same integer luminance as gray 118 using common
    // 54/183/19 weights, while red changes by 61 and green by -18.
    let equal_luminance = [179u8, 100, 118];
    assert_eq!((179 * 54 + 100 * 183 + 118 * 19) / 256, 118);
    for color in [[163; 3], [73; 3], equal_luminance] {
        let mut detector = NolCaptchaDetector::default();
        let baseline = visible();
        arm(&mut detector, &baseline, 1, 0);
        // A static frame never starts closing, even after a long valid wait.
        let still = observe(&mut detector, &baseline, 1, 60_000);
        assert_eq!(still.status, NolCaptchaStatus::Visible);
        assert_eq!(still.backdrop.gate_reason, NolCaptchaGateReason::BackgroundUnchanged);
        let mut changed = visible();
        changed.patch_points(25, color);
        let closing = observe(&mut detector, &changed, 2, 60_040);
        assert_eq!(closing.status, NolCaptchaStatus::Closing, "color {color:?}");
        assert_eq!(closing.backdrop.changed_samples, 25);
        assert_eq!(closing.backdrop.gate_reason, NolCaptchaGateReason::BackgroundChanged);
        assert!(!observe(&mut detector, &changed, 2, 60_139).dismissed);
        assert!(observe(&mut detector, &changed, 2, 60_140).dismissed);
        let recheck = detector.recheck_dismissal(changed.view(), 2, 2_000_000, 2_000_000, 60_141).unwrap();
        assert_eq!(recheck.status, NolCaptchaStatus::Dismissed);
        assert!(!recheck.dismissed);
        let returned = detector.recheck_dismissal(baseline.view(), 3, 3_000_000, 3_000_000, 60_141).unwrap();
        assert_eq!(returned.status, NolCaptchaStatus::Visible);
    }
}

#[test]
fn colored_dark_and_bright_opaque_patch_references_are_all_recorded_once() {
    for baseline in [[35, 160, 210], [5, 10, 15], [240; 3]] {
        let mut detector = NolCaptchaDetector::default();
        let mut image = visible();
        // The reference accepts these colors without title or edge checks.
        image.patch_points(25, baseline);
        let first = arm(&mut detector, &image, 1, 0);
        assert_eq!(first.status, NolCaptchaStatus::Visible);
        assert_eq!(first.backdrop.baseline_samples, 25);
        assert_eq!(first.backdrop.baseline_mean_rgb, Some(baseline));
        assert_eq!(first.backdrop.gate_reason, NolCaptchaGateReason::BackgroundUnchanged);
        // Select a guaranteed change in the appropriate direction, without
        // constraining the patch's original hue or absolute brightness.
        let red = if baseline[0] < 45 { baseline[0] + 45 } else { baseline[0] - 45 };
        let color = [red, baseline[1], baseline[2]];
        image.patch_points(25, color);
        let changed = observe(&mut detector, &image, 2, 40);
        assert_eq!(changed.backdrop.baseline_mean_rgb, Some(baseline));
        assert_eq!(changed.backdrop.changed_samples, 25);
        assert_eq!(changed.status, NolCaptchaStatus::Closing);
        assert!(observe(&mut detector, &image, 2, 140).dismissed);
    }
}

#[test]
fn neither_small_negative_noise_nor_pixels_with_the_original_sequence_can_close() {
    let mut detector = NolCaptchaDetector::default();
    arm(&mut detector, &visible(), 1, 0);
    let mut noise = visible();
    noise.patch_points(25, [74, 117, 162]); // Differences -44, -1, +44.
    for at in [40, 240] {
        let result = observe(&mut detector, &noise, 2, at);
        assert_eq!(result.status, NolCaptchaStatus::Visible);
        assert_eq!(result.backdrop.changed_samples, 0);
    }
    let mut detector = NolCaptchaDetector::default();
    arm(&mut detector, &visible(), 1, 0);
    for at in [40, 240] {
        // A caller must supply a genuinely new capture/sequence for closure.
        assert!(!observe(&mut detector, &closed(), 1, at).dismissed);
        assert_eq!(detector.status(), NolCaptchaStatus::Visible);
    }
}

#[test]
fn stored_baseline_does_not_drift_while_typing_and_difference_must_be_at_least_45() {
    let mut image = visible();
    image.rect(38, 880, 52, 40, [190; 3]);
    let mut detector = NolCaptchaDetector::default();
    assert_eq!(arm(&mut detector, &image, 1, 0).backdrop.baseline_samples, 25);
    for (sequence, value) in [(2, 170), (3, 220), (4, 234)] {
        image.rect(38, 880, 52, 40, [value; 3]);
        let result = observe(&mut detector, &image, sequence, sequence * 40);
        assert_eq!(result.backdrop.baseline_mean_rgb, Some([190; 3]));
        assert_eq!(result.backdrop.changed_samples, 0);
    }
    image.rect(38, 880, 52, 40, [235; 3]);
    assert_eq!(observe(&mut detector, &image, 5, 240).status, NolCaptchaStatus::Closing);
    assert!(observe(&mut detector, &image, 5, 340).dismissed);
}

#[test]
fn transparent_frames_cannot_arm_or_complete_and_break_the_stability_interval() {
    let mut transparent = closed();
    for pixel in transparent.pixels.chunks_exact_mut(4) { pixel[3] = 0; }
    let mut detector = NolCaptchaDetector::default();
    let first = arm(&mut detector, &transparent, 1, 0);
    assert_eq!(first.status, NolCaptchaStatus::Searching);
    assert_eq!(first.backdrop.gate_reason, NolCaptchaGateReason::TransparentFrame);
    assert!(!detector.has_reference());
    arm(&mut detector, &visible(), 2, 40);
    observe(&mut detector, &closed(), 3, 80);
    let rejected = observe(&mut detector, &transparent, 4, 120);
    assert_eq!(rejected.status, NolCaptchaStatus::Visible);
    assert_eq!(rejected.backdrop.opaque_samples, 0);
    assert!(!observe(&mut detector, &closed(), 5, 220).dismissed);
    assert!(observe(&mut detector, &closed(), 5, 320).dismissed);
}

#[test]
fn opaque_white_and_black_replacements_remain_indistinguishable_by_this_local_patch_policy() {
    for value in [0, 255] {
        let mut detector = NolCaptchaDetector::default();
        let replacement = Image::plain(1280, 1000, value);
        arm(&mut detector, &visible(), 1, 0);
        observe(&mut detector, &replacement, 2, 40);
        assert!(observe(&mut detector, &replacement, 2, 140).dismissed);
    }
    // This is a recorded limitation, not a claim that a blank renderer frame
    // or successful authentication is identified by the detector.
}

#[test]
fn first_click_recheck_has_no_extra_delay_but_revokes_an_unchanged_or_transparent_frame() {
    for invalid_alpha in [false, true] {
        let mut detector = NolCaptchaDetector::default();
        arm(&mut detector, &visible(), 1, 0);
        observe(&mut detector, &closed(), 2, 40);
        assert!(observe(&mut detector, &closed(), 2, 140).dismissed);
        let unchanged = detector.recheck_dismissal(closed().view(), 2, 2_000_000, 2_000_000, 141).unwrap();
        assert_eq!(unchanged.status, NolCaptchaStatus::Dismissed);
        assert!(!unchanged.dismissed);
        let mut blocked = if invalid_alpha { closed() } else { visible() };
        if invalid_alpha { for pixel in blocked.pixels.chunks_exact_mut(4) { pixel[3] = 0; } }
        // Even a new capture within the same monotonic millisecond is checked.
        let result = detector.recheck_dismissal(blocked.view(), 3, 3_000_000, 3_000_000, 141).unwrap();
        assert_eq!(result.status, NolCaptchaStatus::Visible);
        assert!(!result.dismissed);
        assert!(!observe(&mut detector, &closed(), 4, 200).dismissed);
        assert!(observe(&mut detector, &closed(), 4, 300).dismissed);
    }
}
