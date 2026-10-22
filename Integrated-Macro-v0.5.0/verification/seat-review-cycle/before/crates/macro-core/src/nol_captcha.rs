//! The first validated input activity arms one background color reference.
//! Ordinary observations never arm it. Closing uses only the lower-left patch;
//! it reads no heading, challenge, entered text or submission button. A different
//! scene or opaque white/black replacement can resemble a background transition.

use crate::{model::ValidationError, vision::RgbaView};

const STABLE_CLOSE_MS: u64 = 100;
const PATCH_SAMPLES: usize = 25;
const REQUIRED_PATCH_SAMPLES: usize = 20;
const PATCH_CHANNEL_DELTA: u8 = 45;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NolCaptchaStatus { Searching, Visible, Closing, Dismissed }

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct NolCaptchaRect { pub x: u32, pub y: u32, pub width: u32, pub height: u32 }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NolCaptchaGateReason {
    #[default]
    WaitingForInput,
    // Retained for consumers of older diagnostics; no longer emitted here.
    SearchingForPanel,
    BaselineNotReady,
    PatchOverlapsPanel,
    BackgroundUnchanged,
    BackgroundPartial,
    BackgroundChanged,
    TransparentFrame,
    StaleObservation,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct NolCaptchaBackdropEvidence {
    pub patch: Option<NolCaptchaRect>,
    pub sample_count: usize,
    pub baseline_samples: usize,
    pub changed_samples: usize,
    pub opaque_samples: usize,
    pub frame_opaque_basis_points: u16,
    pub mean_rgb: Option<[u8; 3]>,
    pub baseline_mean_rgb: Option<[u8; 3]>,
    pub gate_reason: NolCaptchaGateReason,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct NolCaptchaObservation {
    pub status: NolCaptchaStatus,
    /// True once after the reference background changes, until interruption.
    pub dismissed: bool,
    /// Compatibility fields: no panel or heading is recognized or required.
    pub panel: Option<NolCaptchaRect>,
    pub title_score_basis_points: Option<u16>,
    pub backdrop: NolCaptchaBackdropEvidence,
}

#[derive(Clone, Debug)]
struct BackdropPoint { x: u32, y: u32, reference: [u8; 3] }

#[derive(Clone, Debug)]
struct Presence {
    baseline: Vec<BackdropPoint>,
    sequence: u64,
    captured_at_ns: u64,
}

#[derive(Clone, Debug)]
pub struct NolCaptchaDetector {
    status: NolCaptchaStatus,
    presence: Option<Presence>,
    close_since_ms: Option<u64>,
    dimensions: Option<(u32, u32)>,
    last_sequence: u64,
    last_capture_ns: u64,
    last_observed_ns: u64,
    last_checked_ms: Option<u64>,
    backdrop: NolCaptchaBackdropEvidence,
}

impl Default for NolCaptchaDetector {
    fn default() -> Self {
        Self {
            status: NolCaptchaStatus::Searching, presence: None, close_since_ms: None,
            dimensions: None, last_sequence: 0, last_capture_ns: 0, last_observed_ns: 0,
            last_checked_ms: None, backdrop: NolCaptchaBackdropEvidence::default(),
        }
    }
}

impl NolCaptchaDetector {
    pub fn reset(&mut self) { *self = Self::default(); }
    pub fn status(&self) -> NolCaptchaStatus { self.status }
    pub fn has_reference(&self) -> bool { self.presence.is_some() }

    /// A pause/occlusion retracts pending or undelivered dismissal, but preserves
    /// the original color reference. Typing never replaces that reference.
    pub fn interrupt(&mut self) {
        self.close_since_ms = None;
        self.status = if self.has_reference() { NolCaptchaStatus::Visible }
            else { NolCaptchaStatus::Searching };
    }

    /// Called only for input activity validated by the runtime's run token,
    /// foreground and event/frame-time checks. This call never confirms closure.
    /// Repeated calls preserve the reference and retract any pending dismissal.
    pub fn arm_on_input(
        &mut self, frame: RgbaView<'_>, sequence: u64,
        captured_at_ns: u64, last_observed_ns: u64, checked_at_ms: u64,
    ) -> Result<NolCaptchaObservation, ValidationError> {
        if self.has_reference() {
            self.interrupt();
            return Ok(self.observation(false));
        }
        // Input may arrive in the same native poll as an unarmed observation.
        if !self.prepare_observation(frame, sequence, captured_at_ns, last_observed_ns, checked_at_ms, true)?
            || !self.inspect_backdrop(frame) {
            return Ok(self.observation(false));
        }
        let baseline = color_reference(frame, backdrop_patch(frame));
        self.backdrop.baseline_samples = baseline.len();
        self.backdrop.baseline_mean_rgb = Some(mean_rgb(baseline.iter().map(|point| point.reference)));
        self.presence = Some(Presence { baseline, sequence, captured_at_ns });
        self.close_since_ms = None;
        self.status = NolCaptchaStatus::Visible;
        self.backdrop.gate_reason = NolCaptchaGateReason::BackgroundUnchanged;
        Ok(self.observation(false))
    }

    /// checked_at_ms may advance only after a successful native window/stream
    /// validity check. A static SCK image can then finish its 100 ms interval.
    /// Before arm_on_input succeeds, every valid image remains WaitingForInput.
    pub fn observe(
        &mut self, frame: RgbaView<'_>, sequence: u64,
        captured_at_ns: u64, last_observed_ns: u64, checked_at_ms: u64,
    ) -> Result<NolCaptchaObservation, ValidationError> {
        self.observe_inner(frame, sequence, captured_at_ns, last_observed_ns, checked_at_ms, false)
    }

    /// Revalidate immediately before the first post-CAPTCHA click. A still-changed
    /// background retains Dismissed without another 100 ms delay. An unchanged
    /// or unusable image revokes it; the caller must keep waiting, not click.
    pub fn recheck_dismissal(
        &mut self, frame: RgbaView<'_>, sequence: u64,
        captured_at_ns: u64, last_observed_ns: u64, checked_at_ms: u64,
    ) -> Result<NolCaptchaObservation, ValidationError> {
        self.observe_inner(frame, sequence, captured_at_ns, last_observed_ns, checked_at_ms, true)
    }

    fn prepare_observation(
        &mut self, frame: RgbaView<'_>, sequence: u64,
        captured_at_ns: u64, last_observed_ns: u64, checked_at_ms: u64, allow_same_check: bool,
    ) -> Result<bool, ValidationError> {
        if !valid_frame(frame) || captured_at_ns == 0 || captured_at_ns > last_observed_ns {
            self.reset();
            return Err(ValidationError("NOL 캡챠 판단용 화면 또는 관측 시각이 올바르지 않습니다".into()));
        }
        let dimensions = (frame.width, frame.height);
        if self.dimensions.is_some_and(|previous| previous != dimensions) { self.reset(); }
        self.dimensions = Some(dimensions);
        if !allow_same_check && self.last_checked_ms == Some(checked_at_ms) { return Ok(false); }
        if self.last_checked_ms.is_some_and(|previous| checked_at_ms < previous)
            || last_observed_ns < self.last_observed_ns
            || sequence < self.last_sequence || captured_at_ns < self.last_capture_ns
            || (sequence == self.last_sequence && self.last_capture_ns != 0 && captured_at_ns != self.last_capture_ns)
            || (sequence > self.last_sequence && captured_at_ns == self.last_capture_ns)
        {
            self.interrupt();
            self.backdrop.gate_reason = NolCaptchaGateReason::StaleObservation;
            return Ok(false);
        }
        self.last_sequence = sequence;
        self.last_capture_ns = captured_at_ns;
        self.last_observed_ns = last_observed_ns;
        self.last_checked_ms = Some(checked_at_ms);
        Ok(true)
    }

    fn inspect_backdrop(&mut self, frame: RgbaView<'_>) -> bool {
        let patch = backdrop_patch(frame);
        let samples = patch_samples(frame, patch);
        self.backdrop.patch = Some(patch);
        self.backdrop.sample_count = samples.len();
        self.backdrop.opaque_samples = samples.iter().filter(|sample| sample.1 >= 250).count();
        self.backdrop.frame_opaque_basis_points = frame_opacity(frame);
        self.backdrop.mean_rgb = Some(mean_rgb(samples.iter().map(|sample| sample.0)));
        self.backdrop.changed_samples = 0;
        if self.backdrop.frame_opaque_basis_points < 9800 || self.backdrop.opaque_samples != PATCH_SAMPLES {
            self.interrupt();
            self.backdrop.gate_reason = NolCaptchaGateReason::TransparentFrame;
            return false;
        }
        true
    }

    fn observe_inner(
        &mut self, frame: RgbaView<'_>, sequence: u64,
        captured_at_ns: u64, last_observed_ns: u64, checked_at_ms: u64, recheck: bool,
    ) -> Result<NolCaptchaObservation, ValidationError> {
        if !self.prepare_observation(frame, sequence, captured_at_ns, last_observed_ns, checked_at_ms, recheck)? {
            return Ok(self.observation(false));
        }
        let was_dismissed = self.status == NolCaptchaStatus::Dismissed;
        if was_dismissed && !recheck { return Ok(self.observation(false)); }
        if !self.inspect_backdrop(frame) { return Ok(self.observation(false)); }
        let Some(known) = self.presence.as_ref() else {
            self.interrupt();
            self.backdrop.gate_reason = NolCaptchaGateReason::WaitingForInput;
            return Ok(self.observation(false));
        };
        self.backdrop.baseline_samples = known.baseline.len();
        self.backdrop.baseline_mean_rgb = Some(mean_rgb(known.baseline.iter().map(|point| point.reference)));
        self.backdrop.changed_samples = known.baseline.iter().filter(|point| {
            let current = pixel(frame, point.x, point.y);
            current.iter().zip(point.reference.iter()).any(|(after, before)| after.abs_diff(*before) >= PATCH_CHANNEL_DELTA)
        }).count();
        let changed = sequence > known.sequence && captured_at_ns > known.captured_at_ns
            && self.backdrop.changed_samples >= REQUIRED_PATCH_SAMPLES;
        if !changed {
            self.interrupt();
            self.backdrop.gate_reason = if self.backdrop.changed_samples == 0 { NolCaptchaGateReason::BackgroundUnchanged }
                else { NolCaptchaGateReason::BackgroundPartial };
            return Ok(self.observation(false));
        }
        self.backdrop.gate_reason = NolCaptchaGateReason::BackgroundChanged;
        if recheck && was_dismissed {
            self.status = NolCaptchaStatus::Dismissed;
            return Ok(self.observation(false));
        }
        let since = *self.close_since_ms.get_or_insert(checked_at_ms);
        if checked_at_ms.saturating_sub(since) >= STABLE_CLOSE_MS {
            self.status = NolCaptchaStatus::Dismissed;
            self.close_since_ms = None;
            return Ok(self.observation(true));
        }
        self.status = NolCaptchaStatus::Closing;
        Ok(self.observation(false))
    }

    fn observation(&self, dismissed: bool) -> NolCaptchaObservation {
        NolCaptchaObservation {
            status: self.status, dismissed, panel: None, title_score_basis_points: None,
            backdrop: self.backdrop.clone(),
        }
    }
}

fn backdrop_patch(frame: RgbaView<'_>) -> NolCaptchaRect {
    NolCaptchaRect { x: frame.width * 3 / 100, y: frame.height * 88 / 100,
        width: (frame.width * 4 / 100).max(1), height: (frame.height * 4 / 100).max(1) }
}

fn patch_coordinates(patch: NolCaptchaRect) -> impl Iterator<Item = (u32, u32)> {
    (0..5).flat_map(move |row| (0..5).map(move |column|
        (patch.x + (column * 2 + 1) * patch.width / 10, patch.y + (row * 2 + 1) * patch.height / 10)))
}

fn patch_samples(frame: RgbaView<'_>, patch: NolCaptchaRect) -> Vec<([u8; 3], u8)> {
    patch_coordinates(patch).map(|(x, y)| (pixel(frame, x, y), frame.rgba[y as usize * frame.stride + x as usize * 4 + 3])).collect()
}

fn color_reference(frame: RgbaView<'_>, patch: NolCaptchaRect) -> Vec<BackdropPoint> {
    // Opacity was checked for all 25 points before recording the reference.
    patch_coordinates(patch).map(|(x, y)| BackdropPoint { x, y, reference: pixel(frame, x, y) }).collect()
}

fn mean_rgb(values: impl Iterator<Item = [u8; 3]>) -> [u8; 3] {
    let mut sum = [0u32; 3];
    let mut count = 0;
    for rgb in values { for index in 0..3 { sum[index] += rgb[index] as u32; } count += 1; }
    if count == 0 { [0; 3] } else { sum.map(|channel| (channel / count) as u8) }
}

fn frame_opacity(frame: RgbaView<'_>) -> u16 {
    let mut opaque = 0;
    for row in 0..24 {
        for column in 0..32 {
            let x = (column * 2 + 1) * frame.width / 64;
            let y = (row * 2 + 1) * frame.height / 48;
            opaque += usize::from(frame.rgba[y as usize * frame.stride + x as usize * 4 + 3] >= 250);
        }
    }
    (opaque * 10_000 / (24 * 32)) as u16
}

fn valid_frame(frame: RgbaView<'_>) -> bool {
    frame.width > 0 && frame.height > 0
        && (frame.width as usize).checked_mul(4).is_some_and(|row| frame.stride >= row)
        && frame.stride.checked_mul(frame.height as usize) == Some(frame.rgba.len())
}

fn pixel(frame: RgbaView<'_>, x: u32, y: u32) -> [u8; 3] {
    let offset = y as usize * frame.stride + x as usize * 4;
    [frame.rgba[offset], frame.rgba[offset + 1], frame.rgba[offset + 2]]
}
