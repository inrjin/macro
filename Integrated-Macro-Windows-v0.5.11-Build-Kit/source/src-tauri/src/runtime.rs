use crate::{clock::{self, ClockEstimate}, keyboard_check::{self, KeyboardCheck}, storage, visual_recovery::{VisualRecovery, VisualSignal}};
use macro_core::{
    Action, ActionKind, Candidate, Color, Engine, Event, Key, Phase, Point, Profile,
    Rect, RegionCandidate, RunMode, SeatOutcome, Target, VisionConfig,
};
use macro_platform::{InputReceipt, NativeAdapter, OwnedFrame};
use macro_core::nol_captcha::{NolCaptchaDetector, NolCaptchaGateReason, NolCaptchaStatus};
use macro_platform::FrameObservation;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    fs::{self, OpenOptions},
    io::Write,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::{atomic::{AtomicBool, AtomicU64, Ordering}, mpsc, Arc},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct DesktopConfig {
    sale_time: Option<String>,
    scheduled_start: bool,
    refresh_mode: String,
    cancellation_mode: String,
    selected_color: Option<Color>,
    stage_center: Option<Point>,
    region_roi: Option<Rect>,
    capture_reference: Option<FrameReference>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FrameReference { width: usize, height: usize, scale_x: f64, scale_y: f64 }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CaptureState {
    #[default]
    Idle,
    Armed,
    Reading,
    Error,
    Captured,
}

#[derive(Clone, Debug, Serialize)]
struct TargetRecovery {
    active: bool,
    reason: String,
    #[serde(skip)]
    next_check_ms: u64,
    #[serde(skip)]
    blocked_point: Option<Point>,
    #[serde(skip)]
    blocked_image_candidate: bool,
}

fn recoverable_context_error(reason: &str) -> bool {
    let Some(code) = reason.strip_prefix('[').and_then(|s| s.split_once(']').map(|p| p.0)) else {
        return false;
    };
    matches!(code, "target_not_foreground" | "target_window_changed" | "target_not_visible"
        | "target_focus_unavailable" | "target_hit_test_unavailable" | "target_occluded"
        | "capture_waiting" | "capture_interrupted" | "capture_frame_missing"
        | "capture_failed" | "capture_flush_failed" | "capture_incomplete")
}

fn automatic_context_recovery(continuous: bool, state: &macro_core::Snapshot) -> bool {
    continuous || state.selection_count_verification
        && matches!(state.phase, Phase::Seats | Phase::Conflict | Phase::Completing)
}

fn expired_start_owns_run(cancelled_epoch: u64, current_epoch: u64, run_epoch: Option<u64>) -> bool {
    current_epoch == cancelled_epoch && run_epoch == Some(cancelled_epoch.wrapping_sub(1))
}

fn input_cancelled_before_delivery(reason: &str) -> bool {
    reason.starts_with("[input_cancelled]")
}

fn check_profile_revision(expected: u64, current: u64) -> Result<(), String> {
    if expected == current { Ok(()) } else {
        Err("[profile_revision_conflict] 좌표나 설정이 다른 작업에서 변경되었습니다. 최신 설정을 확인한 뒤 다시 실행하세요.".into())
    }
}

fn validate_visual_recovery_frame(frame: &OwnedFrame, visual_enabled: bool) -> Result<(), String> {
    if visual_enabled {
        macro_core::visual_dialog::sample_frame(macro_core::vision::RgbaView {
            width: frame.width as u32, height: frame.height as u32,
            stride: frame.stride, rgba: &frame.rgba,
        }, frame.sequence).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn frame_passes_transition(sequence: u64, after: Option<u64>) -> bool {
    after.is_none_or(|previous| sequence > previous)
}

fn frame_passes_input_transition(frame: &OwnedFrame, after: Option<u64>, started_at_ns: Option<u64>) -> bool {
    frame_passes_transition(frame.sequence, after)
        && started_at_ns.is_none_or(|started| frame.captured_at_ns > started)
}

fn selection_count_frame_fresh(frame: &OwnedFrame, run_id: u64, after_action_id: Option<u64>,
    receipt: Option<(u64, u64, u64)>) -> bool {
    frame.last_observed_ns >= frame.captured_at_ns && after_action_id.is_none_or(|id| receipt
        .is_some_and(|(run, action, started)| run == run_id && action == id && frame.last_observed_ns > started))
}

#[derive(Clone, Debug)]
struct SelectionCountEvidence {
    area: Rect,
    digest: u64,
    frame_digest: u64,
    count: usize,
    heading: Rect,
    width: usize,
    height: usize,
    stride: usize,
    scale_x: f64,
    scale_y: f64,
    origin_x: f64,
    origin_y: f64,
    sequence: u64,
    captured_at_ns: u64,
    last_observed_ns: u64,
}

#[derive(Clone, Debug)]
struct SelectionCountInputProof {
    run_id: u64,
    action_id: u64,
    request_id: u64,
    observation_id: u64,
    evidence: SelectionCountEvidence,
}

#[derive(Clone, Debug)]
struct SelectionCountReadCache {
    width: usize,
    height: usize,
    stride: usize,
    digest: u64,
    value: macro_core::selection_count::CountDiagnosis,
}

fn cached_selection_count_read(frame: &OwnedFrame, cache: &mut Option<SelectionCountReadCache>,
    read: impl FnOnce(macro_core::vision::RgbaView<'_>) -> macro_core::selection_count::CountDiagnosis)
    -> (macro_core::selection_count::CountDiagnosis, u64) {
    let digest = selection_count_frame_digest(frame);
    if let Some(entry) = cache.as_ref().filter(|entry| entry.width == frame.width && entry.height == frame.height
        && entry.stride == frame.stride && entry.digest == digest) {
        return (entry.value.clone(), digest);
    }
    let value = read(macro_core::vision::RgbaView {width:frame.width as u32,height:frame.height as u32,
        stride:frame.stride,rgba:&frame.rgba});
    *cache = Some(SelectionCountReadCache {width:frame.width,height:frame.height,stride:frame.stride,digest,value:value.clone()});
    (value, digest)
}

fn selection_count_issue_message(issue: Option<macro_core::selection_count::CountIssue>) -> &'static str {
    use macro_core::selection_count::CountIssue;
    match issue {
        Some(CountIssue::InvalidFrame | CountIssue::IncompleteCapture) =>
            "NOL 선택 좌석 수 확인 · 화면을 온전히 읽지 못해 다시 확인 중",
        Some(CountIssue::HeadingMissing) =>
            "NOL 선택 좌석 수 확인 · ‘선택 좌석’ 문구를 찾지 못함 · 글꼴·가림·화면 배율 확인",
        Some(CountIssue::HeadingAmbiguous) =>
            "NOL 선택 좌석 수 확인 · ‘선택 좌석’ 문구가 여러 곳에 보여 입력 보류",
        Some(CountIssue::HeadingContext) =>
            "NOL 선택 좌석 수 확인 · 문구의 주변 표시가 불명확해 수량 판단 보류",
        Some(CountIssue::NumericContext) =>
            "NOL 선택 좌석 수 확인 · 수량 표시가 온전하지 않거나 서로 모순되어 입력 보류",
        Some(CountIssue::DigitUnrecognized) =>
            "NOL 선택 좌석 수 확인 · 숫자를 확실히 읽지 못해 다시 확인 중",
        Some(CountIssue::EmptyLayoutUnsupported) =>
            "NOL 선택 좌석 수 확인 · ‘선택된 좌석이 없습니다’ 안내 영역을 확인하지 못해 0석 판단 보류",
        Some(CountIssue::CountMissing) =>
            "NOL 선택 좌석 수 확인 · 수량 또는 선택 없음 안내를 찾지 못해 다시 확인 중",
        None => "NOL 선택 좌석 수 확인 · 최신 화면의 수량 근거를 다시 확인 중",
    }
}

fn selection_count_observation_has_map(engine: &Engine, count: usize, current_candidates: bool) -> bool {
    current_candidates || !engine.selection_count_needs_seat_observation(count)
}

fn needs_selection_count_proof(kind: &ActionKind) -> bool {
    matches!(kind, ActionKind::CompleteSelection { .. } | ActionKind::Refresh { .. })
        || matches!(kind, ActionKind::Click { purpose, .. } if purpose.starts_with("seat:") || purpose == "legacy_cancellation_region")
        || matches!(kind, ActionKind::Key { purpose, .. } if purpose == "legacy_cancellation_region")
}

fn selection_count_region_digest(frame: &OwnedFrame, area: Rect) -> Option<u64> {
    if !area.valid() || area.x < 0.0 || area.y < 0.0
        || area.x + area.width > frame.width as f64 || area.y + area.height > frame.height as f64
        || frame.width.checked_mul(4).is_none_or(|row| row > frame.stride)
        || frame.stride.checked_mul(frame.height) != Some(frame.rgba.len()) { return None; }
    let (left, top) = (area.x.floor() as usize, area.y.floor() as usize);
    let (right, bottom) = ((area.x + area.width).ceil() as usize, (area.y + area.height).ceil() as usize);
    if left >= right || top >= bottom { return None; }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (left, top, right, bottom).hash(&mut hasher);
    for y in top..bottom {
        let row = frame.rgba.get(y * frame.stride + left * 4..y * frame.stride + right * 4)?;
        if row.chunks_exact(4).any(|pixel| pixel[3] < 250) { return None; }
        hasher.write(row);
    }
    Some(hasher.finish())
}

impl SelectionCountEvidence {
    #[cfg(test)]
    fn capture(frame: &OwnedFrame, count: usize, heading: Rect, area: Rect) -> Option<Self> {
        Self::capture_with_digest(frame,count,heading,area,selection_count_frame_digest(frame))
    }

    // `frame_digest` comes from this same frame's pure reader cache lookup;
    // request, freshness and per-action permission are validated separately.
    fn capture_with_digest(frame: &OwnedFrame, count: usize, heading: Rect, area: Rect, frame_digest:u64) -> Option<Self> {
        if !frame.scale_x.is_finite() || frame.scale_x <= 0.0 || !frame.scale_y.is_finite() || frame.scale_y <= 0.0
            || !frame.origin.x.is_finite() || !frame.origin.y.is_finite()
            || frame.last_observed_ns < frame.captured_at_ns { return None; }
        Some(Self { area, digest: selection_count_region_digest(frame, area)?, frame_digest,
            count, heading, width: frame.width, height: frame.height, stride: frame.stride,
            scale_x: frame.scale_x, scale_y: frame.scale_y, origin_x: frame.origin.x, origin_y: frame.origin.y,
            sequence: frame.sequence, captured_at_ns: frame.captured_at_ns, last_observed_ns: frame.last_observed_ns })
    }

    fn matches(&self, frame: &OwnedFrame) -> bool {
        self.width == frame.width && self.height == frame.height && self.stride == frame.stride && self.scale_x == frame.scale_x
            && self.scale_y == frame.scale_y && self.origin_x == frame.origin.x && self.origin_y == frame.origin.y
            && frame.sequence >= self.sequence && frame.captured_at_ns >= self.captured_at_ns
            && frame.last_observed_ns >= self.last_observed_ns && frame.last_observed_ns >= frame.captured_at_ns
            && selection_count_region_digest(frame, self.area) == Some(self.digest)
    }
}

fn selection_count_frame_digest(frame: &OwnedFrame) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // Padding is included conservatively: a change can cause one re-read but
    // never suppress revalidation of a changed global reader precondition.
    hasher.write(&frame.rgba);
    hasher.finish()
}

fn selection_count_input_allowed(proof: Option<&SelectionCountInputProof>, frame: Option<&OwnedFrame>,
    run_id: u64, action_id: u64) -> bool {
    selection_count_input_allowed_with_reader(proof, frame, run_id, action_id, macro_core::selection_count::read_selection_count)
}

fn selection_count_input_allowed_with_reader(proof: Option<&SelectionCountInputProof>, frame: Option<&OwnedFrame>,
    run_id: u64, action_id: u64, read: impl FnOnce(macro_core::vision::RgbaView<'_>) -> macro_core::selection_count::CountRead) -> bool {
    let (Some(proof), Some(frame)) = (proof, frame) else { return false; };
    if proof.run_id != run_id || proof.action_id != action_id || !proof.evidence.matches(frame) { return false; }
    if selection_count_frame_digest(frame) == proof.evidence.frame_digest { return true; }
    // Numeric crops alone do not cover global ambiguity (a second heading or
    // contradictory empty label). Re-read this latest acquired frame once;
    // do not loop on unrelated animation or claim atomicity with OS delivery.
    read(macro_core::vision::RgbaView { width:frame.width as u32, height:frame.height as u32,
        stride:frame.stride, rgba:&frame.rgba }) == macro_core::selection_count::CountRead::Confirmed {
            count:proof.evidence.count, heading:proof.evidence.heading, evidence:proof.evidence.area }
}

#[cfg(test)]
#[path = "selection_count_runtime_tests.rs"]
mod selection_count_runtime_tests;

#[cfg(test)]
mod selection_count_capture_tests {
    use super::*;
    fn frame() -> OwnedFrame {
        OwnedFrame { width: 2, height: 2, stride: 8, rgba: vec![255;16],
            origin: macro_platform::Point { x:0.0, y:0.0 }, scale_x:2.0, scale_y:2.0,
            sequence:10, captured_at_ns:100, last_observed_ns:100 }
    }
    #[test]
    fn post_click_counts_reject_old_run_wrong_action_and_stale_observation() {
        let f=frame();
        for receipt in [None, Some((1,7,90)), Some((2,6,90)), Some((2,7,100)), Some((2,7,101))] {
            assert!(!selection_count_frame_fresh(&f,2,Some(7),receipt));
        }
        assert!(selection_count_frame_fresh(&f,2,Some(7),Some((2,7,99))));
    }
    #[test]
    fn native_reobservation_can_confirm_static_count_without_forging_changed_pixels() {
        let mut cached=Some(frame());
        apply_frame_observation(&mut cached,FrameObservation::Unchanged {
            sequence:10,captured_at_ns:100,last_observed_ns:130 }).unwrap();
        let f=cached.as_ref().unwrap();
        assert_eq!(f.sequence,10);
        assert_eq!(f.captured_at_ns,100);
        assert!(selection_count_frame_fresh(f,2,Some(7),Some((2,7,120))));
        // A count observation is not evidence that a refreshed map has arrived.
        assert!(!frame_passes_input_transition(f,Some(10),Some(120)));
    }
    #[test]
    fn impossible_native_timestamp_is_never_a_count_baseline() {
        let mut f=frame(); f.last_observed_ns=99;
        assert!(!selection_count_frame_fresh(&f,2,None,None));
    }
}

// Shared by the production observation loop and its headless capture-boundary
// tests. None means no new native pixels, not a successful page transition.
fn apply_scan_capture(cached: &mut Option<OwnedFrame>, last_sequence: &mut Option<u64>,
    capture: Result<Option<OwnedFrame>, String>, reuse: bool) -> Result<bool, String> {
    match capture {
        Ok(Some(frame)) => {
            *last_sequence = Some(frame.sequence);
            *cached = Some(frame);
            Ok(true)
        }
        Ok(None) => Ok(reuse && cached.is_some()),
        Err(reason) => Err(reason),
    }
}

fn navigation_requires_new_pixels(kind: &ActionKind, continuous: bool, delivered: bool) -> bool {
    delivered && continuous
        && matches!(kind, ActionKind::Click { purpose, .. } if purpose.starts_with("region:"))
}

fn is_image_candidate_action(kind: &ActionKind) -> bool {
    matches!(kind, ActionKind::Click { purpose, .. } if purpose.starts_with("seat:") || purpose.starts_with("region:"))
}

// Vision targets are captured pixel indices. Sample their centers so division
// and multiplication at a fractional scale cannot floor into the neighbor.
// Windows' native input adapter already centers its physical-pixel indices;
// preserve that input contract instead of adding a second half-pixel there.
fn image_candidate_local(frame: &OwnedFrame, point: Point, windows_native_index: bool) -> Result<macro_platform::Point, String> {
    if !point.x.is_finite() || !point.y.is_finite() || point.x < 0.0 || point.y < 0.0
        || point.x >= frame.width as f64 || point.y >= frame.height as f64
        || !frame.scale_x.is_finite() || !frame.scale_y.is_finite() || frame.scale_x <= 0.0 || frame.scale_y <= 0.0 {
        return Err("좌석 후보의 픽셀 위치나 화면 배율이 올바르지 않습니다. 현재 화면을 다시 확인하세요.".into());
    }
    if windows_native_index || point.x.fract() != 0.0 || point.y.fract() != 0.0 {
        return Ok(macro_platform::Point { x: point.x / frame.scale_x, y: point.y / frame.scale_y });
    }
    frame.pixel_to_local(point.x as usize, point.y as usize)
        .ok_or_else(|| "좌석 후보가 현재 화면 범위를 벗어났습니다.".into())
}

fn candidate_available_in_frame(frame: Option<&OwnedFrame>, target: &Target, color: Color, tolerance: u8,
    refresh_after: Option<u64>, refresh_after_ns: Option<u64>) -> Result<bool, String> {
    let Target::Point { point } = target else {
        return Err("브라우저 요소 연결이 아직 준비되지 않았습니다.".into());
    };
    let frame = frame.ok_or("최신 좌석 화면을 받을 수 없습니다.")?;
    if !frame_passes_input_transition(frame, refresh_after, refresh_after_ns) { return Ok(false); }
    let local = image_candidate_local(frame, *point, false)?;
    Ok(frame.color_at(local).is_some_and(|[r,g,b]| {
        macro_core::vision::color_matches(Color{r,g,b}, color, tolerance)
    }))
}

fn seat_candidate_available_in_frame(frame: Option<&OwnedFrame>, target: &Target, color: Color, tolerance: u8,
    config: &VisionConfig, refresh_after: Option<u64>, refresh_after_ns: Option<u64>) -> Result<bool, String> {
    if !candidate_available_in_frame(frame, target, color, tolerance, refresh_after, refresh_after_ns)? {
        return Ok(false);
    }
    let frame = frame.ok_or("최신 좌석 화면을 받을 수 없습니다.")?;
    let Target::Point { point } = target else { return Ok(false); };
    Ok(macro_core::vision::seat_point_supported(macro_core::vision::RgbaView {
        width: frame.width as u32, height: frame.height as u32, stride: frame.stride, rgba: &frame.rgba,
    }, *point, color, tolerance, config))
}

fn apply_candidate_capture(cached: &mut Option<OwnedFrame>, reference: Option<&FrameReference>,
    capture: Result<Option<OwnedFrame>, String>) -> Result<(), String> {
    if let Some(frame) = capture? {
        let previous_geometry_changed = cached.as_ref().is_some_and(|previous|
            previous.width != frame.width || previous.height != frame.height
            || (previous.scale_x - frame.scale_x).abs() > 0.001
            || (previous.scale_y - frame.scale_y).abs() > 0.001);
        if previous_geometry_changed || reference.is_some_and(|reference| !same_frame_reference(reference, &frame)) {
            return Err("클릭 직전 화면 크기나 배율이 바뀌었습니다. 현재 화면에서 좌표를 다시 채집하세요.".into());
        }
        *cached = Some(frame);
    }
    Ok(())
}

fn apply_frame_observation(cached: &mut Option<OwnedFrame>, observation: FrameObservation) -> Result<(), String> {
    match observation {
        FrameObservation::Changed(frame) => { *cached = Some(frame); }
        FrameObservation::Unchanged { sequence, captured_at_ns, last_observed_ns } => {
            let frame = cached.as_mut().ok_or("NOL 캡챠 판단: 비교할 화면이 없습니다.")?;
            if frame.sequence != sequence || frame.captured_at_ns != captured_at_ns
                || last_observed_ns < frame.last_observed_ns || last_observed_ns < captured_at_ns {
                return Err("NOL 캡챠 판단: 화면 관측 정보가 일치하지 않습니다.".into());
            }
            frame.last_observed_ns = last_observed_ns;
        }
    }
    Ok(())
}

fn nol_captcha_message(status: NolCaptchaStatus) -> &'static str {
    match status {
        NolCaptchaStatus::Searching => "NOL 캡챠 판단 · 첫 문자 입력 대기 · 완료했다면 F9",
        NolCaptchaStatus::Visible => "NOL 캡챠 판단 · 첫 입력 기준 저장됨 · 배경 색 변화 대기",
        NolCaptchaStatus::Closing => "NOL 캡챠 판단 · 왼쪽 아래 배경 색 변화 확인 중",
        NolCaptchaStatus::Dismissed => "NOL 캡챠 배경 색 변화 감지 · 구역·좌석 탐색으로 진행",
    }
}

const CAPTCHA_INPUT_FRAME_WINDOW_NS: u64 = 250_000_000;
const MAX_PENDING_CAPTCHA_INPUTS: usize = 16;

fn captcha_input_frame_ready(input_ns: u64, frame: &OwnedFrame) -> Result<bool, &'static str> {
    if input_ns == 0 || frame.captured_at_ns == 0 || frame.captured_at_ns > frame.last_observed_ns {
        return Err("invalid_capture_time");
    }
    let Some(age) = frame.last_observed_ns.checked_sub(input_ns) else {
        // AppKit can deliver the key before ScreenCaptureKit reports its frame.
        return Ok(false);
    };
    if age > CAPTCHA_INPUT_FRAME_WINDOW_NS { return Err("late_capture"); }
    Ok(true)
}

struct PendingCaptchaInput { press: macro_platform::CaptchaInput, received_at: Instant }

#[derive(Debug, PartialEq, Eq)]
enum CaptchaInputCandidate {
    Ready(macro_platform::CaptchaInput),
    WaitingForFrame,
    Empty,
}

// Keep notification order, but do not let one obsolete input discard a later
// input whose reference frame is already available in this same capture.
fn pending_captcha_input_candidate(pending: &mut VecDeque<PendingCaptchaInput>, frame: &OwnedFrame,
    now: Instant, mut reject: impl FnMut(macro_platform::CaptchaInput, &'static str)) -> CaptchaInputCandidate {
    while let Some(input) = pending.front() {
        let readiness = if now.saturating_duration_since(input.received_at)
            > Duration::from_nanos(CAPTCHA_INPUT_FRAME_WINDOW_NS) {
            Err("capture_wait_expired")
        } else {
            captcha_input_frame_ready(input.press.observed_at_ns, frame)
        };
        match readiness {
            Ok(true) => return CaptchaInputCandidate::Ready(input.press),
            Ok(false) => return CaptchaInputCandidate::WaitingForFrame,
            Err(reason) => {
                let input = pending.pop_front().unwrap();
                reject(input.press, reason);
            }
        }
    }
    CaptchaInputCandidate::Empty
}

fn captcha_input_watch_needed(engine: &Engine, detector: &NolCaptchaDetector,
    capture_armed: bool, interrupted: bool) -> bool {
    engine.awaits_nol_captcha() && !detector.has_reference() && !capture_armed && !interrupted
}

fn visual_change_message(trigger: Option<macro_core::visual_dialog::VisualTriggerKind>) -> &'static str {
    use macro_core::visual_dialog::VisualTriggerKind;
    match trigger {
        Some(VisualTriggerKind::CentralChange) => "중앙 화면 변화 감지 · Esc 처리 중",
        Some(VisualTriggerKind::SurroundingDarkening) => "주변 어두워짐 감지 · Esc 처리 중",
        Some(VisualTriggerKind::Both) => "중앙 변화와 주변 어두워짐 감지 · Esc 처리 중",
        None => "안내창 후보 감지 · Esc 처리 중",
    }
}

fn enter_target_rejection(press: &macro_platform::EnterPress, token: u64, target: &macro_platform::TargetWindow) -> Option<&'static str> {
    if press.token != token { Some("stale_token") }
    else if press.pid != target.pid { Some("app_mismatch") }
    else if press.window_id != 0 && press.window_id != target.id { Some("window_mismatch") }
    else { None }
}

// Record every routing decision, including rejected notifications. The earlier
// find() silently discarded these, making an absent OS callback indistinguishable
// from a mismatched window. Signals contain no characters or key codes.
fn select_captcha_inputs(presses: Vec<macro_platform::CaptchaInput>, token: u64,
    target: Option<&macro_platform::TargetWindow>, received_at: Instant, mut record: impl FnMut(Value))
    -> VecDeque<PendingCaptchaInput> {
    let mut pending = VecDeque::new();
    for press in presses {
        let reason = match target {
            Some(target) => enter_target_rejection(&press, token, target),
            None => Some("target_unavailable"),
        };
        let retained = reason.is_none() && pending.len() < MAX_PENDING_CAPTCHA_INPUTS;
        record(json!({
            "signal":press, "acceptedTarget":reason.is_none(), "reason":reason,
            "retained":retained,
            "expectedToken":token, "boundPID":target.map(|target| target.pid),
            "boundWindowID":target.map(|target| target.id),
        }));
        if retained { pending.push_back(PendingCaptchaInput { press, received_at }); }
    }
    pending
}

fn waiting_for_seat_area(engine: &Engine) -> bool {
    engine.snapshot().phase == Phase::Seats && engine.effective_vision().roi.is_none()
}

fn seat_area_guidance(engine: &Engine) -> String {
    let prefix = engine.snapshot().active_region
        .filter(|&index| engine.profile().legacy.regions.get(index).is_some_and(|region| region.vision.is_some()))
        .map(|index| format!("{}구역 ", index + 1)).unwrap_or_default();
    let mut message = format!("‘{prefix}좌석 영역 · 첫 모서리’와 ‘{prefix}좌석 영역 · 반대 모서리’를 F8로 지정하세요.");
    if engine.confirmation_phase() == Phase::Seats {
        message.push_str(" 지정 후 예매창에서 F3으로 다시 진행합니다.");
    }
    message
}

fn seat_area_presence(profile: &Value) -> Value {
    json!({
        "area":profile.pointer("/vision/roi").is_some_and(|value| !value.is_null()),
        "start":profile.pointer("/desktop/capture_points/seat_start").is_some_and(|value| !value.is_null()),
        "end":profile.pointer("/desktop/capture_points/seat_end").is_some_and(|value| !value.is_null()),
    })
}

fn enter_rejection_message(reason: &str) -> Option<&'static str> {
    match reason {
        "window_mismatch" => Some("Enter 수신 · 연결한 창과 다른 창으로 판정했습니다. 현재 예매창 연결을 확인하세요."),
        "recipient_mismatch" => Some("Enter 수신 · 입력 대상과 전면 앱이 다르게 보고되어 진행하지 않았습니다."),
        "own_source" => Some("Enter 수신 · 프로그램 생성 입력으로 분류되어 진행하지 않았습니다."),
        "stale_event" | "future_event" => Some("Enter 수신 · 입력 시각 검사에서 제외됐습니다. 실행 기록에 사유를 남겼습니다."),
        _ => None,
    }
}

const START_SHORTCUT_TIMEOUT_MS: u64 = 2_000;

#[derive(Clone, Copy, Debug)]
struct StartShortcutRequest { id: u64, mode: RunMode, epoch: u64, expires_at_ms: u64 }

#[derive(Default)]
struct StartShortcutGate {
    next_id: u64,
    pending: Option<StartShortcutRequest>,
}

impl StartShortcutGate {
    fn prepare(&mut self, mode: RunMode, now: u64, epoch: u64) -> Option<u64> {
        if self.pending.is_some_and(|request| request.epoch == epoch && now < request.expires_at_ms) {
            return None;
        }
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let id = self.next_id;
        self.pending = Some(StartShortcutRequest {
            id, mode, epoch, expires_at_ms: now.saturating_add(START_SHORTCUT_TIMEOUT_MS),
        });
        Some(id)
    }

    fn take(&mut self, id: u64, now: u64, epoch: u64) -> Option<StartShortcutRequest> {
        let request = self.pending?;
        if request.id != id { return None; }
        self.pending = None;
        (request.epoch == epoch && now < request.expires_at_ms).then_some(request)
    }

    fn expire(&mut self, now: u64, epoch: u64) -> Option<RunMode> {
        let request = self.pending?;
        if request.epoch != epoch {
            self.pending = None;
            return None;
        }
        if now < request.expires_at_ms { return None; }
        self.pending = None;
        Some(request.mode)
    }
}

fn prepare_keyboard_check_controls(preflight: Result<(), String>, already_active: bool,
    received_epoch: u64, shortcuts: &mut StartShortcutGate, run_epoch: &mut Option<u64>,
    epoch: &AtomicU64, interrupt: &AtomicBool) -> Result<u64, String> {
    // Failed preflight leaves the existing run/shortcut/interrupt untouched.
    preflight?;
    if already_active { return Err("키보드 수신 검사가 이미 진행 중입니다.".into()); }
    let next_epoch = received_epoch.wrapping_add(1);
    epoch.compare_exchange(received_epoch, next_epoch, Ordering::SeqCst, Ordering::SeqCst)
        .map_err(|_| "키보드 검사 시작 요청이 취소되었습니다.".to_string())?;
    shortcuts.pending = None;
    *run_epoch = None;
    interrupt.store(false, Ordering::SeqCst);
    if epoch.load(Ordering::SeqCst) != next_epoch {
        return Err("키보드 검사 시작 요청이 취소되었습니다.".into());
    }
    Ok(next_epoch)
}

fn start_shortcut_label(mode: RunMode) -> &'static str {
    match mode {
        RunMode::Normal => "F1 일반 예매",
        RunMode::Seats => "F5 좌석 바로 탐색",
        RunMode::Cancellation => "F2 취소표 탐색",
    }
}

fn start_shortcut_key(mode: RunMode) -> &'static str {
    match mode {
        RunMode::Normal => "F1",
        RunMode::Seats => "F5",
        RunMode::Cancellation => "F2",
    }
}

fn manual_captcha_resume_required(engine: &Engine, capture_armed: bool) -> Result<bool, String> {
    if capture_armed {
        return Err("캡챠 이후 진행(F9): 좌표 지정을 마친 뒤 다시 누르세요.".into());
    }
    Ok(engine.snapshot().phase == Phase::Paused)
}

fn manual_captcha_request_current(received_epoch: u64, current_epoch: u64, interrupted: bool) -> bool {
    received_epoch == current_epoch && !interrupted
}

fn f9_starts_new_logical_run(engine: &Engine, run_id: u64) -> bool {
    // A stopped/faulted booking may still carry real delivery receipts; keep
    // that lineage. Idle is a fresh engine, including after settings/rebind.
    run_id == 0 || engine.snapshot().phase == Phase::Idle
}

fn binding_changes_context(previous: Option<&macro_platform::TargetWindow>, next: &macro_platform::TargetWindow) -> bool {
    previous.is_none_or(|previous| previous.id != next.id || previous.pid != next.pid)
}

fn reset_engine_after_rebind(engine: &mut Engine, profile: &Profile, changed: bool) -> Result<(), String> {
    if changed { *engine = Engine::new(profile.clone()).map_err(|error| error.to_string())?; }
    Ok(())
}

#[derive(Clone, Debug)]
pub enum Command {
    Bootstrap,
    Windows,
    Bind(Option<u64>),
    Save { profile: Value, expected_profile_revision: u64 },
    CaptureArmed,
    ArmCapture { field: String, profile: Option<Value>, expected_profile_revision: u64 },
    StartKeyboardCheck,
    StopKeyboardCheck,
    StartFromUi { mode: RunMode, profile: Value, expected_profile_revision: u64 },
    CancelExpiredStart { epoch: u64 },
    PrepareShortcut(RunMode),
    ApplyShortcut { request_id: u64, profile: Option<Value>, error: Option<String>, expected_profile_revision: u64 },
    Pause,
    Resume,
    ResumeFromUi,
    TogglePause,
    Stop,
    Continue,
    ContinueAfterCaptcha,
    Confirm { kind: String, run_id: u64, revision: u64, selected_ids: Option<Vec<String>> },
    Clock(String),
    Overlay(bool),
    OverlayIds(Vec<u64>),
    OverlayError(String),
    ShortcutStatus(Vec<String>),
    Shutdown,
}

impl Command {
    fn is_execution_request(&self) -> bool {
        matches!(self, Self::StartFromUi { .. } | Self::PrepareShortcut(_) | Self::ContinueAfterCaptcha)
    }

    fn cancels_start_shortcut(&self) -> bool {
        self.is_execution_request() || matches!(self, Self::Stop | Self::Pause | Self::TogglePause | Self::Shutdown
            | Self::ArmCapture { .. } | Self::CaptureArmed | Self::Bind(_)
            | Self::StartFromUi { .. } | Self::Resume | Self::ResumeFromUi)
    }

    fn cancels_keyboard_check(&self) -> bool {
        // Read-only status and overlay updates may coexist with the check.
        // Every operation that can run input, alter binding/configuration or
        // block the worker ends it before the normal command is handled.
        !matches!(self, Self::Bootstrap | Self::Windows | Self::StartKeyboardCheck | Self::StopKeyboardCheck | Self::ApplyShortcut { .. } | Self::CancelExpiredStart { .. }
            | Self::Overlay(_) | Self::OverlayIds(_) | Self::OverlayError(_) | Self::ShortcutStatus(_))
    }
}

type Reply = mpsc::SyncSender<Result<Value, String>>;

pub struct PendingReply {
    control_epoch: u64,
    rx: mpsc::Receiver<Result<Value, String>>,
    start_cancellation: Option<(RuntimeHandle, u64)>,
}

impl PendingReply {
    pub fn control_epoch(&self) -> u64 { self.control_epoch }

    pub fn wait(self) -> Result<Value, String> { self.wait_for(Duration::from_secs(20)) }

    fn wait_for(self, timeout: Duration) -> Result<Value, String> {
        match self.rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(error) => {
                if matches!(error, mpsc::RecvTimeoutError::Timeout) {
                    if let Some((handle, epoch)) = self.start_cancellation { handle.cancel_expired_start(epoch); }
                }
                Err("실행 엔진 응답이 지연되고 있습니다.".into())
            }
        }
    }
}

struct Request { command: Command, reply: Option<Reply>, shortcut_epoch: u64, received_at: Instant }

fn control_request_current(request: &Request, epoch: u64) -> bool {
    !request.command.cancels_start_shortcut() || matches!(request.command, Command::Shutdown)
        || request.shortcut_epoch == epoch
}

fn capture_commit_current(capture_epoch: u64, current_epoch: u64) -> bool {
    capture_epoch == current_epoch
}

fn known_conflict_retry_event(state: &macro_core::Snapshot, visual_incident: bool) -> Option<Event> {
    if state.phase != Phase::Conflict || state.pending_action_id.is_some() { return None; }
    if let Some(attempt) = &state.conflict_attempt {
        visual_incident.then(|| Event::DeliveredSeatResult {
            action_id: attempt.action_id, candidate_id: attempt.candidate_id.clone(),
            result: SeatOutcome::Conflict,
        })
    } else {
        state.current_candidate_id.as_ref().map(|candidate_id| Event::SeatResult {
            candidate_id: candidate_id.clone(), result: SeatOutcome::Conflict,
        })
    }
}

fn cancel_unposted_actions(engine: &mut Engine, actions: &mut VecDeque<Action>, now: u64) {
    for action in actions.drain(..) {
        if !matches!(action.kind, ActionKind::Observe { .. }) {
            // This queue contains only actions that have not entered native input.
            let _ = engine.step(Event::ActionCancelled { id:action.id }, now);
        }
    }
}

fn stopped_conflict_context(current: Option<RestartContext>, previous: Option<RestartContext>,
    visual: &mut VisualRecovery) -> Option<RestartContext> {
    let context = current.or(previous).filter(|context| context.engine.has_unresolved_conflict());
    // Stopped performs no observation/input. Retain only an already detected
    // incident, never an ordinary speculative seat watch.
    if context.is_none() || !visual.is_incident() { visual.reset(); }
    context
}

fn start_profile_save_needed(profile: &Value, revision: u64, stored: &Value, current_revision: u64) -> Result<bool, String> {
    if revision < current_revision && profile == stored { return Ok(false); }
    check_profile_revision(revision, current_revision)?;
    Ok(true)
}

#[derive(Clone)]
pub struct RuntimeHandle {
    tx: mpsc::Sender<Request>,
    interrupt: Arc<AtomicBool>,
    shortcut_epoch: Arc<AtomicU64>,
}

impl RuntimeHandle {
    pub fn launch(app: AppHandle, path: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let interrupt = Arc::new(AtomicBool::new(false));
        let shortcut_epoch = Arc::new(AtomicU64::new(0));
        let handle = Self { tx, interrupt: interrupt.clone(), shortcut_epoch: shortcut_epoch.clone() };
        thread::Builder::new().name("macro-engine".into())
            .spawn(move || {
                let failure_app = app.clone();
                let failure_path = path.with_file_name("runtime-events.jsonl");
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(||
                    Worker::new(app, path, interrupt, shortcut_epoch).run(rx)));
                if let Err(payload) = result {
                    let detail = payload.downcast_ref::<String>().cloned()
                        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "원인을 기록하지 못했습니다.".into());
                    let message = format!("실행 엔진이 중단되었습니다. 앱을 다시 실행하세요. [worker_panic] {detail}");
                    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(failure_path) {
                        let _ = writeln!(file, "{}", json!({"timeMs":clock::unix_ms(),"kind":"worker_panic","message":message}));
                    }
                    let _ = failure_app.emit("runtime-error", json!({"message":message}));
                    let main_app = failure_app.clone();
                    let _ = failure_app.run_on_main_thread(move || {
                        if let Some(window) = main_app.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    });
                }
            })
            .expect("실행 엔진 스레드를 시작하지 못했습니다.");
        handle
    }

    pub fn send(&self, command: Command) {
        let received_at = Instant::now();
        let shortcut_epoch = self.receive_command(&command);
        let _ = self.tx.send(Request { command, reply: None, shortcut_epoch, received_at });
    }

    pub fn enqueue(&self, command: Command) -> Result<PendingReply, String> {
        let received_at = Instant::now();
        let shortcut_epoch = self.receive_command(&command);
        let start_cancellation = matches!(&command, Command::StartFromUi { .. })
            .then(|| (self.clone(), shortcut_epoch));
        let (tx, rx) = mpsc::sync_channel(1);
        self.tx.send(Request { command, reply: Some(tx), shortcut_epoch, received_at })
            .map_err(|_| "실행 엔진이 종료되었습니다.".to_string())?;
        Ok(PendingReply { control_epoch: shortcut_epoch, rx, start_cancellation })
    }

    pub fn control_epoch_is_current(&self, epoch: u64) -> bool {
        self.shortcut_epoch.load(Ordering::SeqCst) == epoch
    }

    fn cancel_expired_start(&self, epoch: u64) {
        let cancelled_epoch = epoch.wrapping_add(1);
        if self.shortcut_epoch.compare_exchange(epoch, cancelled_epoch, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return; // A newer start/control already superseded this request.
        }
        // Epoch invalidation already blocks guarded startup and native post.
        // Do not write the shared interrupt flag here: a newer start could
        // acquire its own epoch between this CAS and queueing cleanup.
        self.enqueue_expired_start_cleanup(cancelled_epoch);
    }

    fn enqueue_expired_start_cleanup(&self, cancelled_epoch: u64) {
        let _ = self.tx.send(Request {
            command: Command::CancelExpiredStart { epoch: cancelled_epoch }, reply: None,
            shortcut_epoch: cancelled_epoch, received_at: Instant::now(),
        });
    }

    fn receive_command(&self, command: &Command) -> u64 {
        // Invalidate before queueing: an F4/F8 arriving during save or capture
        // must win over a UI response already being processed on the worker.
        if matches!(command, Command::Stop | Command::Pause | Command::TogglePause
            | Command::Shutdown | Command::ArmCapture { .. } | Command::CaptureArmed) {
            // Publish interruption before its epoch. A delayed old sender must
            // not set this flag after a newer start has already cleared it.
            self.interrupt.store(true, Ordering::SeqCst);
        }
        if command.cancels_start_shortcut() {
            self.shortcut_epoch.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
        } else {
            self.shortcut_epoch.load(Ordering::SeqCst)
        }
    }
}

fn capture_label(field: &str) -> Result<String, String> {
    let label = match field {
        "entry_target" => "판매 시작 예매 버튼",
        "color" => "좌석 색상",
        "desktop.region_color" => "구역 색상",
        "desktop.selected_color" => "선택된 좌석 색상",
        "desktop.stage_center" => "무대 중앙",
        "vision.roi.start" => "좌석 범위 시작",
        "vision.roi.end" => "좌석 범위 끝",
        "desktop.region_roi.start" => "구역 범위 시작",
        "desktop.region_roi.end" => "구역 범위 끝",
        "legacy.date" => "날짜 버튼",
        "legacy.session" => "회차 버튼",
        "legacy.next" => "다음 버튼",
        "legacy.complete" => "선택 완료 버튼",
        "legacy.region_first" => "첫 번째 구역",
        "legacy.region_second" => "두 번째 구역",
        "refresh_target" => "취소표 새로고침 버튼",
        _ => {
            if let Some(rest) = field.strip_prefix("legacy.regions.") {
                if let Some((zone @ ("0" | "1"), name)) = rest.split_once('.') {
                    let name = match name {
                        "color" => "색상", "complete" => "완료 버튼",
                        "vision.roi.start" => "범위 시작", "vision.roi.end" => "범위 끝",
                        _ => return Err("지원하지 않는 좌표 지정 항목입니다.".into()),
                    };
                    return Ok(format!("{}구역 {name}", if zone == "0" { 1 } else { 2 }));
                }
            }
            return Err("지원하지 않는 좌표 지정 항목입니다.".into());
        }
    };
    Ok(label.into())
}

fn validate_entry_setup(profile: &Profile, desktop: &DesktopConfig, mode: RunMode) -> Result<(), String> {
    if mode == RunMode::Cancellation && desktop.cancellation_mode == "legacyRegions"
        && !matches!(&profile.legacy.cancellation_navigation,
            macro_core::CancellationNavigation::TwoRegions { first, second }
                if matches!(first, Target::Point { .. }) && first.valid()
                    && matches!(second, Target::Point { .. }) && second.valid()) {
        return Err("취소표 원본 두 구역: ‘원본 · 첫 번째 구역’과 ‘원본 · 두 번째 구역’을 F8로 지정하세요. 현재 구역만 탐색하려면 ‘한 구역 · 현재 좌석 계속 탐색’을 선택하세요.".into());
    }
    if mode == RunMode::Seats {
        if !profile.vision.roi.is_some_and(|roi| roi.valid()) {
            return Err("좌석 바로 탐색: ‘좌석 영역’의 두 모서리를 F8로 지정하세요.".into());
        }
        return Ok(());
    }
    if mode == RunMode::Normal && desktop.scheduled_start && profile.entry_target.is_none() {
        return Err("예약 시작: ‘판매 시작 예매 버튼’을 F8로 지정하세요. 시간 측정만으로 클릭할 버튼이 정해지지는 않습니다.".into());
    }
    if mode == RunMode::Normal && profile.features.auto_entry && profile.entry_target.is_none()
        && !profile.features.auto_captcha_resume && !profile.features.nol_captcha_detection
        && !profile.features.manual_entry {
        return Err("예매 단계 자동 진행: 브라우저 자동 인식 연결이 아직 없습니다. 기능을 끄거나 일반 예매의 ‘판매 시작 예매 버튼’을 지정하세요.".into());
    }
    Ok(())
}

fn entry_click_enabled(profile: &Profile, desktop: &DesktopConfig, mode: RunMode) -> bool {
    mode == RunMode::Normal && (desktop.scheduled_start || profile.features.auto_entry)
}

fn refresh_button_target(profile: &Profile, desktop: &DesktopConfig, mode: RunMode) -> Option<Target> {
    if mode != RunMode::Cancellation || !profile.features.automatic_refresh
        || !matches!(profile.legacy.cancellation_navigation, macro_core::CancellationNavigation::None)
        || !matches!(desktop.refresh_mode.as_str(), "button" | "internalButton") {
        return None;
    }
    profile.refresh_target.clone().filter(|target| matches!(target, Target::Point { .. }))
}

fn requested_refresh_button_missing(profile: &Profile, desktop: &DesktopConfig, mode: RunMode) -> bool {
    mode == RunMode::Cancellation && profile.features.automatic_refresh
        && matches!(profile.legacy.cancellation_navigation, macro_core::CancellationNavigation::None)
        && matches!(desktop.refresh_mode.as_str(), "button" | "internalButton")
        && !matches!(profile.refresh_target, Some(Target::Point { .. }))
}

fn apply_execution_options(profile: &mut Profile, desktop: &DesktopConfig, mode: RunMode) {
    // Saved options belong to their UI modes. Carrying an inactive refresh flag
    // into a legacy cancellation run changes its timed transitions into F6 waits.
    profile.refresh_target = refresh_button_target(profile, desktop, mode);
    profile.features.automatic_refresh = profile.refresh_target.is_some();
    if mode == RunMode::Cancellation { profile.features.region_priority = false; }
}

fn first_legacy_scan_requested(profile: &Profile, generation: u64, last_generation: Option<u64>) -> bool {
    last_generation != Some(generation) && profile.legacy.blind_click_sequence
        && !profile.features.conflict_recovery && !profile.features.automatic_refresh
}

fn same_frame_reference(reference: &FrameReference, frame: &OwnedFrame) -> bool {
    reference.width == frame.width && reference.height == frame.height
        && (reference.scale_x - frame.scale_x).abs() <= 0.001
        && (reference.scale_y - frame.scale_y).abs() <= 0.001
}

fn reference_geometry_changed(previous: Option<&FrameReference>, next: Option<&FrameReference>) -> bool {
    match (previous, next) {
        (Some(previous), Some(next)) => previous.width != next.width || previous.height != next.height
            || (previous.scale_x - next.scale_x).abs() > 0.001
            || (previous.scale_y - next.scale_y).abs() > 0.001,
        _ => false,
    }
}

fn apply_capture(raw: &Value, field: &str, point: Point, color: Color, reference: &FrameReference) -> Result<Value, String> {
    let mut profile = raw.clone();
    if !profile.get("desktop").is_some_and(Value::is_object) { profile["desktop"] = json!({}); }
    let previous_reference = serde_json::from_value::<FrameReference>(profile["desktop"]["capture_reference"].clone()).ok();
    let changed_geometry = reference_geometry_changed(previous_reference.as_ref(), Some(reference));
    if changed_geometry {
        // A color sample cannot bless old coordinates for a new capture scale.
        for field in ["date","session","next","complete"] { profile["legacy"][field] = Value::Null; }
        if profile["legacy"]["cancellation_navigation"]["type"] == "two_regions" {
            profile["legacy"]["cancellation_navigation"] = json!({"type":"none"});
        }
        if let Some(regions) = profile["legacy"]["regions"].as_array_mut() {
            for region in regions { region["vision"] = Value::Null; region["complete"] = Value::Null; }
        }
        profile["vision"]["roi"] = Value::Null;
        profile["vision"]["exclusions"] = json!([]);
        profile["desktop"]["stage_center"] = Value::Null;
        profile["desktop"]["region_roi"] = Value::Null;
        profile["desktop"]["capture_points"] = json!({});
        profile["refresh_target"] = Value::Null;
        profile["entry_target"] = Value::Null;
    }
    if !profile["desktop"].get("capture_points").is_some_and(Value::is_object) {
        profile["desktop"]["capture_points"] = json!({});
    }
    profile["desktop"]["capture_reference"] = serde_json::to_value(reference).unwrap();
    if let Some(zone_field) = field.strip_prefix("legacy.regions.") {
        let (index, suffix) = zone_field.split_once('.').ok_or("구역별 좌표 채집 항목을 확인하세요.")?;
        let index: usize = index.parse().map_err(|_| "구역 번호를 확인하세요.")?;
        if index > 1 { return Err("구역 번호는 1구역 또는 2구역이어야 합니다.".into()); }
        if !profile["legacy"]["regions"].is_array() { profile["legacy"]["regions"] = json!([]); }
        let regions = profile["legacy"]["regions"].as_array_mut().unwrap();
        while regions.len() <= index { regions.push(json!({"color":null,"vision":null,"complete":null})); }
        match suffix {
            "color" => profile["legacy"]["regions"][index]["color"] = serde_json::to_value(color).unwrap(),
            "complete" => profile["legacy"]["regions"][index]["complete"] = serde_json::to_value(Target::Point {point}).unwrap(),
            "vision.roi.start" | "vision.roi.end" => {
                let end = if suffix.ends_with("start") { "start" } else { "end" };
                profile["desktop"]["capture_points"][format!("zone{index}_{end}")] = serde_json::to_value(point).unwrap();
                let points = &profile["desktop"]["capture_points"];
                let a = serde_json::from_value::<Point>(points[format!("zone{index}_start")].clone());
                let b = serde_json::from_value::<Point>(points[format!("zone{index}_end")].clone());
                if let (Ok(a),Ok(b)) = (a,b) {
                    let rect = Rect { x:a.x.min(b.x), y:a.y.min(b.y), width:(a.x-b.x).abs(), height:(a.y-b.y).abs() };
                    if !rect.valid() { return Err("탐색 범위의 두 모서리는 서로 다른 위치여야 합니다.".into()); }
                    let mut vision = profile["legacy"]["regions"][index]["vision"].clone();
                    if !vision.is_object() { vision = profile["vision"].clone(); }
                    vision["roi"] = serde_json::to_value(rect).unwrap();
                    profile["legacy"]["regions"][index]["vision"] = vision;
                }
            }
            _ => return Err("지원하지 않는 구역별 좌표 채집 항목입니다.".into()),
        }
        return Ok(profile);
    }
    match field {
        "color" => profile["color"] = serde_json::to_value(color).unwrap(),
        "desktop.region_color" => {
            profile["region_color"] = serde_json::to_value(color).unwrap();
            profile["desktop"]["region_color"] = serde_json::to_value(color).unwrap();
        }
        "desktop.selected_color" => profile["desktop"]["selected_color"] = serde_json::to_value(color).unwrap(),
        "desktop.stage_center" => profile["desktop"]["stage_center"] = serde_json::to_value(point).unwrap(),
        "legacy.date" | "legacy.session" | "legacy.next" | "legacy.complete" => {
            profile["legacy"][field.strip_prefix("legacy.").unwrap()] =
                serde_json::to_value(Target::Point { point }).unwrap();
        }
        "refresh_target" | "entry_target" => profile[field] = serde_json::to_value(Target::Point { point }).unwrap(),
        _ => {
            let key = match field {
                "vision.roi.start" => "seat_start",
                "vision.roi.end" => "seat_end",
                "desktop.region_roi.start" => "region_start",
                "desktop.region_roi.end" => "region_end",
                "legacy.region_first" => "legacy_region_first",
                "legacy.region_second" => "legacy_region_second",
                _ => return Err("지원하지 않는 좌표 채집 항목입니다.".into()),
            };
            profile["desktop"]["capture_points"][key] = serde_json::to_value(point).unwrap();
            let points = &profile["desktop"]["capture_points"];
            let region = if key.starts_with("seat_") { Some(("seat_start","seat_end",false)) }
                else if key.starts_with("region_") { Some(("region_start","region_end",true)) } else { None };
            if let Some((first, second, is_region)) = region {
                if let (Ok(a),Ok(b)) = (serde_json::from_value::<Point>(points[first].clone()), serde_json::from_value::<Point>(points[second].clone())) {
                    let rect = Rect { x: a.x.min(b.x), y: a.y.min(b.y), width: (a.x-b.x).abs(), height: (a.y-b.y).abs() };
                    if !rect.valid() { return Err("탐색 범위의 두 모서리는 서로 다른 위치여야 합니다.".into()); }
                    if is_region { profile["desktop"]["region_roi"] = serde_json::to_value(rect).unwrap(); }
                    else { profile["vision"]["roi"] = serde_json::to_value(rect).unwrap(); }
                }
            } else {
                let first = points["legacy_region_first"].clone();
                let second = points["legacy_region_second"].clone();
                if first.is_object() && second.is_object()
                    && profile["desktop"]["cancellation_mode"] == "legacyRegions" {
                    profile["legacy"]["cancellation_navigation"] = json!({
                        "type":"two_regions","first":{"type":"point","point":first},"second":{"type":"point","point":second}
                    });
                }
            }
        }
    }
    Ok(profile)
}

// A requested idle scan may inspect a still-valid static frame. It does not
// establish a new click outcome or a completed site refresh.
fn can_rescan_seat_frame(state: &macro_core::Snapshot, scope: &str, requested: bool) -> bool {
    requested && state.phase == Phase::Seats && scope == "seats"
        && state.current_candidate_id.is_none() && state.pending_action_id.is_none()
}

// Only the current observation and the original in-flight click are needed for
// selected-color sampling. Keeping every historical ID grows without bound in
// long cancellation searches; replacing an in-flight point follows repainting
// instead of checking the point that was actually clicked.
fn update_candidate_cache(cache: &mut HashMap<String, Candidate>, observed: &[Candidate], current_id: Option<&str>) {
    let current = current_id.and_then(|id| cache.remove(id));
    cache.clear();
    for candidate in observed { cache.insert(candidate.id.clone(), candidate.clone()); }
    if let Some(current) = current { cache.insert(current.id.clone(), current); }
}

fn manual_confirmation_kind(state: &macro_core::Snapshot, resume_phase: Phase, scope: &str) -> Option<String> {
    let phase = if state.phase == Phase::Paused && matches!(resume_phase, Phase::Region | Phase::WaitingRegion) {
        resume_phase
    } else { state.phase };
    match phase {
        Phase::Captcha if !state.optimistic_selection => Some("captcha_completed"),
        Phase::Conflict => Some("dialog_gone"),
        Phase::WaitingRefresh if !state.optimistic_selection => Some("refresh_completed"),
        Phase::Region | Phase::WaitingRegion if !state.optimistic_selection => Some("region_map_ready"),
        Phase::Seats if !state.optimistic_selection && !state.selection_count_verification
            && (scope == "selection" || state.current_candidate_id.is_some()) => Some("selection"),
        Phase::Entry if !state.optimistic_selection => Some("seat_map"),
        _ => None,
    }.map(str::to_string)
}

fn context_confirmation_phase(state: &macro_core::Snapshot, resume_phase: Phase, recovering: bool) -> Phase {
    if recovering && (state.optimistic_selection || state.selection_count_verification) && state.phase == Phase::Paused
        && matches!(resume_phase, Phase::Seats | Phase::Completing | Phase::Conflict) {
        resume_phase
    } else {
        state.phase
    }
}

fn conflict_confirmation_event(state: &macro_core::Snapshot, selected_ids: Option<&[String]>) -> Result<Event, String> {
    let requested = selected_ids.and_then(|ids| ids.first());
    if state.optimistic_selection || state.selection_count_verification {
        // A queued current candidate has not received input. Attribute a generic
        // manual report to the latest delivered attempt, never to that candidate.
        // This is a fallback estimate; it cannot identify a delayed server error.
        let attempt = match requested {
            Some(id) => state.delivered_attempts.iter().rev().find(|attempt| &attempt.candidate_id == id),
            None => state.delivered_attempts.last(),
        }.ok_or("실패로 처리할 클릭 기록이 없습니다. 아직 전달하지 않은 좌석은 차감하지 않습니다.")?;
        return Ok(Event::DeliveredSeatResult {
            action_id: attempt.action_id,
            candidate_id: attempt.candidate_id.clone(),
            result: SeatOutcome::Conflict,
        });
    }
    let id = requested.cloned().or_else(|| state.current_candidate_id.clone())
        .ok_or("선점 실패한 좌석을 특정할 수 없습니다. 실패한 좌석을 확인하세요.")?;
    Ok(Event::SeatResult { candidate_id: id, result: SeatOutcome::Conflict })
}

#[cfg(test)]
fn captcha_test_frame(rgb: [u8; 3], sequence: u64, observed_at: u64) -> OwnedFrame {
    OwnedFrame {
        width: 100, height: 100, stride: 400,
        rgba: [rgb[0], rgb[1], rgb[2], 255].repeat(10_000),
        origin: macro_platform::Point { x: 0.0, y: 0.0 }, scale_x: 1.0, scale_y: 1.0,
        sequence, captured_at_ns: observed_at, last_observed_ns: observed_at,
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;

    #[test]
    fn keyboard_check_controls_cancel_before_shortcut_capture_and_manual_paths() {
        for command in [Command::PrepareShortcut(RunMode::Normal), Command::PrepareShortcut(RunMode::Seats),
            Command::StartFromUi { mode: RunMode::Cancellation, profile: json!({}), expected_profile_revision: 0 }, Command::CaptureArmed,
            Command::ArmCapture { field: "vision.roi.start".into(), profile: None, expected_profile_revision: 0 },
            Command::ContinueAfterCaptcha, Command::Bind(None), Command::Stop, Command::Pause,
            Command::Resume, Command::Shutdown, Command::Save { profile: json!({}), expected_profile_revision: 0 }] {
            assert!(command.cancels_keyboard_check(), "{command:?}");
        }
        for command in [Command::Bootstrap, Command::Windows, Command::StartKeyboardCheck,
            Command::StopKeyboardCheck, Command::Overlay(true)] {
            assert!(!command.cancels_keyboard_check(), "{command:?}");
        }
        // A rejected diagnostic request must not cancel an active run. A valid
        // idle start cancels any pending shortcut inside start_keyboard_check.
        assert!(!Command::StartKeyboardCheck.cancels_start_shortcut());
    }

    #[test]
    fn keyboard_check_failed_preflight_preserves_pending_shortcut_and_interrupt() {
        for (phase, capture, bound, active, received_epoch) in [
            (Phase::Captcha, false, true, false, 9),
            (Phase::Idle, true, true, false, 9),
            (Phase::Idle, false, false, false, 9),
            (Phase::Idle, false, true, true, 9),
            (Phase::Idle, false, true, false, 8),
        ] {
            let mut shortcuts = StartShortcutGate::default();
            let id = shortcuts.prepare(RunMode::Normal, 0, 9).unwrap();
            let mut run_epoch = Some(9);
            let epoch = AtomicU64::new(9);
            let interrupt = AtomicBool::new(true);
            assert!(prepare_keyboard_check_controls(keyboard_check::validate_start(phase, capture, bound),
                active, received_epoch, &mut shortcuts, &mut run_epoch, &epoch, &interrupt).is_err());
            assert_eq!(shortcuts.pending.unwrap().id, id);
            assert_eq!(run_epoch, Some(9));
            assert_eq!(epoch.load(Ordering::SeqCst), 9);
            assert!(interrupt.load(Ordering::SeqCst));
        }
    }

    #[test]
    fn keyboard_check_after_stop_clears_stale_interrupt_and_ignores_old_shortcut_response() {
        let mut engine = Engine::new(Profile::default()).unwrap();
        engine.step(Event::Stop, 0);
        let mut shortcuts = StartShortcutGate::default();
        let id = shortcuts.prepare(RunMode::Normal, 0, 9).unwrap();
        let mut run_epoch = Some(8);
        let epoch = AtomicU64::new(9);
        let interrupt = AtomicBool::new(true);
        assert_eq!(prepare_keyboard_check_controls(keyboard_check::validate_start(engine.snapshot().phase, false, true),
            false, 9, &mut shortcuts, &mut run_epoch, &epoch, &interrupt).unwrap(), 10);
        assert!(shortcuts.pending.is_none());
        assert_eq!(run_epoch, None);
        assert!(!interrupt.load(Ordering::SeqCst));
        let mut check = KeyboardCheck::default();
        check.start(10, Value::Null);
        let old_response = Command::ApplyShortcut { request_id: id, profile: None, error: None, expected_profile_revision: 0 };
        assert!(!old_response.cancels_keyboard_check());
        assert!(shortcuts.take(id, 11, epoch.load(Ordering::SeqCst)).is_none());
        assert!(check.active());
        assert_eq!(engine.snapshot().phase, Phase::Stopped);
        // A genuinely new F1/F2/F5 is still allowed to end the check first.
        assert!(Command::PrepareShortcut(RunMode::Normal).cancels_keyboard_check());
    }

    fn captcha_test_view(frame: &OwnedFrame) -> macro_core::vision::RgbaView<'_> {
        macro_core::vision::RgbaView {
            width: frame.width as u32, height: frame.height as u32, stride: frame.stride, rgba: &frame.rgba,
        }
    }

    #[test]
    fn captcha_reference_waits_for_input_time_frame_and_rejects_late_capture() {
        let input_ns = 1_000_000_000;
        let mut frame = captcha_test_frame([137; 3], 1, input_ns - 10_000_000);
        assert_eq!(captcha_input_frame_ready(input_ns, &frame), Ok(false));
        // A validated static image need not be replaced with artificial pixels.
        frame.last_observed_ns = input_ns;
        assert_eq!(captcha_input_frame_ready(input_ns, &frame), Ok(true));
        frame.last_observed_ns += CAPTCHA_INPUT_FRAME_WINDOW_NS;
        assert_eq!(captcha_input_frame_ready(input_ns, &frame), Ok(true));
        frame.last_observed_ns += 1;
        assert_eq!(captcha_input_frame_ready(input_ns, &frame), Err("late_capture"));
        assert_eq!(captcha_input_frame_ready(0, &frame), Err("invalid_capture_time"));
    }

    #[test]
    fn first_character_only_arms_color_reference_and_later_background_change_releases_engine() {
        let mut profile = Profile::default();
        profile.features.nol_captcha_detection = true;
        profile.features.auto_captcha_resume = true;
        let mut engine = Engine::new(profile).unwrap();
        engine.step(Event::Start { mode: RunMode::Normal }, 0);
        let mut detector = NolCaptchaDetector::default();
        assert!(captcha_input_watch_needed(&engine, &detector, false, false));
        assert!(!captcha_input_watch_needed(&engine, &detector, true, false));
        assert!(!captcha_input_watch_needed(&engine, &detector, false, true));
        let first = captcha_test_frame([137; 3], 1, 1_000_000_000);
        // This image has no heading or panel, and before typing cannot arm.
        assert!(!detector.observe(captcha_test_view(&first), 1, first.captured_at_ns, first.last_observed_ns, 0).unwrap().dismissed);
        assert!(!detector.has_reference());
        assert!(captcha_input_frame_ready(first.captured_at_ns, &first).unwrap());
        let armed = detector.arm_on_input(captcha_test_view(&first), 1, first.captured_at_ns, first.last_observed_ns, 1).unwrap();
        assert!(!armed.dismissed);
        assert!(!captcha_input_watch_needed(&engine, &detector, false, false));
        assert_eq!(engine.snapshot().phase, Phase::Captcha);
        assert!(engine.step(Event::Tick, 2).is_empty());
        let mut typed = first.clone();
        typed.rgba[(50 * 100 + 50) * 4..(50 * 100 + 50) * 4 + 3].fill(255);
        assert!(!detector.observe(captcha_test_view(&typed), 2, 1_010_000_000, 1_010_000_000, 20).unwrap().dismissed);
        assert_eq!(engine.snapshot().phase, Phase::Captcha);
        let after = captcha_test_frame([235; 3], 3, 1_100_000_000);
        assert!(!detector.observe(captcha_test_view(&after), 3, after.captured_at_ns, after.last_observed_ns, 100).unwrap().dismissed);
        let closed = detector.observe(captcha_test_view(&after), 3, after.captured_at_ns, 1_200_000_000, 200).unwrap();
        assert!(closed.dismissed);
        engine.step(Event::NolCaptchaDismissed, 200);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        assert!(!captcha_input_watch_needed(&engine, &detector, false, false));
    }

    #[test]
    fn captcha_input_batch_is_bounded_and_preserves_earliest_notifications() {
        let target = macro_platform::TargetWindow { id: 44, pid: 55, title: String::new(),
            rect: macro_platform::Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 } };
        let presses: Vec<_> = (1..=20).map(|observed_at_ns| macro_platform::CaptchaInput {
            token: 7, window_id: 44, pid: 55, observed_at_ns,
        }).collect();
        let mut records = Vec::new();
        let pending = select_captcha_inputs(presses.clone(), 7, Some(&target), Instant::now(),
            |record| records.push(record));
        assert_eq!(pending.len(), MAX_PENDING_CAPTCHA_INPUTS);
        assert_eq!(pending.iter().map(|input| input.press).collect::<Vec<_>>(), presses[..16]);
        assert_eq!(records.len(), 20);
        assert!(records.iter().all(|record| record["acceptedTarget"] == true));
        assert_eq!(records.iter().filter(|record| record["retained"] == true).count(), 16);
    }

    #[test]
    fn captcha_input_batch_retries_a_later_frame_eligible_signal_without_advancing_engine() {
        let target = macro_platform::TargetWindow { id: 44, pid: 55, title: String::new(),
            rect: macro_platform::Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 } };
        let first = macro_platform::CaptchaInput { token: 7, window_id: 44, pid: 55, observed_at_ns: 1_000_000_000 };
        let next = macro_platform::CaptchaInput { observed_at_ns: 1_200_000_000, ..first };
        let last = macro_platform::CaptchaInput { observed_at_ns: 1_250_000_000, ..first };
        let received_at = Instant::now();
        let mut pending = select_captcha_inputs(vec![first, next, last], 7, Some(&target), received_at, |_| {});
        let frame = captcha_test_frame([137; 3], 1, 1_300_000_000);
        let mut rejected = Vec::new();
        assert_eq!(pending_captcha_input_candidate(&mut pending, &frame, received_at,
            |press, reason| rejected.push((press, reason))), CaptchaInputCandidate::Ready(next));
        assert_eq!(rejected, vec![(first, "late_capture")]);
        assert_eq!(pending.len(), 2);

        let mut profile = Profile::default();
        profile.features.nol_captcha_detection = true;
        let mut engine = Engine::new(profile).unwrap();
        engine.step(Event::Start { mode: RunMode::Normal }, 0);
        let mut detector = NolCaptchaDetector::default();
        let observation = detector.arm_on_input(captcha_test_view(&frame), frame.sequence,
            frame.captured_at_ns, frame.last_observed_ns, 1).unwrap();
        assert!(!observation.dismissed);
        assert_eq!(engine.snapshot().phase, Phase::Captcha);
        assert!(engine.step(Event::Tick, 2).is_empty());
        assert!(!captcha_input_watch_needed(&engine, &detector, false, false));
        // The watch is deactivated once a reference exists: remaining letters
        // must neither replace the first usable reference nor complete entry.
        pending.clear();
        let mut typed = frame.clone();
        typed.rgba[(50 * 100 + 50) * 4..(50 * 100 + 50) * 4 + 3].fill(255);
        assert!(!detector.observe(captcha_test_view(&typed), 2, 1_310_000_000,
            1_310_000_000, 10).unwrap().dismissed);
        assert_eq!(engine.snapshot().phase, Phase::Captcha);
        let closed = captcha_test_frame([235; 3], 3, 1_400_000_000);
        assert!(!detector.observe(captcha_test_view(&closed), 3, closed.captured_at_ns,
            closed.last_observed_ns, 100).unwrap().dismissed);
        assert!(detector.observe(captcha_test_view(&closed), 3, closed.captured_at_ns,
            1_500_000_000, 200).unwrap().dismissed);
        engine.step(Event::NolCaptchaDismissed, 200);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
    }

    #[test]
    fn captcha_input_batch_waits_in_order_and_discards_only_expired_candidates() {
        let first = macro_platform::CaptchaInput { token: 7, window_id: 44, pid: 55, observed_at_ns: 1_000_000_000 };
        let next = macro_platform::CaptchaInput { observed_at_ns: 1_200_000_000, ..first };
        let received_at = Instant::now();
        let mut pending = VecDeque::from([
            PendingCaptchaInput { press: first, received_at },
            PendingCaptchaInput { press: next, received_at },
        ]);
        let mut frame = captcha_test_frame([137; 3], 1, first.observed_at_ns - 1);
        assert_eq!(pending_captcha_input_candidate(&mut pending, &frame, received_at,
            |_, reason| panic!("unexpected rejection: {reason}")), CaptchaInputCandidate::WaitingForFrame);
        assert_eq!(pending.len(), 2);
        frame.last_observed_ns = first.observed_at_ns;
        assert_eq!(pending_captcha_input_candidate(&mut pending, &frame, received_at,
            |_, reason| panic!("unexpected rejection: {reason}")), CaptchaInputCandidate::Ready(first));
        let mut rejected = Vec::new();
        assert_eq!(pending_captcha_input_candidate(&mut pending, &frame,
            received_at + Duration::from_nanos(CAPTCHA_INPUT_FRAME_WINDOW_NS + 1),
            |press, reason| rejected.push((press, reason))), CaptchaInputCandidate::Empty);
        assert_eq!(rejected, vec![(first, "capture_wait_expired"), (next, "capture_wait_expired")]);
        assert!(pending.is_empty());
        // A later, newly received batch can arm; expired signals are not replayed.
        pending.push_back(PendingCaptchaInput { press: next, received_at: received_at + Duration::from_millis(300) });
        frame.last_observed_ns = next.observed_at_ns;
        assert_eq!(pending_captcha_input_candidate(&mut pending, &frame, received_at + Duration::from_millis(300),
            |_, reason| panic!("unexpected rejection: {reason}")), CaptchaInputCandidate::Ready(next));
    }

    #[test]
    fn captcha_input_batch_drops_invalid_and_late_signals_before_waiting_for_new_input() {
        let received_at = Instant::now();
        let first = macro_platform::CaptchaInput { token: 7, window_id: 44, pid: 55, observed_at_ns: 0 };
        let next = macro_platform::CaptchaInput { observed_at_ns: 1_000_000_000, ..first };
        let mut pending = VecDeque::from([
            PendingCaptchaInput { press: first, received_at },
            PendingCaptchaInput { press: next, received_at },
        ]);
        let frame = captcha_test_frame([137; 3], 1, 1_300_000_000);
        let mut rejected = Vec::new();
        assert_eq!(pending_captcha_input_candidate(&mut pending, &frame, received_at,
            |press, reason| rejected.push((press, reason))), CaptchaInputCandidate::Empty);
        assert_eq!(rejected, vec![(first, "invalid_capture_time"), (next, "late_capture")]);
        assert!(pending.is_empty());
    }

    #[test]
    fn captcha_input_watch_never_arms_during_entry_pause_or_other_modes() {
        let mut profile = Profile::default();
        profile.features.nol_captcha_detection = true;
        profile.legacy.next = Some(Target::Point { point: Point { x: 10.0, y: 20.0 } });
        let mut engine = Engine::new(profile.clone()).unwrap();
        let detector = NolCaptchaDetector::default();
        assert!(!captcha_input_watch_needed(&engine, &detector, false, false));
        let actions = engine.step(Event::Start { mode: RunMode::Normal }, 0);
        assert!(!captcha_input_watch_needed(&engine, &detector, false, false));
        let click = actions.iter().find(|action| matches!(action.kind, ActionKind::Click { .. })).unwrap();
        engine.step(Event::ActionAcknowledged { id: click.id, success: true, detail: String::new() }, 1);
        engine.step(Event::Tick, 1000);
        assert!(captcha_input_watch_needed(&engine, &detector, false, false));
        engine.step(Event::Pause, 1001);
        assert!(!captcha_input_watch_needed(&engine, &detector, false, false));
        engine.step(Event::Resume, 1002);
        assert!(captcha_input_watch_needed(&engine, &detector, false, false));
        engine.step(Event::Stop, 1003);
        assert!(!captcha_input_watch_needed(&engine, &detector, false, false));
        for mode in [RunMode::Seats, RunMode::Cancellation] {
            let mut engine = Engine::new(profile.clone()).unwrap();
            engine.step(Event::Start { mode }, 0);
            assert!(!captcha_input_watch_needed(&engine, &detector, false, false));
        }
    }

    #[test]
    fn manual_captcha_preflight_accepts_wait_or_pause_without_changing_the_run() {
        let mut profile = Profile::default();
        profile.features.nol_captcha_detection = true;
        let mut engine = Engine::new(profile).unwrap();
        engine.step(Event::Start { mode: RunMode::Normal }, 10);
        assert_eq!(engine.snapshot().phase, Phase::Captcha);
        assert_eq!(manual_captcha_resume_required(&engine, false), Ok(false));
        assert!(manual_captcha_resume_required(&engine, true).is_err());
        engine.step(Event::Pause, 20);
        assert_eq!(manual_captcha_resume_required(&engine, false), Ok(true));
        assert_eq!(engine.snapshot().phase, Phase::Paused);
        assert_eq!(engine.confirmation_phase(), Phase::Captcha);
        assert!(engine.can_manually_complete_captcha());
        engine.step(Event::Resume, 30);
        engine.step(Event::CaptchaManuallyCompleted, 30);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        assert_eq!(manual_captcha_resume_required(&engine, false), Ok(false));
    }

    #[test]
    fn manual_captcha_preflight_accepts_idle_undelivered_entry_stop_and_search_modes() {
        let mut profile = Profile::default();
        profile.features.nol_captcha_detection = true;
        profile.legacy.date = Some(Target::Point { point: Point { x: 10.0, y: 20.0 } });
        let mut engine = Engine::new(profile.clone()).unwrap();
        assert_eq!(manual_captcha_resume_required(&engine, false), Ok(engine.snapshot().phase == Phase::Paused));
        engine.step(Event::Start { mode: RunMode::Normal }, 10);
        assert_eq!(engine.snapshot().phase, Phase::Entry);
        engine.step(Event::Pause, 20);
        assert_eq!(manual_captcha_resume_required(&engine, false), Ok(engine.snapshot().phase == Phase::Paused));
        assert_eq!(engine.snapshot().phase, Phase::Paused);
        engine.step(Event::Stop, 30);
        assert_eq!(manual_captcha_resume_required(&engine, false), Ok(engine.snapshot().phase == Phase::Paused));
        for mode in [RunMode::Seats, RunMode::Cancellation] {
            let mut engine = Engine::new(profile.clone()).unwrap();
            engine.step(Event::Start { mode }, 10);
            engine.step(Event::Pause, 20);
            assert_eq!(manual_captcha_resume_required(&engine, false), Ok(true));
            assert_eq!(engine.snapshot().phase, Phase::Paused);
        }
    }

    #[test]
    fn queued_manual_captcha_request_is_cancelled_by_later_controls() {
        for control in [Command::Stop, Command::Pause, Command::CaptureArmed,
            Command::Bind(None), Command::StartFromUi { mode: RunMode::Normal, profile: json!({}), expected_profile_revision: 0 }] {
            let (tx, rx) = mpsc::channel();
            let handle = RuntimeHandle { tx, interrupt: Arc::new(AtomicBool::new(false)),
                shortcut_epoch: Arc::new(AtomicU64::new(0)) };
            handle.send(Command::ContinueAfterCaptcha);
            let request = rx.try_recv().unwrap();
            assert!(manual_captcha_request_current(request.shortcut_epoch,
                handle.shortcut_epoch.load(Ordering::SeqCst), false));
            handle.send(control);
            assert!(!manual_captcha_request_current(request.shortcut_epoch,
                handle.shortcut_epoch.load(Ordering::SeqCst), handle.interrupt.load(Ordering::SeqCst)));
        }
        assert!(!manual_captcha_request_current(3, 3, true));
    }

    #[test]
    fn latest_f9_during_startup_or_before_run_initialization_is_an_explicit_handoff() {
        for mode in [RunMode::Normal, RunMode::Seats, RunMode::Cancellation] {
            let (tx, rx) = mpsc::channel();
            let handle = RuntimeHandle { tx, interrupt: Arc::new(AtomicBool::new(false)),
                shortcut_epoch: Arc::new(AtomicU64::new(0)) };
            handle.send(Command::PrepareShortcut(mode));
            let old = rx.try_recv().unwrap();
            handle.send(Command::ContinueAfterCaptcha);
            let request = rx.try_recv().unwrap();
            // A later native initialization timestamp is not an ownership test.
            let initialized_at = request.received_at + Duration::from_nanos(1);
            assert!(request.received_at < initialized_at);
            assert!(manual_captcha_request_current(request.shortcut_epoch,
                handle.shortcut_epoch.load(Ordering::SeqCst), false));
            assert!(!control_request_current(&old, handle.shortcut_epoch.load(Ordering::SeqCst)));
        }
    }

    #[test]
    fn start_shortcut_accepts_one_current_response_and_preserves_requested_mode() {
        let mut gate = StartShortcutGate::default();
        let id = gate.prepare(RunMode::Normal, 10, 4).unwrap();
        assert!(gate.prepare(RunMode::Normal, 20, 4).is_none());
        assert!(gate.prepare(RunMode::Seats, 20, 4).is_none());
        assert!(gate.prepare(RunMode::Cancellation, 20, 4).is_none());
        assert!(gate.take(id + 1, 30, 4).is_none());
        let request = gate.take(id, 40, 4).unwrap();
        assert_eq!((request.mode, request.epoch), (RunMode::Normal, 4));
        assert!(gate.take(id, 41, 4).is_none());
        let next = gate.prepare(RunMode::Seats, 50, 4).unwrap();
        assert_ne!(next, id);
        assert!(gate.take(id, 51, 4).is_none());
        let request = gate.take(next, 52, 4).unwrap();
        assert_eq!((request.mode, request.epoch), (RunMode::Seats, 4));
        let cancellation = gate.prepare(RunMode::Cancellation, 60, 4).unwrap();
        assert!(gate.take(next, 61, 4).is_none());
        let request = gate.take(cancellation, 62, 4).unwrap();
        assert_eq!((request.mode, request.epoch), (RunMode::Cancellation, 4));
    }

    #[test]
    fn start_shortcut_expiry_is_final_and_reports_the_timed_out_mode_once() {
        let mut gate = StartShortcutGate::default();
        for mode in [RunMode::Normal, RunMode::Seats, RunMode::Cancellation] {
            let id = gate.prepare(mode, 100, 2).unwrap();
            assert!(gate.expire(100 + START_SHORTCUT_TIMEOUT_MS - 1, 2).is_none());
            assert_eq!(gate.expire(100 + START_SHORTCUT_TIMEOUT_MS, 2), Some(mode));
            assert!(gate.expire(100 + START_SHORTCUT_TIMEOUT_MS + 1, 2).is_none());
            assert!(gate.take(id, 100 + START_SHORTCUT_TIMEOUT_MS + 1, 2).is_none());
            let next = gate.prepare(mode, 3000, 2).unwrap();
            assert!(gate.take(next, 3000 + START_SHORTCUT_TIMEOUT_MS, 2).is_none());
        }
    }

    #[test]
    fn controls_invalidate_start_shortcut_before_the_worker_receives_them() {
        let (tx, rx) = mpsc::channel();
        let handle = RuntimeHandle {
            tx, interrupt: Arc::new(AtomicBool::new(false)),
            shortcut_epoch: Arc::new(AtomicU64::new(0)),
        };
        let mut gate = StartShortcutGate::default();
        for mode in [RunMode::Normal, RunMode::Seats, RunMode::Cancellation] {
            for command in [
                Command::Stop, Command::Pause, Command::TogglePause,
                Command::CaptureArmed, Command::ArmCapture { field: "color".into(), profile: None, expected_profile_revision: 0 },
                Command::Bind(None), Command::StartFromUi { mode: RunMode::Cancellation, profile: json!({}), expected_profile_revision: 0 },
                Command::StartFromUi { mode: RunMode::Seats, profile: json!({}), expected_profile_revision: 0 }, Command::ResumeFromUi, Command::Shutdown,
            ] {
                let epoch = handle.shortcut_epoch.load(Ordering::SeqCst);
                let id = gate.prepare(mode, 0, epoch).unwrap();
                handle.send(command);
                // No worker has dequeued the control yet, but a late UI response
                // already cannot obtain authorization to save/start.
                assert!(gate.take(id, 1, handle.shortcut_epoch.load(Ordering::SeqCst)).is_none());
                assert!(rx.try_recv().unwrap().command.cancels_start_shortcut());
            }
        }
    }

    #[test]
    fn stop_after_accepting_ui_response_invalidates_in_progress_start_epoch() {
        let (tx, rx) = mpsc::channel();
        let handle = RuntimeHandle {
            tx, interrupt: Arc::new(AtomicBool::new(false)),
            shortcut_epoch: Arc::new(AtomicU64::new(0)),
        };
        let mut gate = StartShortcutGate::default();
        let id = gate.prepare(RunMode::Seats, 0, 0).unwrap();
        let accepted_epoch = gate.take(id, 1, 0).unwrap().epoch;
        // Simulate F4 arriving while storage or native startup capture blocks.
        handle.send(Command::Stop);
        assert_ne!(accepted_epoch, handle.shortcut_epoch.load(Ordering::SeqCst));
        assert!(handle.interrupt.load(Ordering::SeqCst));
        assert!(matches!(rx.try_recv().unwrap().command, Command::Stop));
        assert!(gate.take(id, 2, handle.shortcut_epoch.load(Ordering::SeqCst)).is_none());
    }

    #[test]
    fn unseen_map_can_be_calibrated_later_without_future_coordinates() {
        let raw = serde_json::to_value(Profile::default()).unwrap();
        let reference = FrameReference {width:1000,height:800,scale_x:2.0,scale_y:2.0};
        let raw = apply_capture(&raw,"vision.roi.start",Point{x:500.0,y:600.0},Color::default(),&reference).unwrap();
        assert!(raw["vision"]["roi"].is_null());
        let raw = apply_capture(&raw,"vision.roi.end",Point{x:100.0,y:200.0},Color::default(),&reference).unwrap();
        assert_eq!(raw["vision"]["roi"],json!({"x":100.0,"y":200.0,"width":400.0,"height":400.0}));
        assert!(Profile::from_json(&raw.to_string()).is_ok());
    }

    #[test]
    fn region_capture_updates_region_color_without_changing_seat_color() {
        let raw = serde_json::to_value(Profile::default()).unwrap();
        let reference = FrameReference {width:1000,height:800,scale_x:1.0,scale_y:1.0};
        let next = apply_capture(&raw,"desktop.region_color",Point::default(),Color{r:1,g:2,b:3},&reference).unwrap();
        assert_eq!(next["color"],raw["color"]);
        assert_eq!(next["region_color"],json!({"r":1,"g":2,"b":3}));
    }

    #[test]
    fn color_capture_on_new_scale_invalidates_old_coordinates() {
        let mut raw = serde_json::to_value(Profile::default()).unwrap();
        raw["legacy"]["date"] = json!({"type":"point","point":{"x":500.0,"y":300.0}});
        raw["vision"]["roi"] = json!({"x":0.0,"y":0.0,"width":900.0,"height":600.0});
        raw["desktop"] = json!({"capture_reference":{"width":2000,"height":1600,"scale_x":2.0,"scale_y":2.0}});
        let reference = FrameReference {width:1000,height:800,scale_x:1.0,scale_y:1.0};
        let next = apply_capture(&raw,"color",Point::default(),Color{r:1,g:2,b:3},&reference).unwrap();
        assert!(next["legacy"]["date"].is_null());
        assert!(next["vision"]["roi"].is_null());
        assert_eq!(next["color"],json!({"r":1,"g":2,"b":3}));
    }

    #[test]
    fn region_capture_does_not_replace_arrow_key_navigation() {
        let mut raw = serde_json::to_value(Profile::default()).unwrap();
        raw["legacy"]["cancellation_navigation"] = json!({"type":"arrow_keys"});
        raw["desktop"] = json!({"cancellation_mode":"legacyKeys"});
        let reference = FrameReference {width:1000,height:800,scale_x:1.0,scale_y:1.0};
        let raw = apply_capture(&raw,"legacy.region_first",Point{x:10.0,y:20.0},Color::default(),&reference).unwrap();
        let raw = apply_capture(&raw,"legacy.region_second",Point{x:30.0,y:40.0},Color::default(),&reference).unwrap();
        assert_eq!(raw["legacy"]["cancellation_navigation"]["type"],"arrow_keys");
    }

    #[test]
    fn per_region_roi_capture_preserves_global_settings() {
        let mut raw = serde_json::to_value(Profile::default()).unwrap();
        raw["vision"]["exclusions"] = json!([{"x":1.0,"y":1.0,"width":10.0,"height":10.0}]);
        let reference = FrameReference {width:1000,height:800,scale_x:1.0,scale_y:1.0};
        let next = apply_capture(&raw,"legacy.regions.1.vision.roi.start",Point{x:100.0,y:100.0},Color::default(),&reference).unwrap();
        let next = apply_capture(&next,"legacy.regions.1.vision.roi.end",Point{x:500.0,y:400.0},Color::default(),&reference).unwrap();
        assert_eq!(next["vision"],raw["vision"]);
        assert!(next["legacy"]["regions"][0]["vision"].is_null());
        assert_eq!(next["legacy"]["regions"][1]["vision"]["exclusions"],raw["vision"]["exclusions"]);
        assert!(Profile::from_json(&next.to_string()).is_ok());
    }

    #[test]
    fn scheduled_entry_requires_an_actual_button_but_f2_keeps_original_navigation() {
        let mut profile = Profile::default();
        let desktop = DesktopConfig { scheduled_start: true, ..DesktopConfig::default() };
        assert!(validate_entry_setup(&profile, &desktop, RunMode::Normal).unwrap_err().contains("판매 시작 예매 버튼"));
        assert!(validate_entry_setup(&profile, &desktop, RunMode::Cancellation).is_ok());
        profile.entry_target = Some(Target::Point {point:Point{x:40.0,y:50.0}});
        assert!(validate_entry_setup(&profile, &desktop, RunMode::Normal).is_ok());
        profile.features.auto_entry = true;
        profile.entry_target = None;
        assert!(validate_entry_setup(&profile, &desktop, RunMode::Cancellation).is_ok());
    }

    #[test]
    fn nol_gate_allows_unspecified_manual_entry_but_never_skips_a_scheduled_button() {
        let mut profile = Profile::default();
        profile.features.auto_entry = true;
        profile.features.nol_captcha_detection = true;
        let mut desktop = DesktopConfig::default();
        assert!(validate_entry_setup(&profile, &desktop, RunMode::Normal).is_ok());
        desktop.scheduled_start = true;
        assert!(validate_entry_setup(&profile, &desktop, RunMode::Normal).unwrap_err().contains("판매 시작 예매 버튼"));
        assert!(validate_entry_setup(&profile, &desktop, RunMode::Cancellation).is_ok());
    }

    #[test]
    fn idle_capture_metadata_advances_observation_without_inventing_pixels_or_sequence() {
        let mut cached = Some(OwnedFrame {
            width: 1, height: 1, stride: 4, rgba: vec![10, 20, 30, 255],
            origin: macro_platform::Point { x: 0.0, y: 0.0 }, scale_x: 2.0, scale_y: 2.0,
            sequence: 7, captured_at_ns: 100, last_observed_ns: 120,
        });
        let pixels = cached.as_ref().unwrap().rgba.as_ptr();
        apply_frame_observation(&mut cached, FrameObservation::Unchanged {
            sequence: 7, captured_at_ns: 100, last_observed_ns: 240,
        }).unwrap();
        let frame = cached.as_ref().unwrap();
        assert_eq!(frame.rgba.as_ptr(), pixels);
        assert_eq!((frame.sequence, frame.captured_at_ns, frame.last_observed_ns), (7, 100, 240));
        for (sequence, captured_at_ns, last_observed_ns) in [(8, 100, 300), (7, 101, 300), (7, 100, 200)] {
            assert!(apply_frame_observation(&mut cached, FrameObservation::Unchanged {
                sequence, captured_at_ns, last_observed_ns,
            }).is_err());
        }
        assert!(apply_frame_observation(&mut None, FrameObservation::Unchanged {
            sequence: 7, captured_at_ns: 100, last_observed_ns: 300,
        }).is_err());
        assert_eq!(cached.as_ref().unwrap().last_observed_ns, 240);
    }

    #[test]
    fn entry_button_capture_survives_save_and_geometry_change_clears_it() {
        let raw = serde_json::to_value(Profile::default()).unwrap();
        let reference = FrameReference {width:1000,height:800,scale_x:2.0,scale_y:2.0};
        let point = Point {x:240.0,y:160.0};
        let raw = apply_capture(&raw,"entry_target",point,Color::default(),&reference).unwrap();
        let profile = Profile::from_json(&raw.to_string()).unwrap();
        assert_eq!(profile.entry_target,Some(Target::Point{point}));
        let changed = FrameReference {scale_x:1.0,scale_y:1.0,..reference};
        let raw = apply_capture(&raw,"color",Point::default(),Color::default(),&changed).unwrap();
        assert!(Profile::from_json(&raw.to_string()).unwrap().entry_target.is_none());
    }

    #[test]
    fn capture_labels_reject_unknown_fields_before_arming() {
        assert_eq!(capture_label("entry_target").unwrap(),"판매 시작 예매 버튼");
        assert_eq!(capture_label("legacy.regions.1.vision.roi.end").unwrap(),"2구역 범위 끝");
        assert!(capture_label("legacy.regions.2.color").is_err());
        assert!(capture_label("desktop.anything").is_err());
    }

    #[test]
    fn stored_entry_coordinates_do_not_enable_a_disabled_feature() {
        let mut profile = Profile::default();
        profile.entry_target = Some(Target::Point {point:Point{x:40.0,y:50.0}});
        let mut desktop = DesktopConfig::default();
        assert!(!entry_click_enabled(&profile,&desktop,RunMode::Normal));
        desktop.scheduled_start = true;
        assert!(entry_click_enabled(&profile,&desktop,RunMode::Normal));
        assert!(!entry_click_enabled(&profile,&desktop,RunMode::Cancellation));
        desktop.scheduled_start = false;
        profile.features.auto_entry = true;
        assert!(entry_click_enabled(&profile,&desktop,RunMode::Normal));
    }

    #[test]
    fn cancellation_execution_options_isolate_refresh_and_general_region_settings() {
        let point = |x| Target::Point { point: Point { x, y: 5.0 } };
        let desktop = DesktopConfig { refresh_mode: "internalButton".into(), ..DesktopConfig::default() };
        for navigation in [macro_core::CancellationNavigation::ArrowKeys,
            macro_core::CancellationNavigation::TwoRegions { first: point(10.0), second: point(20.0) }] {
            let mut saved = Profile::default();
            saved.features.automatic_refresh = true;
            saved.features.region_priority = true;
            saved.refresh_target = Some(point(30.0));
            saved.legacy.region_delay_ms = 10;
            saved.legacy.cancellation_navigation = navigation;
            let mut effective = saved.clone();
            apply_execution_options(&mut effective, &desktop, RunMode::Cancellation);
            assert!(!effective.features.automatic_refresh);
            assert!(!effective.features.region_priority);
            assert!(effective.refresh_target.is_none());
            assert!(saved.features.automatic_refresh && saved.features.region_priority);
            assert!(saved.refresh_target.is_some());
            let mut engine = Engine::new(effective).unwrap();
            let first = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
            let navigation = first.iter().find(|action| matches!(&action.kind,
                ActionKind::Click { purpose, .. } | ActionKind::Key { purpose, .. } if purpose == "legacy_cancellation_region")).unwrap();
            engine.step(Event::ActionAcknowledged { id: navigation.id, success: true, detail: String::new() }, 1);
            engine.step(Event::Tick, 11);
            let state = engine.snapshot();
            assert_eq!(state.phase, Phase::Seats);
            // A stale saved ON flag must not suppress the first static legacy scan.
            assert!(!first_legacy_scan_requested(&saved, state.generation, None));
            assert!(first_legacy_scan_requested(engine.profile(), state.generation, None));
            assert!(can_rescan_seat_frame(&state, "seats", true));
            engine.step(Event::CandidatesObserved { generation: state.generation, candidates: vec![] }, 12);
            let next = engine.step(Event::Tick, 12);
            assert!(next.iter().any(|action| matches!(&action.kind,
                ActionKind::Click { purpose, .. } | ActionKind::Key { purpose, .. } if purpose == "legacy_cancellation_region")));
        }
        let mut profile = Profile::default();
        profile.features.automatic_refresh = true;
        profile.features.region_priority = true;
        profile.refresh_target = Some(point(30.0));
        let saved = profile.clone();
        apply_execution_options(&mut profile, &desktop, RunMode::Cancellation);
        assert!(profile.features.automatic_refresh);
        assert_eq!(profile.refresh_target, saved.refresh_target);
        assert!(!profile.features.region_priority);
        for mode in [RunMode::Normal, RunMode::Seats] {
            let mut profile = saved.clone();
            apply_execution_options(&mut profile, &desktop, mode);
            assert!(!profile.features.automatic_refresh);
            assert!(profile.refresh_target.is_none());
            assert!(profile.features.region_priority); // general booking preference is preserved
        }
    }

    #[test]
    fn cancellation_without_an_authorized_button_scans_empty_static_maps_without_refresh() {
        for (refresh_mode, has_target) in [("none", true), ("internalButton", false), ("site", true)] {
            let mut profile = Profile::default();
            profile.features.automatic_refresh = true;
            profile.refresh_interval_ms = 10;
            profile.refresh_target = has_target.then_some(Target::Point { point: Point { x: 3.0, y: 3.0 } });
            let desktop = DesktopConfig { refresh_mode: refresh_mode.into(), ..DesktopConfig::default() };
            assert_eq!(requested_refresh_button_missing(&profile, &desktop, RunMode::Cancellation),
                refresh_mode == "internalButton" && !has_target);
            assert!(!requested_refresh_button_missing(&profile, &desktop, RunMode::Normal));
            apply_execution_options(&mut profile, &desktop, RunMode::Cancellation);
            assert!(!profile.features.automatic_refresh);
            let mut engine = Engine::new(profile).unwrap();
            engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
            for now in 1..=20 {
                let state = engine.snapshot();
                engine.step(Event::CandidatesObserved { generation: state.generation, candidates: vec![] }, now * 10);
                let actions = engine.step(Event::Tick, now * 10);
                assert!(actions.iter().all(|action| matches!(action.kind, ActionKind::Observe { .. })));
                assert_eq!(engine.snapshot().phase, Phase::Seats);
                assert!(can_rescan_seat_frame(&engine.snapshot(), "seats", true));
            }
        }
    }

    #[test]
    fn explicit_two_region_cancellation_requires_both_buttons_only_when_starting_that_mode() {
        let mut profile = Profile::default();
        let desktop = DesktopConfig { cancellation_mode: "legacyRegions".into(), ..DesktopConfig::default() };
        for navigation in [macro_core::CancellationNavigation::None, macro_core::CancellationNavigation::ArrowKeys] {
            profile.legacy.cancellation_navigation = navigation;
            let error = validate_entry_setup(&profile, &desktop, RunMode::Cancellation).unwrap_err();
            assert!(error.contains("원본 · 첫 번째 구역"));
            assert!(error.contains("원본 · 두 번째 구역"));
            assert!(validate_entry_setup(&profile, &desktop, RunMode::Normal).is_ok());
            assert!(Profile::from_json(&serde_json::to_string(&profile).unwrap()).is_ok()); // partial setup remains saveable
        }
        profile.legacy.cancellation_navigation = macro_core::CancellationNavigation::TwoRegions {
            first: Target::Point { point: Point { x: 1.0, y: 1.0 } }, second: Target::Point { point: Point { x: 2.0, y: 2.0 } },
        };
        assert!(validate_entry_setup(&profile, &desktop, RunMode::Cancellation).is_ok());
        for invalid in [Target::Element { id: "unsupported-region-element".into() },
            Target::Point { point: Point { x: -1.0, y: 2.0 } }] {
            profile.legacy.cancellation_navigation = macro_core::CancellationNavigation::TwoRegions {
                first: invalid, second: Target::Point { point: Point { x: 2.0, y: 2.0 } },
            };
            assert!(validate_entry_setup(&profile, &desktop, RunMode::Cancellation).is_err());
        }
        profile.legacy.cancellation_navigation = macro_core::CancellationNavigation::None;
        assert!(validate_entry_setup(&profile, &DesktopConfig::default(), RunMode::Cancellation).is_ok());
    }

    #[test]
    fn saved_refresh_button_spellings_work_only_for_opted_in_cancellation() {
        let mut profile = Profile::default();
        profile.features.automatic_refresh = true;
        profile.refresh_target = Some(Target::Point { point: Point { x: 25.0, y: 40.0 } });
        for spelling in ["button", "internalButton"] {
            let desktop: DesktopConfig = serde_json::from_value(json!({"refresh_mode": spelling})).unwrap();
            assert_eq!(refresh_button_target(&profile, &desktop, RunMode::Cancellation), profile.refresh_target);
            assert!(refresh_button_target(&profile, &desktop, RunMode::Normal).is_none());
            assert!(refresh_button_target(&profile, &desktop, RunMode::Seats).is_none());
            profile.features.automatic_refresh = false;
            assert!(refresh_button_target(&profile, &desktop, RunMode::Cancellation).is_none());
            profile.features.automatic_refresh = true;
        }
        for spelling in ["", "none", "site", "f5"] {
            let desktop: DesktopConfig = serde_json::from_value(json!({"refresh_mode": spelling})).unwrap();
            assert!(refresh_button_target(&profile, &desktop, RunMode::Cancellation).is_none());
        }
        let desktop = DesktopConfig { refresh_mode: "button".into(), ..DesktopConfig::default() };
        profile.refresh_target = None;
        assert!(refresh_button_target(&profile, &desktop, RunMode::Cancellation).is_none());
    }

    #[test]
    fn direct_seat_setup_ignores_unused_entry_and_schedule_but_requires_seat_area() {
        let mut profile = Profile::default();
        profile.features.auto_entry = true;
        profile.features.server_clock = true;
        profile.features.region_priority = true;
        let desktop = DesktopConfig { scheduled_start: true, ..DesktopConfig::default() };
        assert!(validate_entry_setup(&profile, &desktop, RunMode::Seats).unwrap_err().contains("좌석 영역"));
        profile.vision.roi = Some(Rect { x: 20.0, y: 30.0, width: 400.0, height: 300.0 });
        assert!(validate_entry_setup(&profile, &desktop, RunMode::Seats).is_ok());
        assert!(!entry_click_enabled(&profile, &desktop, RunMode::Seats));
        assert!(validate_entry_setup(&profile, &desktop, RunMode::Normal).is_err());
    }
}

#[cfg(test)]
mod seat_poll_tests {
    use super::*;

    #[test]
    fn optimistic_conflict_uses_delivered_attempt_not_queued_next_seat() {
        let mut profile = Profile::default();
        profile.target_seats = 2;
        profile.features.conflict_recovery = true;
        let color = profile.color;
        let mut engine = Engine::new(profile).unwrap();
        engine.step(Event::Start { mode: RunMode::Seats }, 0);
        let candidates = ["delivered", "queued"].iter().enumerate().map(|(index, id)| {
            let point = Point { x: 10.0, y: 10.0 + index as f64 * 20.0 };
            Candidate { id: (*id).into(), point, color, target: Target::Point { point }, selected: false, geometry: None }
        }).collect();
        let actions = engine.step(Event::CandidatesObserved { generation: 0, candidates }, 1);
        let click = actions.iter().find(|action| matches!(&action.kind, ActionKind::Click { purpose, .. }
            if purpose.starts_with("seat:"))).unwrap();
        // No input has been delivered, so neither a default nor explicit failure can count down.
        assert!(conflict_confirmation_event(&engine.snapshot(), None).is_err());
        engine.step(Event::ActionAcknowledged { id: click.id, success: true, detail: String::new() }, 2);
        let state = engine.snapshot();
        assert_eq!(state.current_candidate_id.as_deref(), Some("queued"));
        assert!(manual_confirmation_kind(&state, Phase::Seats, "seat_result").is_none());
        let event = conflict_confirmation_event(&state, None).unwrap();
        assert!(matches!(&event, Event::DeliveredSeatResult { action_id, candidate_id, result: SeatOutcome::Conflict }
            if *action_id == click.id && candidate_id == "delivered"));
        let queued = vec!["queued".to_string()];
        assert!(conflict_confirmation_event(&state, Some(&queued)).is_err());
        engine.step(event, 3);
        assert!(engine.snapshot().attempted_ids.is_empty());
        assert_eq!(engine.snapshot().rejected_ids, vec!["delivered"]);
        assert!(conflict_confirmation_event(&engine.snapshot(), None).is_err());
    }

    #[test]
    fn manual_failure_remains_available_during_context_hold_not_manual_pause() {
        let mut engine = Engine::new(Profile::default()).unwrap();
        engine.step(Event::Start { mode: RunMode::Seats }, 0);
        engine.step(Event::ContextLost { reason: "settings window in front".into() }, 1);
        let state = engine.snapshot();
        for phase in [Phase::Seats, Phase::Completing, Phase::Conflict] {
            assert_eq!(context_confirmation_phase(&state, phase, true), phase);
            assert_eq!(context_confirmation_phase(&state, phase, false), Phase::Paused);
        }
        assert_eq!(context_confirmation_phase(&state, Phase::Entry, true), Phase::Paused);
        assert_eq!(manual_confirmation_kind(&state, Phase::Seats, "seat_result"), None);
    }

    #[test]
    fn cancelled_native_post_is_not_a_failed_or_successful_seat_ack() {
        let profile = Profile::default();
        let candidate = Candidate { id: "unsent".into(), point: Point { x: 3.0, y: 3.0 },
            target: Target::Point { point: Point { x: 3.0, y: 3.0 } }, color: profile.color, selected: false, geometry: None };
        let mut engine = Engine::new(profile).unwrap();
        engine.step(Event::Start { mode: RunMode::Seats }, 0);
        let actions = engine.step(Event::CandidatesObserved { generation: 0, candidates: vec![candidate.clone()] }, 1);
        let click = actions.iter().find(|action| matches!(&action.kind, ActionKind::Click { .. })).unwrap();
        assert!(input_cancelled_before_delivery("[input_cancelled] stop during validation"));
        assert!(!input_cancelled_before_delivery("[input_rejected] delivery unknown"));
        engine.step(Event::ActionCancelled { id: click.id }, 2);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        assert!(engine.snapshot().attempted_ids.is_empty());
        assert!(engine.snapshot().rejected_ids.is_empty());
        engine.step(Event::Pause, 3);
        engine.step(Event::Resume, 4);
        let retry = engine.step(Event::CandidatesObserved { generation: 0, candidates: vec![candidate] }, 5);
        assert!(retry.iter().any(|action| matches!(&action.kind, ActionKind::Click { purpose, .. } if purpose == "seat:unsent")));
    }

    #[test]
    fn stale_full_profile_cannot_replace_a_new_capture_revision() {
        let original = serde_json::to_value(Profile::default()).unwrap();
        let mut authoritative = apply_capture(&original, "vision.roi.start", Point { x: 10.0, y: 20.0 },
            Color { r: 100, g: 100, b: 100 }, &FrameReference { width: 100, height: 100, scale_x: 1.0, scale_y: 1.0 }).unwrap();
        let captured = authoritative.clone();
        let current_revision = 1;
        let stale_save = check_profile_revision(0, current_revision).map(|_| authoritative = original);
        assert!(stale_save.unwrap_err().contains("[profile_revision_conflict]"));
        assert_eq!(authoritative, captured);
        assert!(check_profile_revision(current_revision, current_revision).is_ok());
        assert!(check_profile_revision(2, current_revision).is_err());
    }

    #[test]
    fn ui_start_keeps_its_ingress_epoch_when_stop_arrives_during_save() {
        let (tx, _rx) = mpsc::channel();
        let handle = RuntimeHandle { tx, interrupt: Arc::new(AtomicBool::new(false)), shortcut_epoch: Arc::new(AtomicU64::new(0)) };
        let command = Command::StartFromUi { mode: RunMode::Normal,
            profile: serde_json::to_value(Profile::default()).unwrap(), expected_profile_revision: 0 };
        let received_epoch = handle.receive_command(&command);
        assert_eq!(received_epoch, handle.shortcut_epoch.load(Ordering::SeqCst));
        handle.send(Command::Stop);
        assert_ne!(received_epoch, handle.shortcut_epoch.load(Ordering::SeqCst));
        assert!(handle.interrupt.load(Ordering::SeqCst));
        // A single StartFromUi IPC has no second Start request after Save to
        // acquire this newer epoch and accidentally restart the stopped run.
        assert_eq!(received_epoch, 1);
    }

    #[test]
    fn ui_enqueue_captures_epoch_before_a_delayed_blocking_reply_wait() {
        let (tx, rx) = mpsc::channel();
        let handle = RuntimeHandle { tx, interrupt: Arc::new(AtomicBool::new(false)), shortcut_epoch: Arc::new(AtomicU64::new(0)) };
        let pending = handle.enqueue(Command::StartFromUi { mode: RunMode::Normal,
            profile: serde_json::to_value(Profile::default()).unwrap(), expected_profile_revision: 0 }).unwrap();
        // The blocking-pool reply waiter has not run yet. F4 still invalidates
        // the already-enqueued start rather than giving it a newer epoch.
        handle.send(Command::Stop);
        let start = rx.recv().unwrap();
        let stop = rx.recv().unwrap();
        assert!(matches!(start.command, Command::StartFromUi { .. }));
        assert!(matches!(stop.command, Command::Stop));
        assert!(start.shortcut_epoch < stop.shortcut_epoch);
        assert_ne!(start.shortcut_epoch, handle.shortcut_epoch.load(Ordering::SeqCst));
        start.reply.unwrap().send(Ok(json!({"cancelled":true}))).unwrap();
        assert_eq!(pending.wait().unwrap()["cancelled"], true);
    }

    #[test]
    fn timed_out_ui_start_cancels_queued_or_blocked_work_without_cancelling_a_newer_start() {
        let (tx, rx) = mpsc::channel();
        let handle = RuntimeHandle { tx, interrupt: Arc::new(AtomicBool::new(false)), shortcut_epoch: Arc::new(AtomicU64::new(0)) };
        let start_command = || Command::StartFromUi { mode: RunMode::Normal,
            profile: serde_json::to_value(Profile::default()).unwrap(), expected_profile_revision: 0 };
        let pending = handle.enqueue(start_command()).unwrap();
        let start = rx.recv().unwrap();
        assert!(pending.wait_for(Duration::ZERO).is_err());
        assert!(!handle.control_epoch_is_current(start.shortcut_epoch));
        assert!(!handle.interrupt.load(Ordering::SeqCst));
        let cancellation = rx.recv().unwrap();
        let Command::CancelExpiredStart { epoch } = cancellation.command else { panic!("scoped cancellation expected") };
        assert!(handle.control_epoch_is_current(epoch));
        let _new_pending = handle.enqueue(start_command()).unwrap();
        assert!(!handle.control_epoch_is_current(epoch)); // old cleanup cannot stop the new run
        let newer = rx.recv().unwrap();
        handle.interrupt.store(false, Ordering::SeqCst);
        handle.cancel_expired_start(start.shortcut_epoch);
        assert!(handle.control_epoch_is_current(newer.shortcut_epoch));
        assert!(!handle.interrupt.load(Ordering::SeqCst));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn new_start_between_timeout_epoch_invalidation_and_cleanup_keeps_its_controls() {
        let (tx, rx) = mpsc::channel();
        let handle = RuntimeHandle { tx, interrupt: Arc::new(AtomicBool::new(false)), shortcut_epoch: Arc::new(AtomicU64::new(1)) };
        // Reproduce the scheduling boundary immediately after the timeout CAS.
        handle.shortcut_epoch.compare_exchange(1, 2, Ordering::SeqCst, Ordering::SeqCst).unwrap();
        let _pending = handle.enqueue(Command::StartFromUi { mode: RunMode::Seats,
            profile: serde_json::to_value(Profile::default()).unwrap(), expected_profile_revision: 0 }).unwrap();
        let newer = rx.recv().unwrap();
        handle.enqueue_expired_start_cleanup(2);
        let old_cleanup = rx.recv().unwrap();
        let Command::CancelExpiredStart { epoch } = old_cleanup.command else { panic!("cleanup expected") };
        assert!(!handle.control_epoch_is_current(epoch));
        assert!(handle.control_epoch_is_current(newer.shortcut_epoch));
        assert!(!handle.interrupt.load(Ordering::SeqCst));
    }

    #[test]
    fn timeout_cleanup_does_not_own_a_new_shortcut_pending_or_run_in_the_same_epoch() {
        let mut shortcuts = StartShortcutGate::default();
        let request_id = shortcuts.prepare(RunMode::Normal, 10, 2).unwrap();
        // Old UI start epoch 1 was invalidated to 2. New F1 may also use 2;
        // the actual old run must be present before its cleanup can stop it.
        assert!(!expired_start_owns_run(2, 2, None));
        let request = shortcuts.take(request_id, 11, 2).unwrap();
        assert_eq!(request.epoch, 2);
        assert!(!expired_start_owns_run(2, 2, Some(request.epoch)));
        assert!(expired_start_owns_run(2, 2, Some(1)));
        assert!(!expired_start_owns_run(2, 3, Some(1)));
    }

    #[test]
    fn visual_recovery_waits_for_opaque_capture_before_resuming_engine() {
        let mut profile = Profile::default();
        profile.features.conflict_recovery = true;
        let mut engine = Engine::new(profile).unwrap();
        engine.step(Event::Start { mode: RunMode::Seats }, 0);
        engine.step(Event::ContextLost { reason: "[capture_incomplete] partial capture".into() }, 1);
        let mut frame = captcha_test_frame([200, 200, 200], 2, 2);
        for pixel in frame.rgba.chunks_exact_mut(4) { pixel[3] = 0; }
        let error = validate_visual_recovery_frame(&frame, true).unwrap_err();
        assert!(recoverable_context_error(&error));
        assert_eq!(engine.snapshot().phase, Phase::Paused);
        assert!(validate_visual_recovery_frame(&frame, false).is_ok());
        for pixel in frame.rgba.chunks_exact_mut(4) { pixel[3] = 255; }
        validate_visual_recovery_frame(&frame, true).unwrap();
        engine.step(Event::Resume, 2);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
    }

    #[test]
    fn recovery_never_retries_unknown_delivery_or_identity_changes() {
        for code in ["target_not_foreground", "target_occluded", "capture_interrupted", "capture_waiting", "capture_incomplete"] {
            assert!(recoverable_context_error(&format!("[{code}] unavailable")));
        }
        for code in ["input_rejected", "input_event_creation", "accessibility_permission",
            "target_identity_changed", "target_geometry_changed", "target_closed",
            "capture_stopped", "capture_start_failed", "capture_geometry_changed"] {
            assert!(!recoverable_context_error(&format!("[{code}] unavailable")));
        }
        assert!(!recoverable_context_error("unknown error mentioning [target_not_foreground]"));
    }

    #[test]
    fn continuous_routes_do_not_require_manual_entry_region_refresh_or_completion() {
        for mode in [RunMode::Normal, RunMode::Seats, RunMode::Cancellation] {
            let mut profile = Profile::default();
            profile.features.auto_captcha_resume = true;
            let mut engine = Engine::new(profile).unwrap();
            engine.step(Event::Start { mode }, 0);
            let mut state = engine.snapshot();
            for phase in [Phase::Captcha, Phase::Entry, Phase::Region, Phase::WaitingRegion,
                Phase::Seats, Phase::WaitingRefresh, Phase::Completing] {
                state.phase = phase;
                assert_eq!(manual_confirmation_kind(&state, phase, "seats"), None);
            }
        }
    }

    #[test]
    fn captcha_enter_rejects_old_runs_other_apps_and_known_other_windows() {
        let target = macro_platform::TargetWindow { id: 44, pid: 55, title: String::new(),
            rect: macro_platform::Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 } };
        let mut press = macro_platform::EnterPress { token: 7, window_id: 44, pid: 55, observed_at_ns: 1 };
        assert_eq!(enter_target_rejection(&press, 7, &target), None);
        assert_eq!(enter_target_rejection(&press, 8, &target), Some("stale_token"));
        press.window_id = 45;
        assert_eq!(enter_target_rejection(&press, 7, &target), Some("window_mismatch"));
        press.window_id = 0;
        assert_eq!(enter_target_rejection(&press, 7, &target), None);
        press.pid = 56;
        assert_eq!(enter_target_rejection(&press, 7, &target), Some("app_mismatch"));
        assert!(enter_rejection_message("window_mismatch").is_some());
        assert!(enter_rejection_message("stale_event").is_some());
        assert!(enter_rejection_message("enqueued").is_none());
        assert!(enter_rejection_message("repeated").is_none());
    }

    #[test]
    fn captcha_input_records_all_routing_results_and_retains_valid_signals_in_order() {
        let target = macro_platform::TargetWindow { id: 44, pid: 55, title: "excluded title".into(),
            rect: macro_platform::Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 } };
        let first = macro_platform::CaptchaInput { token: 7, window_id: 44, pid: 55, observed_at_ns: 100 };
        let mut records = Vec::new();
        let picked = select_captcha_inputs(vec![
            macro_platform::CaptchaInput { token: 6, ..first },
            macro_platform::CaptchaInput { window_id: 45, ..first },
            macro_platform::CaptchaInput { pid: 56, ..first },
            first,
            macro_platform::CaptchaInput { observed_at_ns: 200, ..first },
        ], 7, Some(&target), Instant::now(), |record| records.push(record));
        assert_eq!(picked.iter().map(|pending| pending.press).collect::<Vec<_>>(),
            vec![first, macro_platform::CaptchaInput { observed_at_ns: 200, ..first }]);
        assert_eq!(records.len(), 5);
        assert_eq!(records.iter().map(|record| record["reason"].clone()).collect::<Vec<_>>(),
            vec![json!("stale_token"), json!("window_mismatch"), json!("app_mismatch"), Value::Null, Value::Null]);
        assert_eq!(records.iter().filter(|record| record["acceptedTarget"] == true).count(), 2);
        assert!(!serde_json::to_string(&records).unwrap().contains("excluded title"));
        records.clear();
        assert!(select_captcha_inputs(vec![first], 7, None, Instant::now(), |record| records.push(record)).is_empty());
        assert_eq!(records[0]["reason"], "target_unavailable");
        assert!(records[0]["boundWindowID"].is_null());
    }

    #[test]
    fn missing_seat_area_wait_starts_after_captcha_and_clears_after_calibration() {
        let mut profile = Profile::default();
        profile.features.nol_captcha_detection = true;
        assert!(validate_entry_setup(&profile, &DesktopConfig::default(), RunMode::Normal).is_ok());
        let mut engine = Engine::new(profile.clone()).unwrap();
        assert!(!waiting_for_seat_area(&engine));
        assert!(!seat_area_guidance(&engine).contains("F3"));
        engine.step(Event::Start { mode: RunMode::Normal }, 0);
        assert_eq!(engine.snapshot().phase, Phase::Captcha);
        assert!(!waiting_for_seat_area(&engine));
        engine.step(Event::NolCaptchaDismissed, 1);
        assert!(waiting_for_seat_area(&engine));
        assert!(seat_area_guidance(&engine).contains("좌석 영역 · 반대 모서리"));
        assert!(seat_area_guidance(&engine).contains("F3"));
        engine.step(Event::Pause, 2);
        assert!(!waiting_for_seat_area(&engine));
        assert!(seat_area_guidance(&engine).contains("F3"));
        profile.vision.roi = Some(Rect { x: 10.0, y: 20.0, width: 100.0, height: 80.0 });
        engine.update_calibration(profile.vision, profile.color, profile.region_color, None).unwrap();
        engine.step(Event::Resume, 3);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        assert!(!waiting_for_seat_area(&engine));
        engine.step(Event::Stop, 4);
        assert!(!waiting_for_seat_area(&engine));
    }

    #[test]
    fn seat_area_wait_respects_the_active_cancellation_region_override() {
        let roi = Rect { x: 10.0, y: 20.0, width: 100.0, height: 80.0 };
        let region_vision = VisionConfig { roi: Some(roi), ..VisionConfig::default() };
        for (global_roi, region_vision, expected_wait, region_label) in [
            (None, Some(region_vision), false, true),
            (Some(roi), Some(VisionConfig::default()), true, true),
            (Some(roi), None, false, false),
            (None, None, true, false),
        ] {
            let mut profile = Profile::default();
            profile.vision.roi = global_roi;
            profile.legacy.cancellation_navigation = macro_core::CancellationNavigation::ArrowKeys;
            profile.legacy.regions = vec![macro_core::LegacyRegionSettings {
                vision: region_vision, ..macro_core::LegacyRegionSettings::default()
            }];
            let mut engine = Engine::new(profile).unwrap();
            let actions = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
            for action in actions.iter().filter(|action| !matches!(action.kind, ActionKind::Observe { .. })) {
                engine.step(Event::ActionAcknowledged { id: action.id, success: true, detail: String::new() }, 1);
            }
            engine.step(Event::Tick, 1000);
            assert_eq!(engine.snapshot().active_region, Some(0));
            assert_eq!(engine.snapshot().phase, Phase::Seats);
            assert_eq!(waiting_for_seat_area(&engine), expected_wait);
            assert_eq!(seat_area_guidance(&engine).contains("1구역 좌석 영역 · 첫 모서리"), region_label);
        }
    }

    #[test]
    fn post_refresh_old_pixels_cannot_trigger_a_seat_but_polling_continues() {
        let mut profile = Profile::default();
        profile.features.automatic_refresh = true;
        profile.refresh_target = Some(Target::Point { point: Point { x: 40.0, y: 30.0 } });
        profile.vision.roi = Some(Rect { x: 0.0, y: 0.0, width: 8.0, height: 8.0 });
        let mut engine = Engine::new(profile.clone()).unwrap();
        engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
        engine.step(Event::CandidatesObserved { generation: 0, candidates: vec![] }, 1);
        let actions = engine.step(Event::Tick, 1001);
        let refresh = actions.iter().find(|a| matches!(a.kind, ActionKind::Refresh { .. })).unwrap();
        engine.step(Event::ActionAcknowledged { id: refresh.id, success: true, detail: String::new() }, 1002);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        let mut pixels = vec![0u8; 8 * 8 * 4];
        for (x, y) in [(2, 2), (3, 2), (2, 3), (3, 3)] {
            let offset = (y * 8 + x) * 4;
            pixels[offset..offset + 4].copy_from_slice(&[profile.color.r, profile.color.g, profile.color.b, 255]);
        }
        let candidates = macro_core::vision::extract_candidates_view(
            macro_core::vision::RgbaView { width: 8, height: 8, stride: 32, rgba: &pixels },
            profile.color, profile.color_tolerance, &profile.vision, 0).unwrap();
        assert_eq!(candidates.len(), 1);
        let stale_frame = captcha_test_frame([200, 200, 200], 13, 99_000_000);
        assert!(frame_passes_transition(stale_frame.sequence, Some(12))); // old guard admitted it
        let stale = if frame_passes_input_transition(&stale_frame, Some(12), Some(100_000_000)) { candidates.clone() } else { vec![] };
        assert!(engine.step(Event::CandidatesObserved { generation: 0, candidates: stale }, 1003).is_empty());
        assert!(engine.snapshot().current_candidate_id.is_none());
        let restored = captcha_test_frame([200, 200, 200], 14, 100_001_000);
        // This frame can arrive while native click still completes mouse-up.
        // A later function-return fence (e.g. 100_005_000) would deadlock a static map.
        assert!(frame_passes_input_transition(&restored, Some(12), Some(100_000_000)));
        let actions = engine.step(Event::CandidatesObserved { generation: 0, candidates }, 1004);
        assert_eq!(actions.iter().filter(|a| matches!(&a.kind, ActionKind::Click { purpose, .. } if purpose.starts_with("seat:"))).count(), 1);
        // An explicit already-open Seats start has no navigation-image barrier.
        assert!(frame_passes_transition(12, None));
    }

    #[test]
    fn region_click_requires_a_capture_after_native_post_not_only_a_new_sequence() {
        let mut profile = Profile::default();
        profile.features.nol_captcha_detection = true;
        profile.features.region_priority = true;
        let candidate = Candidate { id: "candidate".into(), point: Point { x: 3.0, y: 3.0 },
            target: Target::Point { point: Point { x: 3.0, y: 3.0 } }, color: profile.color, selected: false, geometry: None };
        let mut engine = Engine::new(profile).unwrap();
        engine.step(Event::Start { mode: RunMode::Normal }, 0);
        engine.step(Event::CaptchaManuallyCompleted, 1);
        let region = engine.step(Event::RegionsObserved { candidates: vec![RegionCandidate {
            candidate: candidate.clone(), center_rank: 0, distance_to_stage: 1.0,
        }] }, 2);
        let click = region.iter().find(|action| matches!(&action.kind, ActionKind::Click { purpose, .. } if purpose.starts_with("region:"))).unwrap();
        engine.step(Event::ActionAcknowledged { id: click.id, success: true, detail: String::new() }, 100);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        for captured in [99_000_000, 100_000_000] {
            let frame = captcha_test_frame([200, 200, 200], 13, captured);
            assert!(!frame_passes_input_transition(&frame, Some(12), Some(100_000_000)));
        }
        assert!(engine.snapshot().current_candidate_id.is_none());
        let frame = captcha_test_frame([200, 200, 200], 14, 100_001_000);
        assert!(frame_passes_input_transition(&frame, Some(12), Some(100_000_000)));
        let actions = engine.step(Event::CandidatesObserved { generation: engine.snapshot().generation, candidates: vec![candidate] }, 101);
        assert!(actions.iter().any(|action| matches!(&action.kind, ActionKind::Click { purpose, .. } if purpose.starts_with("seat:"))));
    }

    #[test]
    fn f6_remains_available_after_context_loss_on_an_already_open_seat_map() {
        let mut profile = Profile::default();
        profile.features.region_priority = true;
        profile.legacy.cancellation_navigation = macro_core::CancellationNavigation::ArrowKeys;
        let mut engine = Engine::new(profile).unwrap();
        engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
        engine.step(Event::ContextLost { reason: "target_not_foreground".into() }, 1);
        assert_eq!(engine.snapshot().phase, Phase::Paused);
        assert_eq!(manual_confirmation_kind(&engine.snapshot(), engine.confirmation_phase(), "regions").as_deref(),
            Some("region_map_ready"));
        engine.step(Event::Resume, 2);
        engine.step(Event::RegionMapReady, 3);
        assert_eq!(engine.snapshot().phase, Phase::Seats);
        assert_eq!(manual_confirmation_kind(&engine.snapshot(), engine.confirmation_phase(), "seats"), None);
        engine.step(Event::Pause, 4);
        assert_eq!(manual_confirmation_kind(&engine.snapshot(), engine.confirmation_phase(), "seats"), None);
    }

    #[test]
    fn cached_rescan_requires_idle_seat_observation_not_result_or_refresh() {
        let mut engine = Engine::new(Profile::default()).unwrap();
        engine.step(Event::Start { mode: RunMode::Normal }, 0);
        let mut state = engine.snapshot();
        assert!(can_rescan_seat_frame(&state, "seats", true));
        assert!(!can_rescan_seat_frame(&state, "seats", false));
        for scope in ["selection", "seat_result", "refresh", "conflict_dialog"] {
            assert!(!can_rescan_seat_frame(&state, scope, true));
        }
        state.current_candidate_id = Some("waiting-for-result".into());
        assert!(!can_rescan_seat_frame(&state, "seats", true));
        state.current_candidate_id = None;
        state.pending_action_id = Some(7);
        assert!(!can_rescan_seat_frame(&state, "seats", true));
        state.pending_action_id = None;
        for phase in [Phase::WaitingRefresh, Phase::Completing, Phase::Conflict, Phase::Paused, Phase::Stopped] {
            state.phase = phase;
            assert!(!can_rescan_seat_frame(&state, "seats", true));
        }
    }

    #[test]
    fn empty_static_map_is_polled_then_new_pixels_produce_one_click() {
        let mut profile = Profile::default();
        profile.features.conflict_recovery = true;
        profile.color = Color { r: 130, g: 70, b: 210 };
        profile.vision.roi = Some(Rect { x: 0.0, y: 0.0, width: 8.0, height: 8.0 });
        profile.vision.min_component_pixels = 4;
        let mut engine = Engine::new(profile.clone()).unwrap();
        engine.step(Event::Start { mode: RunMode::Normal }, 0);
        let mut pixels = vec![0u8; 8 * 8 * 4];
        let scan = |rgba: &[u8]| macro_core::vision::extract_candidates_view(
            macro_core::vision::RgbaView { width: 8, height: 8, stride: 32, rgba },
            profile.color, profile.color_tolerance, &profile.vision, 0).unwrap();
        assert!(engine.step(Event::CandidatesObserved { generation: 0, candidates: scan(&pixels) }, 1).is_empty());
        for now in [1001, 2001, 6001, 12001] {
            let actions = engine.step(Event::Tick, now);
            assert!(actions.iter().any(|a| matches!(&a.kind, ActionKind::Observe { scope } if scope == "seats")));
            assert!(can_rescan_seat_frame(&engine.snapshot(), "seats", true));
            assert!(engine.step(Event::CandidatesObserved { generation: 0, candidates: scan(&pixels) }, now).is_empty());
            assert_eq!(engine.snapshot().phase, Phase::Seats);
            assert!(!engine.snapshot().message.contains("결과 확인 대기"));
        }
        for (x, y) in [(2, 2), (3, 2), (2, 3), (3, 3)] {
            let offset = (y * 8 + x) * 4;
            pixels[offset..offset + 4].copy_from_slice(&[130, 70, 210, 255]);
        }
        let actions = engine.step(Event::CandidatesObserved { generation: 0, candidates: scan(&pixels) }, 12002);
        let clicks: Vec<_> = actions.iter().filter(|a| matches!(&a.kind, ActionKind::Click { purpose, .. } if purpose.starts_with("seat:"))).collect();
        assert_eq!(clicks.len(), 1);
        assert!(!can_rescan_seat_frame(&engine.snapshot(), "seats", true));
        engine.step(Event::ActionAcknowledged { id: clicks[0].id, success: true, detail: String::new() }, 12003);
        assert!(!can_rescan_seat_frame(&engine.snapshot(), "seats", true));
        assert!(engine.step(Event::Tick, 18003).is_empty());
        assert!(engine.snapshot().selected_ids.is_empty());
    }
}

#[cfg(test)]
mod priority_p1_tests {
    use super::*;

    const POST_NS: u64 = 100_000_000;

    // Scripted native return values, actual OwnedFrame buffers, production
    // runtime branches, and the real vision/engine. No AppHandle, OS input,
    // browser stage marker, or successful selection is supplied to production.
    struct Replay {
        engine: Engine,
        cached: Option<OwnedFrame>,
        last_sequence: Option<u64>,
        last_generation: Option<u64>,
        fence_sequence: Option<u64>,
        fence_ns: Option<u64>,
        scope: String,
        requested: bool,
    }

    fn frame(sequence: u64, captured_at_ns: u64) -> OwnedFrame {
        let mut rgba = [240, 240, 240, 255].repeat(24 * 16);
        for y in 4..8 {
            for x in 4..8 {
                rgba[(y * 24 + x) * 4..(y * 24 + x) * 4 + 4].copy_from_slice(&[80, 30, 180, 255]);
            }
        }
        OwnedFrame { width: 24, height: 16, stride: 24 * 4, rgba,
            origin: macro_platform::Point { x: 0.0, y: 0.0 }, scale_x: 1.0, scale_y: 1.0,
            sequence, captured_at_ns, last_observed_ns: captured_at_ns }
    }

    impl Replay {
        fn after_legacy_navigation(keys: bool) -> Self {
            let mut profile = Profile::default();
            profile.color = Color { r: 80, g: 30, b: 180 };
            profile.color_tolerance = 0;
            profile.vision.roi = Some(Rect { x: 0.0, y: 0.0, width: 24.0, height: 16.0 });
            profile.legacy.region_delay_ms = 10;
            profile.legacy.cancellation_navigation = if keys { macro_core::CancellationNavigation::ArrowKeys }
                else { macro_core::CancellationNavigation::TwoRegions {
                    first: Target::Point { point: Point { x: 20.0, y: 2.0 } },
                    second: Target::Point { point: Point { x: 20.0, y: 12.0 } },
                }};
            apply_execution_options(&mut profile, &DesktopConfig::default(), RunMode::Cancellation);
            let mut engine = Engine::new(profile).unwrap();
            let start = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
            let navigation = start.iter().find(|action| !matches!(action.kind, ActionKind::Observe { .. })).unwrap();
            assert!(matches!(&navigation.kind, ActionKind::Click { purpose, .. } | ActionKind::Key { purpose, .. }
                if purpose == "legacy_cancellation_region"));
            let has_fence = navigation_requires_new_pixels(&navigation.kind, engine.continuous_search(), true);
            engine.step(Event::ActionAcknowledged { id: navigation.id, success: true, detail: String::new() }, 100);
            let ready = engine.step(Event::Tick, 110);
            let scope = ready.iter().find_map(|action| if let ActionKind::Observe { scope } = &action.kind {
                Some(scope.clone())
            } else { None }).unwrap();
            assert_eq!(scope, "seats");
            Self { engine, cached: Some(frame(12, POST_NS - 1_000_000)), last_sequence: Some(12),
                last_generation: None, fence_sequence: has_fence.then_some(12), fence_ns: has_fence.then_some(POST_NS),
                scope, requested: true }
        }

        fn scan(&mut self, capture: Result<Option<OwnedFrame>, String>, now_ms: u64) -> Result<Option<Point>, String> {
            self.scan_with_preclick(capture, Ok(None), now_ms)
        }

        fn scan_with_preclick(&mut self, capture: Result<Option<OwnedFrame>, String>,
            preclick_capture: Result<Option<OwnedFrame>, String>, now_ms: u64) -> Result<Option<Point>, String> {
            let state = self.engine.snapshot();
            let reuse = can_rescan_seat_frame(&state, &self.scope, self.requested
                || first_legacy_scan_requested(self.engine.profile(), state.generation, self.last_generation));
            if !apply_scan_capture(&mut self.cached, &mut self.last_sequence, capture, reuse)? { return Ok(None); }
            let frame = self.cached.as_ref().unwrap();
            if !frame_passes_input_transition(frame, self.fence_sequence, self.fence_ns) { return Ok(None); }
            let mut candidates = macro_core::vision::extract_candidates_view(macro_core::vision::RgbaView {
                width: frame.width as u32, height: frame.height as u32, stride: frame.stride, rgba: &frame.rgba,
            }, self.engine.effective_color(), self.engine.profile().color_tolerance,
                self.engine.effective_vision(), state.generation).unwrap();
            if let Some(index) = state.active_region {
                for candidate in &mut candidates { candidate.id = format!("zone{index}:{}", candidate.id); }
            }
            self.last_generation = Some(state.generation);
            self.requested = false;
            let actions = self.engine.step(Event::CandidatesObserved { generation: state.generation, candidates }, now_ms);
            for action in actions {
                if let ActionKind::Click { target: target @ Target::Point { point }, purpose } = action.kind {
                    if purpose.starts_with("seat:") {
                        // The real second native query may return unchanged pixels,
                        // a newer image, or an error after candidate extraction.
                        apply_candidate_capture(&mut self.cached, None, preclick_capture)?;
                        if candidate_available_in_frame(self.cached.as_ref(), &target,
                            self.engine.effective_color(), self.engine.profile().color_tolerance, None, None)? {
                            return Ok(Some(point)); // eligible for guarded native input; not an OS delivery
                        }
                        self.engine.step(Event::ActionCancelled { id: action.id }, now_ms);
                        return Ok(None);
                    }
                }
            }
            Ok(None)
        }
    }

    #[test]
    fn p1_legacy_old_cached_and_unread_pre_post_pixels_reach_the_real_preclick_check() {
        for keys in [false, true] {
            for unread in [false, true] {
                let mut replay = Replay::after_legacy_navigation(keys);
                let capture = unread.then(|| frame(13, POST_NS - 1_000_000));
                let chosen = replay.scan(Ok(capture), 111).unwrap();
                assert!(chosen.is_some());
                let cached = replay.cached.as_ref().unwrap();
                assert!(cached.captured_at_ns < POST_NS);
                assert!(!frame_passes_input_transition(cached, Some(12), Some(POST_NS)));
                println!("{}", json!({"case":"legacy_pre_input_pixels","navigation":if keys {"keys"} else {"two_regions"},
                    "unreadNewSequence":unread,"eligibleSeatPoint":chosen,"nativeInputSent":false}));
            }
        }
    }

    #[test]
    fn p1_strict_new_pixel_fence_rejects_a_legitimate_static_map_indefinitely() {
        let mut current = Replay::after_legacy_navigation(true);
        assert!(current.scan(Ok(None), 111).unwrap().is_some());
        let mut strict = Replay::after_legacy_navigation(true);
        strict.fence_sequence = Some(12);
        strict.fence_ns = Some(POST_NS);
        for now_ms in [111, 1_000, 60_000, 3_600_000] {
            assert!(strict.scan(Ok(None), now_ms).unwrap().is_none());
            let actions = strict.engine.step(Event::Tick, now_ms);
            assert!(actions.iter().all(|action| matches!(action.kind, ActionKind::Observe { .. })));
            assert_eq!(strict.engine.snapshot().phase, Phase::Seats);
        }
        println!("{}", json!({"case":"legitimate_identical_destination","strictFenceProgress":false,
            "observedUntilMs":3_600_000,"captureSource":"scripted_static_SCK_contract"}));
    }

    #[test]
    fn p1_fresh_idle_and_fresh_pixels_do_not_identify_which_region_the_site_rendered() {
        for fresh_pixels in [false, true] {
            let mut points = Vec::new();
            for truth in ["destination_rendered_identical_map", "previous_region_while_destination_loading"] {
                let mut replay = Replay::after_legacy_navigation(false);
                if fresh_pixels {
                    // A fresh callback or GDI capture may still show old site content.
                    let post_frame = frame(13, POST_NS + 5_000_000);
                    assert!(frame_passes_input_transition(&post_frame, Some(12), Some(POST_NS)));
                    replay.cached = Some(post_frame.clone());
                    replay.last_sequence = Some(13);
                } else {
                    // This uses the production metadata updater; no new seq/capture is invented.
                    apply_frame_observation(&mut replay.cached, FrameObservation::Unchanged {
                        sequence: 12, captured_at_ns: POST_NS - 1_000_000, last_observed_ns: POST_NS + 5_000_000,
                    }).unwrap();
                    assert!(replay.cached.as_ref().unwrap().last_observed_ns > POST_NS);
                    assert!(!frame_passes_input_transition(replay.cached.as_ref().unwrap(), Some(12), Some(POST_NS)));
                }
                let chosen = replay.scan(Ok(None), 111).unwrap();
                assert!(chosen.is_some());
                points.push(chosen);
                println!("{}", json!({"case":"freshness_is_not_region_identity","truthOnlyInTestOracle":truth,
                    "freshPixels":fresh_pixels,"eligibleSeatPoint":chosen,"semanticRegionKnownToRuntime":false}));
            }
            assert_eq!(points[0], points[1]); // identical observable evidence, opposite site truth
        }
    }

    #[test]
    fn p1_capture_errors_or_no_cached_pixels_never_become_a_static_scan() {
        let mut replay = Replay::after_legacy_navigation(false);
        let error = replay.scan(Err("[capture_interrupted] scripted native error".into()), 111).unwrap_err();
        assert!(recoverable_context_error(&error));
        assert!(replay.engine.snapshot().pending_action_id.is_none());
        replay.cached = None;
        assert!(replay.scan(Ok(None), 112).unwrap().is_none());
        let fresh = frame(13, POST_NS + 5_000_000);
        assert!(replay.scan(Ok(Some(fresh)), 113).unwrap().is_some());
    }

    #[test]
    fn p1_navigation_fence_keeps_current_delivery_and_mode_boundaries() {
        let region = ActionKind::Click { target: Target::Point { point: Point { x: 1.0, y: 1.0 } }, purpose: "region:one".into() };
        assert!(navigation_requires_new_pixels(&region, true, true));
        assert!(!navigation_requires_new_pixels(&region, true, false));
        assert!(!navigation_requires_new_pixels(&region, false, true));
        let legacy = ActionKind::Key { key: Key::ArrowDown, purpose: "legacy_cancellation_region".into() };
        assert!(!navigation_requires_new_pixels(&legacy, false, true));
        assert!(!navigation_requires_new_pixels(&legacy, false, false));
    }

    #[test]
    fn p1_second_native_query_replaces_pixels_and_rejects_changed_geometry_or_transparency() {
        for condition in ["unchanged", "new_seat", "new_empty", "geometry", "transparent", "error"] {
            let mut replay = Replay::after_legacy_navigation(false);
            let mut updated = frame(13, POST_NS + 5_000_000);
            match condition {
                "new_empty" => updated.rgba = [240, 240, 240, 255].repeat(24 * 16),
                "geometry" => updated.scale_x = 2.0,
                "transparent" => { for pixel in updated.rgba.chunks_exact_mut(4) { pixel[3] = 249; } },
                _ => {},
            }
            let capture = match condition {
                "unchanged" => Ok(None),
                "error" => Err("[capture_interrupted] scripted second-query failure".into()),
                _ => Ok(Some(updated)),
            };
            let result = replay.scan_with_preclick(Ok(None), capture, 111);
            match condition {
                "unchanged" | "new_seat" => assert!(result.unwrap().is_some()),
                "new_empty" | "transparent" => {
                    assert!(result.unwrap().is_none());
                    assert!(replay.engine.snapshot().pending_action_id.is_none());
                    assert!(replay.engine.snapshot().selected_ids.is_empty());
                },
                _ => assert!(result.is_err()),
            }
        }
    }
}

#[cfg(test)]
mod priority_p4_tests {
    use super::*;

    fn pixels(sequence: u64, captured_at_ns: u64, seat: bool) -> OwnedFrame {
        let mut rgba = [240, 240, 240, 255].repeat(24 * 16);
        if seat { for y in 4..8 { for x in 4..8 {
            rgba[(y * 24 + x) * 4..(y * 24 + x) * 4 + 4].copy_from_slice(&[80, 30, 180, 255]);
        }}}
        OwnedFrame { width: 24, height: 16, stride: 96, rgba,
            origin: macro_platform::Point { x: 0.0, y: 0.0 }, scale_x: 1.0, scale_y: 1.0,
            sequence, captured_at_ns, last_observed_ns: captured_at_ns }
    }

    // Actual runtime capture/color/fence functions and Engine, with explicit
    // native return values and ACKs. No native input or refresh-success event.
    struct Replay {
        engine: Engine,
        cached: Option<OwnedFrame>,
        last_sequence: Option<u64>,
        last_input_sequence: Option<u64>,
        after: Option<u64>,
        after_ns: Option<u64>,
        requested: bool,
        scope: String,
        refreshes: Vec<u64>,
        eligible_seats: Vec<Point>,
        scans: usize,
    }

    impl Replay {
        fn new(refresh: bool) -> Self {
            let mut profile = Profile::default();
            profile.color = Color { r: 80, g: 30, b: 180 };
            profile.color_tolerance = 0;
            profile.features.automatic_refresh = refresh;
            profile.refresh_interval_ms = 1000;
            profile.refresh_target = Some(Target::Point { point: Point { x: 23.0, y: 15.0 } });
            profile.vision.roi = Some(Rect { x: 0.0, y: 0.0, width: 24.0, height: 16.0 });
            apply_execution_options(&mut profile, &DesktopConfig { refresh_mode: "internalButton".into(),
                ..DesktopConfig::default() }, RunMode::Cancellation);
            let mut engine = Engine::new(profile).unwrap();
            let actions = engine.step(Event::Start { mode: RunMode::Cancellation }, 0);
            let mut replay = Self { engine, cached: None, last_sequence: None, last_input_sequence: None,
                after: None, after_ns: None, requested: false, scope: String::new(),
                refreshes: vec![], eligible_seats: vec![], scans: 0 };
            replay.actions(actions, 0);
            replay.scan(Ok(Some(pixels(12, 1_000_000, false))), 1);
            replay
        }

        fn actions(&mut self, actions: Vec<Action>, now_ms: u64) {
            for action in actions {
                match action.kind {
                    ActionKind::Observe { scope } => {
                        self.requested = scope == "seats";
                        self.scope = scope;
                    },
                    ActionKind::Refresh { target } => {
                        assert_eq!(target, Target::Point { point: Point { x: 23.0, y: 15.0 } });
                        assert!(self.refreshes.last().is_none_or(|previous| now_ms - previous >= 1000));
                        self.refreshes.push(now_ms);
                        self.last_input_sequence = self.cached.as_ref().map(|frame| frame.sequence);
                        self.after = self.last_input_sequence;
                        self.after_ns = Some(now_ms * 1_000_000);
                        let ack = self.engine.step(Event::ActionAcknowledged {
                            id: action.id, success: true, detail: String::new(),
                        }, now_ms);
                        self.actions(ack, now_ms);
                    },
                    ActionKind::Click { target: target @ Target::Point { point }, purpose } if purpose.starts_with("seat:") => {
                        // Explicitly unchanged second native query; its production
                        // error/changed-pixel/geometry cases are covered by P1.
                        apply_candidate_capture(&mut self.cached, None, Ok(None)).unwrap();
                        if candidate_available_in_frame(self.cached.as_ref(), &target, self.engine.effective_color(),
                            self.engine.profile().color_tolerance, self.after, self.after_ns).unwrap() {
                            self.eligible_seats.push(point);
                        } else {
                            self.engine.step(Event::ActionCancelled { id: action.id }, now_ms);
                        }
                    },
                    other => panic!("unexpected physical action: {other:?}"),
                }
            }
        }

        fn tick(&mut self, now_ms: u64) {
            let actions = self.engine.step(Event::Tick, now_ms);
            self.actions(actions, now_ms);
        }

        fn scan(&mut self, capture: Result<Option<OwnedFrame>, String>, now_ms: u64) {
            let state = self.engine.snapshot();
            let reuse = can_rescan_seat_frame(&state, &self.scope, self.requested);
            if !apply_scan_capture(&mut self.cached, &mut self.last_sequence, capture, reuse).unwrap() { return; }
            if self.last_input_sequence == self.last_sequence && !reuse { return; }
            let frame = self.cached.as_ref().unwrap();
            let mut candidates = macro_core::vision::extract_candidates_view(macro_core::vision::RgbaView {
                width: frame.width as u32, height: frame.height as u32, stride: frame.stride, rgba: &frame.rgba,
            }, self.engine.effective_color(), self.engine.profile().color_tolerance,
                self.engine.effective_vision(), state.generation).unwrap();
            // Same glue as observe_frame: freshness gates candidates, while the
            // empty observation still drives the normal timed refresh loop.
            if !frame_passes_input_transition(frame, self.after, self.after_ns) { candidates.clear(); }
            else { self.after = None; self.after_ns = None; }
            self.requested = false;
            self.scans += 1;
            let actions = self.engine.step(Event::CandidatesObserved { generation: state.generation, candidates }, now_ms);
            self.actions(actions, now_ms);
        }
    }

    #[test]
    fn p4_refresh_no_response_retries_only_the_registered_button_at_the_interval() {
        let mut replay = Replay::new(true);
        for now in (10..=10_010).step_by(10) {
            replay.tick(now);
            replay.scan(Ok(None), now);
        }
        assert_eq!(replay.refreshes.len(), 10);
        assert!(replay.eligible_seats.is_empty());
        assert_eq!(replay.engine.snapshot().phase, Phase::Seats);
        assert_eq!(replay.engine.snapshot().generation, 0);
        assert!(replay.scans >= 11);
        assert!(replay.after.is_some());
        println!("{}", json!({"case":"no_refresh_response","buttonDeliveriesSimulated":replay.refreshes,
            "scans":replay.scans,"generation":0,"nativeInputSent":false}));
    }

    #[test]
    fn p4_static_refresh_off_keeps_scanning_and_unchanged_on_keeps_refreshing() {
        let mut off = Replay::new(false);
        let mut on = Replay::new(true);
        for now in (10..=10_010).step_by(10) {
            for replay in [&mut off, &mut on] { replay.tick(now); replay.scan(Ok(None), now); }
        }
        assert!(off.refreshes.is_empty());
        assert!(off.scans >= 10);
        assert_eq!(on.refreshes.len(), 10);
        assert!(on.after.is_some());
        for replay in [&off, &on] {
            assert_eq!(replay.engine.snapshot().phase, Phase::Seats);
            assert!(replay.engine.snapshot().pending_action_id.is_none());
        }
    }

    #[test]
    fn p4_delayed_pre_latest_refresh_image_is_blocked_then_new_seat_resumes() {
        let mut replay = Replay::new(true);
        replay.tick(1001);
        replay.scan(Ok(None), 1002);
        replay.tick(2001);
        assert_eq!(replay.refreshes, vec![1001, 2001]);
        // Higher sequence, but image was captured before the latest button post.
        replay.scan(Ok(Some(pixels(13, 1_500_000_000, true))), 2002);
        assert!(replay.eligible_seats.is_empty());
        assert_eq!(replay.after_ns, Some(2_001_000_000));
        replay.scan(Ok(Some(pixels(14, 2_003_000_000, true))), 2003);
        assert_eq!(replay.eligible_seats, vec![Point { x: 5.0, y: 5.0 }]);
        assert!(replay.after.is_none());
        assert_eq!(replay.engine.snapshot().generation, 0);
    }

    #[test]
    fn p4_same_pixels_new_capture_releases_fence_but_response_identity_remains_unknown() {
        let mut static_map = Replay::new(true);
        static_map.tick(1001);
        static_map.scan(Ok(Some(pixels(13, 1_002_000_000, false))), 1002);
        assert!(static_map.after.is_none());
        assert!(static_map.eligible_seats.is_empty());
        static_map.tick(2001);
        assert_eq!(static_map.refreshes.len(), 2);
        let mut decisions = vec![];
        for truth in ["latest_refresh_response", "older_response_rendered_after_latest_post"] {
            let mut replay = Replay::new(true);
            replay.tick(1001); replay.scan(Ok(None), 1002); replay.tick(2001);
            replay.scan(Ok(Some(pixels(13, 2_002_000_000, true))), 2002);
            assert_eq!(replay.eligible_seats.len(), 1);
            decisions.push(replay.eligible_seats);
            println!("{}", json!({"case":"refresh_response_identity_unobservable","truthOnlyInTestOracle":truth,
                "eligibleSeatCount":1,"refreshCompletionClaimed":false}));
        }
        assert_eq!(decisions[0], decisions[1]);
    }
}

#[cfg(test)]
mod seat_accuracy_runtime_tests {
    use super::*;

    fn map(scale: f64) -> OwnedFrame {
        let mut rgba = [240, 240, 240, 255].repeat(80 * 40);
        for y in 10..15 { for x in 30..35 {
            rgba[(y * 80 + x) * 4..(y * 80 + x) * 4 + 4].copy_from_slice(&[185, 224, 180, 255]);
        }}
        OwnedFrame { width: 80, height: 40, stride: 320, rgba,
            origin: macro_platform::Point { x: 0.0, y: 0.0 }, scale_x: scale, scale_y: scale,
            sequence: 2, captured_at_ns: 200, last_observed_ns: 200 }
    }

    fn candidate(frame: &OwnedFrame, config: &VisionConfig) -> Candidate {
        macro_core::vision::extract_seat_candidates_view_with_stats(macro_core::vision::RgbaView {
            width: frame.width as u32, height: frame.height as u32, stride: frame.stride, rgba: &frame.rgba,
        }, Color { r: 185, g: 224, b: 180 }, 12, config, 0).unwrap().candidates.remove(0)
    }

    #[test]
    fn candidate_cache_discards_historical_observations_during_continuous_search() {
        let prototype = candidate(&map(2.0), &VisionConfig::default());
        let mut cache = HashMap::new();
        for sequence in 0..1000 {
            let mut latest = prototype.clone();
            latest.id = format!("frame-{sequence}");
            update_candidate_cache(&mut cache, &[latest.clone()], None);
            assert_eq!(cache.len(), 1, "an idle cancellation search only needs its current candidates");
            assert!(cache.contains_key(&latest.id));
        }
        update_candidate_cache(&mut cache, &[], None);
        assert!(cache.is_empty());
    }

    #[test]
    fn candidate_cache_retains_original_inflight_point_until_result_is_resolved() {
        let original = candidate(&map(2.0), &VisionConfig::default());
        let mut cache = HashMap::new();
        update_candidate_cache(&mut cache, &[original.clone()], None);
        // A new image can move the color centroid while retaining its top-left ID.
        // Confirmed mode must still sample the point used by the delivered click.
        let mut repainted_frame = map(2.0);
        for y in 10..15 { for x in 35..38 {
            repainted_frame.rgba[(y * 80 + x) * 4..(y * 80 + x) * 4 + 4]
                .copy_from_slice(&[185, 224, 180, 255]);
        }}
        let repainted = candidate(&repainted_frame, &VisionConfig::default());
        assert_eq!(repainted.id, original.id);
        assert_ne!(repainted.point, original.point);
        let mut neighbor = original.clone();
        neighbor.id = "neighbor".into();
        update_candidate_cache(&mut cache, &[repainted, neighbor.clone()], Some(&original.id));
        assert_eq!(cache.len(), 2);
        assert_eq!(cache[&original.id].point, original.point);
        update_candidate_cache(&mut cache, &[], Some(&original.id));
        assert_eq!(cache.len(), 1, "keep an in-flight point even if its old color disappeared");
        assert_eq!(cache[&original.id].point, original.point);
        update_candidate_cache(&mut cache, &[neighbor.clone()], None);
        assert_eq!(cache.len(), 1);
        assert!(!cache.contains_key(&original.id));
        assert!(cache.contains_key(&neighbor.id));
    }

    #[test]
    fn candidate_cache_can_seed_a_missing_current_candidate_from_the_latest_frame() {
        let current = candidate(&map(1.0), &VisionConfig::default());
        let mut cache = HashMap::new();
        update_candidate_cache(&mut cache, &[current.clone()], Some(&current.id));
        assert_eq!(cache[&current.id].point, current.point);
        update_candidate_cache(&mut cache, &[], Some("different-current"));
        assert!(cache.is_empty(), "an obsolete current candidate cannot survive a context switch");
    }

    #[test]
    fn pre_click_rejects_a_seat_that_became_a_color_fragment_without_counting_delivery() {
        for scale in [1.0, 1.3, 1.75, 2.0] {
            let mut frame = map(scale);
            let config = VisionConfig::default();
            let candidate = candidate(&frame, &config);
            let color = candidate.color;
            assert!(seat_candidate_available_in_frame(Some(&frame), &candidate.target, color, 12,
                &config, Some(1), Some(100)).unwrap());
            // The center remains the expected color, but only a thin residue survives.
            for y in 10..15 { for x in 30..35 {
                if x != candidate.point.x as usize {
                    frame.rgba[(y * 80 + x) * 4..(y * 80 + x) * 4 + 4].copy_from_slice(&[140, 140, 140, 255]);
                }
            }}
            assert!(candidate_available_in_frame(Some(&frame), &candidate.target, color, 12, None, None).unwrap(),
                "the old single-pixel check accepts this residue");
            assert!(!seat_candidate_available_in_frame(Some(&frame), &candidate.target, color, 12,
                &config, None, None).unwrap());
            let mut profile = Profile::default();
            profile.target_seats = 2;
            profile.color = color;
            let mut engine = Engine::new(profile).unwrap();
            engine.step(Event::Start { mode: RunMode::Seats }, 0);
            let actions = engine.step(Event::CandidatesObserved { generation: 0, candidates: vec![candidate] }, 1);
            let click = actions.iter().find(|action| matches!(action.kind, ActionKind::Click { .. })).unwrap();
            let following = engine.step(Event::ActionCancelled { id: click.id }, 2);
            assert!(engine.snapshot().attempted_ids.is_empty());
            assert!(!following.iter().any(|action| matches!(action.kind, ActionKind::CompleteSelection { .. })));
        }
    }

    #[test]
    fn supported_seats_still_obey_refresh_timestamp_roi_exclusions_and_opacity() {
        let mut frame = map(2.0);
        let config = VisionConfig::default();
        let seat = candidate(&frame, &config);
        let available = |frame: &OwnedFrame, config: &VisionConfig, sequence, timestamp| {
            seat_candidate_available_in_frame(Some(frame), &seat.target, seat.color, 12,
                config, sequence, timestamp).unwrap()
        };
        assert!(available(&frame, &config, None, None));
        assert!(!available(&frame, &config, Some(2), None));
        assert!(!available(&frame, &config, None, Some(200)));
        let clipped = VisionConfig { roi: Some(Rect { x: seat.point.x, y: 10.0, width: 1.0, height: 5.0 }), ..config.clone() };
        assert!(!available(&frame, &clipped, None, None));
        let excluded = VisionConfig { exclusions: vec![Rect { x: 30.0, y: 10.0, width: 5.0, height: 5.0 }], ..config.clone() };
        assert!(!available(&frame, &excluded, None, None));
        for pixel in frame.rgba.chunks_exact_mut(4) { pixel[3] = 249; }
        assert!(!available(&frame, &config, None, None));
    }
}

#[cfg(test)]
mod priority_p7_pixel_tests {
    use super::*;

    #[test]
    fn p7_extracted_candidate_checks_the_same_physical_pixel_at_fractional_scale() {
        for scale in [1.0, 2.0, 1.75, 1.3] {
            let color = Color { r: 80, g: 30, b: 180 };
            let mut rgba = [240, 240, 240, 255].repeat(128 * 64);
            for y in 4..7 { rgba[(y * 128 + 61) * 4..(y * 128 + 61) * 4 + 4]
                .copy_from_slice(&[color.r, color.g, color.b, 255]); }
            let frame = OwnedFrame { width: 128, height: 64, stride: 512, rgba,
                origin: macro_platform::Point { x: 0.0, y: 0.0 }, scale_x: scale, scale_y: scale,
                sequence: 1, captured_at_ns: 1, last_observed_ns: 1 };
            let mut profile = Profile::default();
            profile.color = color;
            profile.color_tolerance = 0;
            let candidates = macro_core::vision::extract_candidates_view(macro_core::vision::RgbaView {
                width: 128, height: 64, stride: 512, rgba: &frame.rgba,
            }, color, 0, &profile.vision, 0).unwrap();
            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].point, Point { x: 61.0, y: 5.0 });
            let mut engine = Engine::new(profile).unwrap();
            engine.step(Event::Start { mode: RunMode::Seats }, 0);
            let actions = engine.step(Event::CandidatesObserved { generation: 0, candidates }, 1);
            let target = actions.iter().find_map(|action| match &action.kind {
                ActionKind::Click { target, purpose } if purpose.starts_with("seat:") => Some(target), _ => None,
            }).unwrap();
            let raw = macro_platform::Point { x: 61.0 / scale, y: 5.0 / scale };
            println!("{}", json!({"scale":scale,"pixelIndex":[61,5],"rawRoundTrip":frame.local_to_pixel(raw),
                "centerRoundTrip":frame.local_to_pixel(frame.pixel_to_local(61,5).unwrap())}));
            assert!(candidate_available_in_frame(Some(&frame), target, color, 0, None, None).unwrap(),
                "an extracted matching pixel must survive its local-coordinate roundtrip at scale {scale}");
            let native = image_candidate_local(&frame, Point { x: 61.0, y: 5.0 }, false).unwrap();
            assert_eq!(frame.color_at(native), Some([color.r, color.g, color.b]));
            // Mac native delivery and blocked-point recovery call this same
            // conversion. Color sampling resolves to exactly that pixel.
            assert_eq!(frame.local_to_pixel(native), Some((61, 5)));
        }
    }

    fn geometry_frame() -> OwnedFrame {
        OwnedFrame { width: 128, height: 64, stride: 512, rgba: [240, 240, 240, 255].repeat(128 * 64),
            origin: macro_platform::Point { x: -1920.0, y: -1080.0 }, scale_x: 1.0, scale_y: 1.0,
            sequence: 1, captured_at_ns: 1, last_observed_ns: 1 }
    }

    #[test]
    fn p7_windows_input_keeps_integer_indices_and_avoids_double_centering() {
        let frame = geometry_frame(); // Windows capture always reports physical scale 1.
        let point = Point { x: 61.0, y: 5.0 };
        let sample = image_candidate_local(&frame, point, false).unwrap();
        let native = image_candidate_local(&frame, point, true).unwrap();
        assert_eq!(native.x, 61.0);
        assert_eq!(native.y, 5.0);
        assert_eq!(frame.local_to_pixel(sample), frame.local_to_pixel(native));
        // Match the existing Windows formula, not an OS/SendInput simulation.
        let encoded = |local: f64| (((frame.origin.x + local - (-1920.0) + 0.5) * 65536.0 / 4096.0).floor()) as i32;
        assert_eq!(encoded(native.x), 984);
        assert_eq!(encoded(sample.x), 992); // blindly sending the sample center would change the native target
        println!("{}", json!({"case":"windows_existing_integer_index_contract","nativePixelIndex":native.x,
            "samplePixelCenter":sample.x,"existingNormalization":encoded(native.x),"doubleCenteredNormalization":encoded(sample.x),
            "windowsOsExecuted":false}));
        let mut wide = frame.clone();
        wide.width = 4096; wide.stride = 4096 * 4;
        wide.rgba = [240, 240, 240, 255].repeat(4096 * 64);
        let last = Point { x: 4095.0, y: 5.0 };
        let last_native = image_candidate_local(&wide, last, true).unwrap();
        let last_sample = image_candidate_local(&wide, last, false).unwrap();
        assert_eq!(wide.local_to_pixel(last_native), wide.local_to_pixel(last_sample));
        assert_eq!(encoded(last_native.x), 65528);
        assert_eq!(encoded(last_sample.x), 65536); // actual driver range check would reject a double center
        assert!(encoded(last_native.x) < 65536);
    }

    #[test]
    fn p7_candidate_mapping_bounds_scales_negative_origins_and_fractional_compatibility() {
        let mut frame = geometry_frame();
        for scale in [1.0, 2.0, 1.75, 1.3, 1.751] {
            frame.scale_x = scale; frame.scale_y = scale;
            for (x, y) in [(0, 0), (127, 63), (61, 5)] {
                let local = image_candidate_local(&frame, Point { x: x as f64, y: y as f64 }, false).unwrap();
                assert_eq!(frame.local_to_pixel(local), Some((x, y)));
                assert!(frame.origin.x + local.x < 0.0 && frame.origin.y + local.y < 0.0);
            }
            let fractional = Point { x: 61.25, y: 5.75 };
            let local = image_candidate_local(&frame, fractional, false).unwrap();
            assert_eq!(local.x, fractional.x / scale);
            assert_eq!(local.y, fractional.y / scale);
        }
        for point in [Point { x: -1.0, y: 0.0 }, Point { x: 128.0, y: 0.0 }, Point { x: 0.0, y: 64.0 },
            Point { x: f64::NAN, y: 0.0 }, Point { x: 0.0, y: f64::INFINITY }] {
            assert!(image_candidate_local(&frame, point, false).is_err());
        }
        for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            frame.scale_x = scale;
            assert!(image_candidate_local(&frame, Point { x: 0.0, y: 0.0 }, false).is_err());
        }
    }

    #[test]
    fn p7_only_image_candidate_actions_select_centered_input_and_selected_color_uses_that_pixel() {
        let target = Target::Point { point: Point { x: 61.0, y: 5.0 } };
        for purpose in ["seat:pixel", "region:pixel"] {
            assert!(is_image_candidate_action(&ActionKind::Click { target: target.clone(), purpose: purpose.into() }));
        }
        for purpose in ["legacy_entry", "legacy_cancellation_region", "entry"] {
            assert!(!is_image_candidate_action(&ActionKind::Click { target: target.clone(), purpose: purpose.into() }));
        }
        assert!(!is_image_candidate_action(&ActionKind::Refresh { target: target.clone() }));
        assert!(!is_image_candidate_action(&ActionKind::CompleteSelection { target }));
        let mut frame = geometry_frame(); frame.scale_x = 1.75; frame.scale_y = 1.75;
        let selected = [50, 80, 90];
        frame.rgba[(5 * 128 + 61) * 4..(5 * 128 + 61) * 4 + 3].copy_from_slice(&selected);
        let local = image_candidate_local(&frame, Point { x: 61.0, y: 5.0 }, false).unwrap();
        assert_eq!(frame.color_at(local), Some(selected));
        assert_ne!(frame.color_at(macro_platform::Point { x: 61.0 / 1.75, y: 5.0 / 1.75 }), Some(selected));
    }
}

#[derive(Clone)]
struct RestartContext {
    engine: Engine,
    target_id: Option<u64>,
    geometry: Option<FrameReference>,
    run_id: u64,
    started_at: Instant,
    count_receipt: Option<(u64, u64, u64)>,
}

fn same_search_mapping(previous: &Engine, next: &Profile, mode: RunMode) -> bool {
    let region = if mode == RunMode::Cancellation {
        previous.snapshot().active_region.and_then(|index| next.legacy.regions.get(index))
    } else { None };
    same_candidate_interpretation(previous, next, region)
        && (mode != RunMode::Cancellation || previous.snapshot().active_region.is_none()
            || serde_json::to_value(&previous.profile().legacy.cancellation_navigation).ok()
                == serde_json::to_value(&next.legacy.cancellation_navigation).ok())
}

fn same_candidate_interpretation(previous: &Engine, next: &Profile,
    region: Option<&macro_core::LegacyRegionSettings>) -> bool {
    let vision = region.and_then(|region| region.vision.as_ref()).unwrap_or(&next.vision);
    let color = region.and_then(|region| region.color).unwrap_or(next.color);
    serde_json::to_value(previous.effective_vision()).ok() == serde_json::to_value(vision).ok()
        && previous.effective_color() == color && previous.profile().color_tolerance == next.color_tolerance
        && previous.profile().features.nol_selection_count == next.features.nol_selection_count
        && previous.profile().features.seat_objects == next.features.seat_objects
}

fn same_post_entry_mapping(previous: &Engine, next: &Profile) -> bool {
    // A delivered selection stays in its current cancellation region even when
    // F9 changes the run mode to Normal. Do not silently switch to global ROI.
    let region = previous.snapshot().active_region.and_then(|index| next.legacy.regions.get(index));
    same_candidate_interpretation(previous, next, region)
}

struct Worker {
    app: AppHandle,
    path: PathBuf,
    origin: Instant,
    raw_profile: Value,
    profile_revision: u64,
    profile: Profile,
    desktop: DesktopConfig,
    engine: Engine,
    restart_context: Option<RestartContext>,
    native: Result<NativeAdapter, String>,
    permissions: Option<macro_platform::Permissions>,
    frame: Option<OwnedFrame>,
    last_sequence: Option<u64>,
    last_scan_generation: Option<u64>,
    last_input_sequence: Option<u64>,
    last_input_started_at_ns: Option<u64>,
    /// Run/action/native timestamp of the most recent delivered input. A count
    /// request may never consume a frame from before its originating click.
    selection_count_after_input: Option<(u64, u64, u64)>,
    next_selection_count_ms: u64,
    selection_count_poll_id: u64,
    selection_count_input_proof: Option<SelectionCountInputProof>,
    selection_count_read_cache: Option<SelectionCountReadCache>,
    seat_frame_after: Option<u64>,
    seat_frame_after_ns: Option<u64>,
    refresh_frame_after: Option<u64>,
    refresh_frame_after_ns: Option<u64>,
    seat_rescan_requested: bool,
    target_recovery: Option<TargetRecovery>,
    visual_recovery: VisualRecovery,
    enter_watch_token: Option<u64>,
    next_enter_watch_token: u64,
    enter_watch_error: Option<String>,
    captcha_input_watch_token: Option<u64>,
    next_captcha_input_watch_token: u64,
    captcha_input_watch_error: Option<String>,
    captcha_input_diagnostic_error: Option<String>,
    pending_captcha_inputs: VecDeque<PendingCaptchaInput>,
    keyboard_check: KeyboardCheck,
    keyboard_check_token: Option<u64>,
    next_keyboard_health_ms: u64,
    next_keyboard_log_ms: u64,
    nol_captcha: NolCaptchaDetector,
    next_nol_observation_ms: u64,
    last_nol_log_ms: Option<u64>,
    nol_handoff_pending: bool,
    enter_diagnostic_error: Option<String>,
    vision_diagnostics: Option<Value>,
    last_vision_log_ms: u64,
    candidates: HashMap<String, Candidate>,
    actions: VecDeque<Action>,
    scope: String,
    runtime_message: Option<String>,
    clock: Option<ClockEstimate>,
    interrupt: Arc<AtomicBool>,
    start_shortcut: StartShortcutGate,
    shortcut_epoch: Arc<AtomicU64>,
    run_shortcut_epoch: Option<u64>,
    run_id: u64,
    run_started_at: Instant,
    revision: u64,
    last_published: String,
    shortcut_errors: Vec<String>,
    capture_field: String,
    capture_armed: bool,
    capture_state: CaptureState,
    capture_error: Option<String>,
    overlay_enabled: bool,
    overlay_visible: bool,
    feedback_until_ms: u64,
    overlay_error: Option<String>,
    shutting_down: bool,
    timings: VecDeque<Value>,
}

impl Worker {
    fn new(app: AppHandle, path: PathBuf, interrupt: Arc<AtomicBool>, shortcut_epoch: Arc<AtomicU64>) -> Self {
        let default = Profile::default();
        let mut load_error = None;
        let raw = match storage::load(&path) {
            Ok(Some(value)) => value,
            Ok(None) => serde_json::to_value(&default).unwrap(),
            Err(error) => { load_error = Some(error); serde_json::to_value(&default).unwrap() }
        };
        let profile = match Profile::from_json(&raw.to_string()) {
            Ok(p) => p,
            Err(error) => { load_error = Some(format!("설정 불러오기 실패: {error}")); default }
        };
        let desktop = serde_json::from_value(raw.get("desktop").cloned().unwrap_or(json!({})))
            .unwrap_or_default();
        let native = NativeAdapter::new().map_err(|e| e.to_string());
        let permissions = native.as_ref().ok().map(NativeAdapter::permissions);
        Self {
            app, path, origin: Instant::now(),
            engine: Engine::new(profile.clone()).unwrap(), restart_context: None,
            overlay_enabled: profile.features.status_overlay,
            raw_profile: raw, profile_revision: 0, profile, desktop,
            native, permissions,
            frame: None, last_sequence: None, last_scan_generation: None, last_input_sequence: None, last_input_started_at_ns: None,
            selection_count_after_input: None, next_selection_count_ms: 0, selection_count_poll_id: 0,
            selection_count_input_proof: None,
            selection_count_read_cache: None,
            seat_frame_after: None, seat_frame_after_ns: None, refresh_frame_after: None, refresh_frame_after_ns: None,
            target_recovery: None,
            visual_recovery: VisualRecovery::default(),
            enter_watch_token: None, next_enter_watch_token: 0, enter_watch_error: None,
            captcha_input_watch_token: None, next_captcha_input_watch_token: 0,
            captcha_input_watch_error: None, captcha_input_diagnostic_error: None, pending_captcha_inputs: VecDeque::new(),
            keyboard_check: KeyboardCheck::default(), keyboard_check_token: None,
            next_keyboard_health_ms: 0, next_keyboard_log_ms: 0,
            nol_captcha: NolCaptchaDetector::default(), next_nol_observation_ms: 0,
            last_nol_log_ms: None,
            nol_handoff_pending: false,
            enter_diagnostic_error: None,
            seat_rescan_requested: false,
            vision_diagnostics: None, last_vision_log_ms: 0,
            candidates: HashMap::new(), actions: VecDeque::new(), scope: String::new(),
            runtime_message: load_error, clock: None, interrupt,
            start_shortcut: StartShortcutGate::default(), shortcut_epoch, run_shortcut_epoch: None,
            run_id: 0, run_started_at: Instant::now(), revision: 0, last_published: String::new(),
            shortcut_errors: vec![], capture_field: String::new(), capture_armed: false,
            capture_state: CaptureState::Idle, capture_error: None,
            shutting_down: false, timings: VecDeque::new(),
            overlay_error: None, overlay_visible: false, feedback_until_ms: 0,
        }
    }

    fn now(&self) -> u64 { self.origin.elapsed().as_millis() as u64 }
    fn adapter(&mut self) -> Result<&mut NativeAdapter, String> {
        self.native.as_mut().map_err(|s| s.clone())
    }
    fn active(&self) -> bool {
        !matches!(self.engine.snapshot().phase,
            Phase::Idle | Phase::Stopped | Phase::Faulted | Phase::Completed | Phase::Paused)
    }
    fn editable(&self) -> bool {
        matches!(self.engine.snapshot().phase, Phase::Idle | Phase::Stopped | Phase::Faulted | Phase::Completed)
    }

    fn record(&self, kind: &str, detail: Value) {
        let Some(parent) = self.path.parent() else { return; };
        let path = parent.join("runtime-events.jsonl");
        let _ = fs::create_dir_all(parent);
        let truncate = fs::metadata(&path).is_ok_and(|m| m.len() > 2 * 1024 * 1024);
        if let Ok(mut file) = OpenOptions::new().create(true).write(true)
            .append(!truncate).truncate(truncate).open(path) {
            let value = json!({"timeMs":clock::unix_ms(),"elapsedMs":self.now(),
                "appVersion":env!("CARGO_PKG_VERSION"),"processId":std::process::id(),
                "runId":self.run_id,"kind":kind,"detail":detail});
            let _ = writeln!(file, "{value}");
        }
    }

    fn sync_overlay_visibility(&mut self) {
        let visible = self.overlay_error.is_none()
            && (self.overlay_enabled || self.capture_armed || self.keyboard_check.active() || self.now() < self.feedback_until_ms);
        if visible == self.overlay_visible { return; }
        self.overlay_visible = visible;
        let app = self.app.clone();
        let _ = self.app.run_on_main_thread(move || {
            if let Some(window) = app.get_webview_window("status") {
                let _ = if visible { window.show() } else { window.hide() };
            }
        });
    }

    fn report_error(&mut self, message: String) {
        let capture_field = if self.capture_state == CaptureState::Reading {
            self.capture_state = CaptureState::Error;
            self.capture_error = Some(message.clone());
            Some(self.capture_field.clone())
        } else { None };
        self.record("error", json!({"message":message,"captureField":capture_field}));
        self.runtime_message = Some(message.clone());
        self.feedback_until_ms = self.now() + 10_000;
        self.sync_overlay_visibility();
        let _ = self.app.emit("runtime-error", json!({"message":message,"captureField":capture_field}));
    }

    fn run(mut self, rx: mpsc::Receiver<Request>) {
        self.publish();
        while !self.shutting_down {
            match rx.recv_timeout(Duration::from_millis(if self.active() { 4 } else { 20 })) {
                Ok(request) => self.handle(request),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            // Controls are drained before input. A pending stop also guards each click.
            while let Ok(request) = rx.try_recv() { self.handle(request); }
            if let Some(mode) = self.start_shortcut.expire(self.now(), self.shortcut_epoch.load(Ordering::SeqCst)) {
                self.report_error(format!("{} 시작 실패 · 설정 화면 응답이 늦었습니다. 앱 설정 화면을 확인한 뒤 예매창에서 {} 키를 다시 누르세요.",
                    start_shortcut_label(mode), start_shortcut_key(mode)));
            }
            self.sync_overlay_visibility();
            if self.keyboard_check.active() {
                // This branch deliberately does not observe pixels, feed the
                // booking engine, or execute any macro action.
                self.poll_keyboard_check();
                self.publish();
                continue;
            }
            self.recover_target();
            self.sync_enter_watch();
            self.sync_captcha_input_watch();
            if self.active() && !self.interrupt.load(Ordering::SeqCst) {
                // A repeated CAPTCHA submission supersedes queued map/seat input.
                if !self.poll_captcha_enter() && !self.poll_captcha_input() {
                    self.observe_frame();
                    self.rearm_cancelled_visual_escape();
                    self.feed(Event::Tick);
                    self.execute_actions();
                }
            }
            self.publish();
        }
        self.deactivate_enter_watch();
        self.finish_keyboard_check(true, None);
        self.deactivate_captcha_input_watch();
        self.actions.clear();
    }

    fn deactivate_enter_watch(&mut self) {
        if self.enter_watch_token.take().is_none() { return; }
        if let Ok(adapter) = &mut self.native {
            if let Err(error) = adapter.set_enter_watch(None) {
                // Local token is already invalid; queued callbacks cannot act.
                self.record("enter_watch_stop_error", json!({"error":error.to_string()}));
            }
        }
    }

    fn keyboard_environment(&mut self) -> Result<Value, String> {
        let status = self.adapter()?.keyboard_input_status().map_err(|error| error.to_string())?;
        serde_json::to_value(status).map_err(|error| error.to_string())
    }

    fn start_keyboard_check(&mut self, received_epoch: u64) -> Result<Value, String> {
        let preflight = keyboard_check::validate_start(self.engine.snapshot().phase, self.capture_armed,
            self.native.as_ref().ok().and_then(NativeAdapter::target).is_some());
        let epoch = prepare_keyboard_check_controls(preflight, self.keyboard_check.active(), received_epoch,
            &mut self.start_shortcut, &mut self.run_shortcut_epoch, &self.shortcut_epoch, &self.interrupt)?;
        self.deactivate_enter_watch();
        self.deactivate_captcha_input_watch();
        let environment = match self.keyboard_environment() {
            Ok(environment) => environment,
            Err(error) => {
                self.keyboard_check.start(self.now(), Value::Null);
                self.keyboard_check.error(format!("키보드 권한 상태를 읽지 못했습니다. {error}"));
                return Err(self.keyboard_check.message().into());
            }
        };
        self.keyboard_check.start(self.now(), environment.clone());
        if self.shortcut_epoch.load(Ordering::SeqCst) != epoch || self.interrupt.load(Ordering::SeqCst) {
            self.finish_keyboard_check(true, None);
            return Err("키보드 검사 시작 요청이 취소되었습니다.".into());
        }
        self.next_captcha_input_watch_token = self.next_captcha_input_watch_token.wrapping_add(1).max(1);
        let token = self.next_captcha_input_watch_token;
        // A separate owner uses the production listener, never the CAPTCHA
        // detector. Old diagnostic records must not count toward this check.
        self.keyboard_check_token = Some(token);
        let result = self.adapter().and_then(|adapter| {
            adapter.take_captcha_input_diagnostics().map_err(|error| error.to_string())?;
            adapter.set_captcha_input_watch(Some(token)).map_err(|error| error.to_string())
        });
        if let Err(error) = result {
            let message = format!("키보드 수신 검사를 시작하지 못했습니다. {error}");
            self.finish_keyboard_check(false, Some(message.clone()));
            return Err(message);
        }
        if self.shortcut_epoch.load(Ordering::SeqCst) != epoch || self.interrupt.load(Ordering::SeqCst) {
            self.finish_keyboard_check(true, None);
            return Err("키보드 검사 시작 요청이 취소되었습니다.".into());
        }
        let now = self.now();
        self.keyboard_check.start(now, environment);
        self.next_keyboard_health_ms = now + 1000;
        self.next_keyboard_log_ms = now + 1000;
        self.feedback_until_ms = now + keyboard_check::KEYBOARD_CHECK_DURATION_MS + 8000;
        self.runtime_message = Some(self.keyboard_check.message().into());
        self.record("keyboard_check_started", json!({"token":token,"check":self.keyboard_check.snapshot(now)}));
        self.sync_overlay_visibility();
        Ok(self.snapshot_value())
    }

    fn finish_keyboard_check(&mut self, cancelled: bool, mut error: Option<String>) {
        if !self.keyboard_check.active() && self.keyboard_check_token.is_none() { return; }
        let token = self.keyboard_check_token.take();
        if token.is_some() {
            if let Err(stop_error) = self.adapter().and_then(|adapter|
                adapter.set_captcha_input_watch(None).map_err(|error| error.to_string())) {
                error = Some(format!("키보드 감시 종료 실패 · {stop_error}"));
            }
        }
        match self.keyboard_environment() {
            Ok(environment) => { self.keyboard_check.environment(environment); }
            Err(health_error) => {
                if error.is_none() { error = Some(format!("키보드 권한 상태 조회 실패 · {health_error}")); }
            }
        }
        if let Some(error) = error { self.keyboard_check.error(error); }
        else if cancelled { self.keyboard_check.cancel(); }
        else { self.keyboard_check.finish(); }
        self.runtime_message = Some(self.keyboard_check.message().into());
        self.feedback_until_ms = self.now() + 8000;
        self.record("keyboard_check_finished", json!({"token":token,"check":self.keyboard_check.snapshot(self.now())}));
    }

    fn poll_keyboard_check(&mut self) {
        let Some(token) = self.keyboard_check_token else {
            self.finish_keyboard_check(false, Some("키보드 수신 검사에 활성 감시가 없습니다.".into()));
            return;
        };
        if self.interrupt.load(Ordering::SeqCst) {
            self.finish_keyboard_check(true, None);
            return;
        }
        let now = self.now();
        // Drain the last native batch before ending the check. A character
        // arriving just before the deadline must not be discarded as no input.
        let expired = self.keyboard_check.expired(now);
        if now >= self.next_keyboard_health_ms {
            self.next_keyboard_health_ms = now + 1000;
            match self.keyboard_environment() {
                Ok(environment) => {
                    if self.keyboard_check.environment(environment) {
                        self.record("keyboard_check_environment", json!({"token":token,
                            "environment":self.keyboard_check.snapshot(now)["environment"]}));
                    }
                }
                Err(error) => {
                    self.finish_keyboard_check(false, Some(format!("키보드 권한 상태 조회 실패 · {error}")));
                    return;
                }
            }
        }
        let result = self.adapter().and_then(|adapter| adapter.take_captcha_inputs().map_err(|error| error.to_string()));
        let diagnostics = self.adapter().and_then(|adapter|
            adapter.take_captcha_input_diagnostics().map_err(|error| error.to_string()));
        match diagnostics {
            Ok(diagnostics) => {
                for diagnostic in diagnostics { self.keyboard_check.diagnostic(&diagnostic, token); }
            }
            Err(error) => {
                self.finish_keyboard_check(false, Some(format!("키보드 입력 진단 조회 실패 · {error}")));
                return;
            }
        }
        let presses = match result {
            Ok(presses) => presses,
            Err(error) => {
                self.finish_keyboard_check(false, Some(format!("키보드 입력 조회 실패 · {error}")));
                return;
            }
        };
        let target = self.native.as_ref().ok().and_then(NativeAdapter::target).cloned();
        for press in presses {
            self.keyboard_check.received();
            let rejection = target.as_ref().map_or(Some("target_unavailable"), |target|
                enter_target_rejection(&press, token, target));
            if let Some(reason) = rejection {
                self.keyboard_check.reject(&format!("runtime:{reason}"));
                continue;
            }
            // No activation, pointer input or capture: verify only the current
            // bound foreground window using the same production guard.
            match self.adapter().and_then(|adapter| adapter.validate_target().map_err(|error| error.to_string())) {
                Ok(()) => self.keyboard_check.accept(),
                Err(error) => {
                    let code = error.strip_prefix('[').and_then(|error| error.split_once(']'))
                        .map_or("target_validation_failed", |(code, _)| code);
                    self.keyboard_check.reject(&format!("runtime:{code}"));
                }
            }
        }
        self.runtime_message = Some(self.keyboard_check.message().into());
        if expired {
            self.finish_keyboard_check(false, None);
            return;
        }
        if now >= self.next_keyboard_log_ms {
            self.next_keyboard_log_ms = now + 1000;
            self.record("keyboard_check_progress", json!({"token":token,"check":self.keyboard_check.snapshot(now)}));
        }
    }

    fn sync_enter_watch(&mut self) {
        if !self.engine.accepts_captcha_enter() || self.capture_armed
            || self.interrupt.load(Ordering::SeqCst) {
            self.deactivate_enter_watch();
            return;
        }
        if self.enter_watch_token.is_some() { return; }
        self.next_enter_watch_token = self.next_enter_watch_token.wrapping_add(1).max(1);
        let token = self.next_enter_watch_token;
        match self.adapter().and_then(|adapter| adapter.set_enter_watch(Some(token)).map_err(|e| e.to_string())) {
            Ok(()) => {
                self.enter_watch_token = Some(token);
                self.enter_watch_error = None;
                self.record("enter_watch_started", json!({"token":token}));
            }
            Err(reason) => {
                self.enter_watch_error = Some(reason.clone());
                self.pause_for_context(format!("캡챠 Enter 감지를 시작하지 못했습니다. {reason}"), None);
            }
        }
    }

    fn deactivate_captcha_input_watch(&mut self) {
        self.pending_captcha_inputs.clear();
        if self.captcha_input_watch_token.take().is_none() { return; }
        if let Ok(adapter) = &mut self.native {
            if let Err(error) = adapter.set_captcha_input_watch(None) {
                self.record("captcha_input_watch_stop_error", json!({"error":error.to_string()}));
            }
        }
    }

    fn sync_captcha_input_watch(&mut self) {
        if self.keyboard_check.active() { return; }
        if !captcha_input_watch_needed(&self.engine, &self.nol_captcha,
            self.capture_armed, self.interrupt.load(Ordering::SeqCst)) {
            self.deactivate_captcha_input_watch();
            return;
        }
        if self.captcha_input_watch_token.is_some() { return; }
        self.next_captcha_input_watch_token = self.next_captcha_input_watch_token.wrapping_add(1).max(1);
        let token = self.next_captcha_input_watch_token;
        match self.adapter().and_then(|adapter| adapter.set_captcha_input_watch(Some(token)).map_err(|e| e.to_string())) {
            Ok(()) => {
                self.captcha_input_watch_token = Some(token);
                self.captcha_input_watch_error = None;
                self.record("captcha_input_watch_started", json!({"token":token}));
            }
            Err(reason) => {
                self.captcha_input_watch_error = Some(reason.clone());
                self.pause_for_context(format!("NOL 캡챠 판단 · 첫 입력 감지를 시작하지 못했습니다. {reason}"), None);
            }
        }
    }

    /// Typing only stores a reference. It never emits CAPTCHA completion,
    /// queues a click, or replaces the reference after later letters/retries.
    fn poll_captcha_input(&mut self) -> bool {
        let Some(token) = self.captcha_input_watch_token else { return false; };
        if !captcha_input_watch_needed(&self.engine, &self.nol_captcha,
            self.capture_armed, self.interrupt.load(Ordering::SeqCst)) || self.shortcut_start_cancelled() {
            return false;
        }
        if self.pending_captcha_inputs.is_empty() {
            let result = self.adapter().and_then(|adapter| adapter.take_captcha_inputs().map_err(|e| e.to_string()));
            // Take signals before diagnostics so queue expiration reasons from
            // this same poll are included. Diagnostic failure never changes the
            // input acceptance policy or releases the CAPTCHA gate.
            if let Ok(adapter) = &mut self.native {
                match adapter.take_captcha_input_diagnostics() {
                    Ok(diagnostics) => {
                        self.captcha_input_diagnostic_error = None;
                        for diagnostic in diagnostics {
                            self.record("captcha_input_diagnostic", json!(diagnostic));
                        }
                    }
                    Err(error) => {
                        let error = error.to_string();
                        if self.captcha_input_diagnostic_error.as_deref() != Some(&error) {
                            self.record("captcha_input_diagnostic_error", json!({"error":error}));
                        }
                        self.captcha_input_diagnostic_error = Some(error);
                    }
                }
            }
            let presses = match result {
                Ok(presses) => presses,
                Err(reason) => {
                    self.captcha_input_watch_error = Some(reason.clone());
                    self.pause_for_context(format!("NOL 캡챠 판단 · 첫 입력 감지 오류 · {reason}"), None);
                    return true;
                }
            };
            let target = self.native.as_ref().ok().and_then(NativeAdapter::target).cloned();
            // Queues contain metadata only. Retain the batch in arrival order:
            // a late first input must not discard a usable later notification.
            self.pending_captcha_inputs = select_captcha_inputs(presses, token, target.as_ref(), Instant::now(), |detail|
                self.record("captcha_input_signal_received", detail));
            for pending in &self.pending_captcha_inputs {
                let press = pending.press;
                self.record("captcha_input_received", json!({"token":token,"observedAtNs":press.observed_at_ns}));
            }
        }
        if self.pending_captcha_inputs.is_empty() { return false; }
        let sequence = self.frame.as_ref().map_or(0, |frame| frame.sequence);
        // capture_observation validates the bound foreground window, even when
        // the native stream reports unchanged pixels after the input.
        let result = self.adapter().and_then(|adapter|
            adapter.capture_observation(sequence).map_err(|error| error.to_string()))
            .and_then(|observation| apply_frame_observation(&mut self.frame, observation));
        if let Err(reason) = result {
            self.pause_for_context(reason, None);
            return true;
        }
        if self.interrupt.load(Ordering::SeqCst) || self.shortcut_start_cancelled() { return true; }
        let frame = self.frame.as_ref().unwrap();
        if self.desktop.capture_reference.as_ref().is_some_and(|reference| !same_frame_reference(reference, frame)) {
            self.pause_for_context("화면 크기나 배율이 채집할 때와 달라졌습니다. 현재 화면에서 좌표를 다시 채집하세요.".into(), None);
            return true;
        }
        let mut rejected = Vec::new();
        let candidate = pending_captcha_input_candidate(&mut self.pending_captcha_inputs, frame, Instant::now(),
            |press, reason| rejected.push((press, reason)));
        for (press, reason) in rejected {
            self.record("captcha_input_reference_rejected", json!({"reason":reason,"observedAtNs":press.observed_at_ns}));
        }
        let press = match candidate {
            CaptchaInputCandidate::WaitingForFrame => {
                self.runtime_message = Some("NOL 캡챠 판단 · 첫 입력 수신 · 기준 화면 읽는 중".into());
                return true;
            }
            CaptchaInputCandidate::Empty => {
                self.runtime_message = Some("NOL 캡챠 판단 · 입력 시점의 화면을 놓쳤습니다 · 다음 문자 입력에 다시 채집".into());
                return true;
            }
            CaptchaInputCandidate::Ready(press) => press,
        };
        if !frame_passes_input_transition(frame, self.seat_frame_after, self.seat_frame_after_ns) { return true; }
        let checked_at_ms = self.now();
        let frame = self.frame.as_ref().unwrap();
        let result = self.nol_captcha.arm_on_input(macro_core::vision::RgbaView {
            width: frame.width as u32, height: frame.height as u32, stride: frame.stride, rgba: &frame.rgba,
        }, frame.sequence, frame.captured_at_ns, frame.last_observed_ns, checked_at_ms);
        self.pending_captcha_inputs.pop_front();
        match result {
            Ok(observation) => {
                self.record("captcha_input_reference", json!({
                    "observation":observation,"observedAtNs":press.observed_at_ns,
                }));
                if self.nol_captcha.has_reference() {
                    self.deactivate_captcha_input_watch();
                    self.runtime_message = Some(nol_captcha_message(NolCaptchaStatus::Visible).into());
                } else {
                    self.runtime_message = Some("NOL 캡챠 판단 · 유효한 기준 화면 대기 · 다음 문자 입력에 다시 채집".into());
                }
            }
            Err(reason) => {
                self.runtime_message = Some(format!("NOL 캡챠 판단 · 기준 색 채집 실패 · {reason}"));
            }
        }
        // Even a malformed completion flag from the detector cannot advance the
        // engine here. Only a subsequent background observation can do that.
        true
    }

    /// Native listeners observe Enter without consuming or replaying it. The
    /// queue expires old events; this layer checks run token, app/window and
    /// fresh target focus before replacing any pending macro action.
    fn poll_captcha_enter(&mut self) -> bool {
        let Some(token) = self.enter_watch_token else { return false; };
        if !self.engine.accepts_captcha_enter() || self.interrupt.load(Ordering::SeqCst) { return false; }
        let result = self.adapter().and_then(|adapter| adapter.take_enter_presses().map_err(|e| e.to_string()));
        // Drain only Enter metadata, including callback/queue rejections. This
        // adds no input, text collection, retry delay, or acceptance exception.
        if let Ok(adapter) = &mut self.native {
            match adapter.take_enter_diagnostics() {
                Ok(diagnostics) => {
                    self.enter_diagnostic_error = None;
                    for diagnostic in diagnostics {
                        if diagnostic.token == Some(token) {
                            if let Some(message) = enter_rejection_message(&diagnostic.reason) {
                                self.runtime_message = Some(message.into());
                                self.feedback_until_ms = self.now() + 8000;
                            }
                        }
                        self.record("enter_diagnostic", json!(diagnostic));
                    }
                }
                Err(error) => {
                    let error = error.to_string();
                    if self.enter_diagnostic_error.as_deref() != Some(&error) {
                        self.record("enter_diagnostic_error", json!({"error":error}));
                    }
                    self.enter_diagnostic_error = Some(error);
                }
            }
        }
        let presses = match result {
            Ok(presses) => presses,
            Err(reason) => {
                self.enter_watch_error = Some(reason.clone());
                self.pause_for_context(format!("캡챠 Enter 감지 오류 · {reason}"), None);
                return true;
            }
        };
        let Some(target) = self.native.as_ref().ok().and_then(NativeAdapter::target).cloned() else { return false; };
        for press in &presses {
            let reason = enter_target_rejection(press, token, &target);
            self.record("enter_signal_received", json!({
                "signal":press, "acceptedTarget":reason.is_none(), "reason":reason,
                "expectedToken":token, "boundPID":target.pid, "boundWindowID":target.id,
            }));
            if press.token == token {
                if let Some(message) = reason.and_then(enter_rejection_message) {
                    self.runtime_message = Some(message.into());
                    self.feedback_until_ms = self.now() + 8000;
                }
            }
        }
        let Some(press) = presses.into_iter().rev().find(|press| enter_target_rejection(press, token, &target).is_none()) else {
            return false;
        };
        if let Err(reason) = self.adapter().and_then(|adapter| adapter.validate_target().map_err(|e| e.to_string())) {
            self.record("enter_target_validation_failed", json!({"signal":press, "error":reason}));
            self.runtime_message = Some(format!("Enter 수신 · 연결한 예매창에서 다시 입력하세요. {reason}"));
            self.feedback_until_ms = self.now() + 8000;
            return true;
        }
        if self.interrupt.load(Ordering::SeqCst) { return true; }
        // The Enter may be a corrected CAPTCHA submission. Do not replay the
        // initial booking button or retain candidates from the previous attempt.
        let after = self.frame.as_ref().map(|frame| frame.sequence).or(self.last_sequence);
        self.cancel_queued_actions();
        self.visual_recovery.reset();
        self.target_recovery = None;
        self.frame = None;
        self.last_sequence = None;
        self.last_scan_generation = None;
        self.last_input_sequence = None;
        self.last_input_started_at_ns = None;
        self.seat_frame_after = after;
        self.seat_frame_after_ns = Some(press.observed_at_ns);
        self.refresh_frame_after = None;
        self.refresh_frame_after_ns = None;
        self.seat_rescan_requested = false;
        self.candidates.clear();
        self.vision_diagnostics = None;
        self.runtime_message = None;
        self.feed(Event::CaptchaSubmitted);
        self.feedback_until_ms = self.now() + 8000;
        self.record("captcha_enter_submitted", json!({
            "token":token, "generation":self.engine.snapshot().generation,
            "observedAtNs":press.observed_at_ns,
        }));
        true
    }

    fn handle(&mut self, request: Request) {
        if !control_request_current(&request, self.shortcut_epoch.load(Ordering::SeqCst)) {
            if let Some(reply) = request.reply { let _ = reply.send(Ok(self.cancelled_profile_reply())); }
            return;
        }
        // Includes F1/F2/F5, F8 and F9, which otherwise return before command().
        if self.keyboard_check.active() && request.command.cancels_keyboard_check() {
            self.finish_keyboard_check(true, None);
        }
        if request.command.cancels_start_shortcut() {
            self.start_shortcut.pending = None;
            self.run_shortcut_epoch = Some(request.shortcut_epoch);
        }
        if request.command.is_execution_request() {
            self.supersede_execution(matches!(request.command, Command::ContinueAfterCaptcha));
        }
        if matches!(request.command, Command::StartKeyboardCheck) {
            let result = self.start_keyboard_check(request.shortcut_epoch);
            if let Err(message) = &result { self.report_error(message.clone()); }
            if let Some(reply) = request.reply { let _ = reply.send(result); }
            return;
        }
        if let Command::PrepareShortcut(mode) = &request.command {
            let result = self.prepare_start_shortcut(*mode, request.shortcut_epoch);
            let result = self.current_execution_result(result, request.shortcut_epoch);
            if let Err(message) = &result { self.report_error(message.clone()); }
            if let Some(reply) = request.reply { let _ = reply.send(result); }
            return;
        }
        if let Command::StartFromUi { mode, profile, expected_profile_revision } = request.command {
            let result = self.start_from_ui(mode, profile, expected_profile_revision, request.shortcut_epoch);
            let result = self.current_execution_result(result, request.shortcut_epoch);
            if let Err(message) = &result { self.report_error(message.clone()); }
            if let Some(reply) = request.reply { let _ = reply.send(result); }
            return;
        }
        if matches!(request.command, Command::ContinueAfterCaptcha) {
            let result = self.continue_after_captcha(request.shortcut_epoch);
            let result = self.current_execution_result(result, request.shortcut_epoch);
            if let Err(message) = &result { self.report_error(message.clone()); }
            if let Some(reply) = request.reply { let _ = reply.send(result); }
            return;
        }
        if matches!(request.command, Command::CaptureArmed) {
            self.deactivate_enter_watch();
            self.target_recovery = None;
            self.record("capture_hotkey", json!({"armed":self.capture_armed,"field":self.capture_field}));
            self.interrupt.store(false, Ordering::SeqCst);
            if !self.capture_armed {
                let message = "F8 수신 · 앱에서 지정할 항목의 ‘F8로 지정’을 먼저 누르세요.".to_string();
                self.report_error(message.clone());
                if let Some(reply) = request.reply { let _ = reply.send(Err(message)); }
                return;
            }
            if self.active() {
                self.cancel_queued_actions();
                self.feed(Event::Pause);
            }
            let field = self.capture_field.clone();
            self.capture_state = CaptureState::Reading;
            self.capture_error = None;
            self.runtime_message = Some(format!("F8 수신 · {} 읽는 중", capture_label(&field).unwrap_or_else(|_| field.clone())));
            self.feedback_until_ms = self.now() + 8000;
            self.sync_overlay_visibility();
            self.publish();
            // F8 captures immediately on this worker. There is no timer queue
            // that can consume, defer or cancel a second capture request.
            let result = self.capture_point(&field, request.shortcut_epoch);
            if request.shortcut_epoch != self.shortcut_epoch.load(Ordering::SeqCst) {
                if let Some(reply) = request.reply { let _ = reply.send(Ok(self.cancelled_profile_reply())); }
                return;
            }
            if let Err(error) = &result { self.report_error(error.clone()); }
            if let Ok(value) = &result {
                self.capture_armed = false;
                self.capture_state = CaptureState::Captured;
                self.capture_error = None;
                self.feedback_until_ms = self.now() + 5000;
                if let Ok(adapter) = &self.native { let _ = adapter.beep(); }
                let _ = self.app.emit("point-captured", value);
            }
            if let Some(reply) = request.reply { let _ = reply.send(result); }
            return;
        }
        let start_response = matches!(request.command, Command::ApplyShortcut { .. });
        let result = self.command(request.command);
        let result = if start_response { self.current_execution_result(result, request.shortcut_epoch) } else { result };
        if let Err(message) = &result { self.report_error(message.clone()); }
        if let Some(reply) = request.reply { let _ = reply.send(result); }
    }

    fn command(&mut self, command: Command) -> Result<Value, String> {
        match command {
            Command::StartKeyboardCheck => unreachable!("keyboard check retains its received control epoch"),
            Command::CancelExpiredStart { epoch } => {
                // A new F1/F2/F5 can share the invalidated epoch. Cleanup owns
                // only a run actually created by the original timed-out UI start.
                if !expired_start_owns_run(epoch, self.shortcut_epoch.load(Ordering::SeqCst), self.run_shortcut_epoch) {
                    return Ok(json!({"cancelled":true}));
                }
                self.run_shortcut_epoch = None;
                self.command(Command::Stop)?;
                self.runtime_message = Some("시작 요청 응답 시간이 지나 실행을 취소했습니다. 설정을 확인한 뒤 다시 시작하세요.".into());
                self.record("start_response_expired", json!({"epoch":epoch}));
                Ok(self.snapshot_value())
            }

            Command::StopKeyboardCheck => {
                self.finish_keyboard_check(true, None);
                Ok(self.snapshot_value())
            }
            Command::Bootstrap => {
                self.permissions = self.native.as_ref().ok().map(NativeAdapter::permissions);
                Ok(json!({
                "profile": self.raw_profile, "profileRevision": self.profile_revision, "snapshot": self.snapshot_value(),
                "capabilities": {
                    "native": self.native.is_ok(), "browserObserver": false,
                    "automaticCaptchaDetection": false, "automaticConflictDetection": false,
                    "visualDialogDetection": true,
                    "automaticEntry": false, "clock": true,
                    "captchaEnter": true, "nolCaptchaDetection": true,
                    "message": "일반 예매에서 NOL 캡챠 판단 또는 캡챠 Enter 진행을 선택할 수 있습니다. 둘 다 켜면 NOL 판단이 우선합니다. 새로고침 버튼은 취소표에서만 사용합니다."
                }
                }))
            }
            Command::Windows => {
                let windows = self.adapter()?.windows().map_err(|e| e.to_string())?;
                Ok(serde_json::to_value(windows).map_err(|e| e.to_string())?)
            }
            Command::Bind(id) => {
                if self.active() { return Err("브라우저 연결을 변경하려면 먼저 일시정지하세요.".into()); }
                self.deactivate_enter_watch();
                self.deactivate_captcha_input_watch();
                self.captcha_input_watch_error = None;
                self.target_recovery = None;
                self.nol_captcha.reset();
                self.nol_handoff_pending = false;
                self.cancel_capture("브라우저 연결 변경으로 좌표 채집을 취소했습니다.");
                let adapter = self.adapter()?;
                let target = if let Some(id) = id {
                    adapter.windows().map_err(|e| e.to_string())?.into_iter()
                        .find(|w| w.id == id).ok_or("선택한 브라우저 창을 찾을 수 없습니다.")?
                } else { adapter.foreground_window().map_err(|e| e.to_string())? };
                let changed_context = binding_changes_context(adapter.target(), &target);
                adapter.bind(target.clone()).map_err(|e| e.to_string())?;
                reset_engine_after_rebind(&mut self.engine, &self.profile, changed_context)?;
                if changed_context {
                    self.restart_context = None;
                    self.visual_recovery.reset();
                    self.actions.clear();
                    self.selection_count_input_proof = None;
                    self.selection_count_read_cache = None;
                    self.selection_count_after_input = None;
                    self.run_shortcut_epoch = None;
                    self.run_id = self.run_id.saturating_add(1);
                    self.run_started_at = Instant::now();
                } else {
                    self.visual_recovery.same_target_stream_restarted();
                }
                self.frame = None;
                self.last_sequence = None;
                self.last_scan_generation = None;
                self.last_input_sequence = None;
                self.last_input_started_at_ns = None;
                self.seat_frame_after = None;
                self.seat_frame_after_ns = None;
                self.refresh_frame_after = None;
                self.refresh_frame_after_ns = None;
                self.candidates.clear();
                self.vision_diagnostics = None;
                self.runtime_message = Some(format!("브라우저 연결 완료 · {}", target.title));
                Ok(serde_json::to_value(target).unwrap())
            }
            Command::Save { profile, expected_profile_revision } => {
                check_profile_revision(expected_profile_revision, self.profile_revision)?;
                self.save(profile)?;
                Ok(self.profile_reply())
            }
            Command::ArmCapture { field, profile, expected_profile_revision } => {
                check_profile_revision(expected_profile_revision, self.profile_revision)?;
                self.deactivate_enter_watch();
                self.target_recovery = None;
                if self.active() { self.cancel_queued_actions(); self.feed(Event::Pause); }
                self.interrupt.store(false, Ordering::SeqCst);
                self.cancel_capture("다른 항목을 지정하여 이전 채집을 취소했습니다.");
                let label = capture_label(&field)?;
                if self.adapter()?.target().is_none() { return Err("먼저 예매창을 ‘브라우저 연결’로 연결하세요.".into()); }
                self.permissions = self.native.as_ref().ok().map(NativeAdapter::permissions);
                if !self.permissions.as_ref().is_some_and(|p| p.screen_capture) {
                    return Err("좌표 지정에는 화면 기록 권한이 필요합니다. ‘화면 및 시스템 오디오 녹음’에서 이 앱을 허용하고 다시 실행하세요.".into());
                }
                if let Some(profile) = profile { self.save(profile)?; }
                self.capture_field = field.clone();
                self.capture_armed = true;
                self.capture_state = CaptureState::Armed;
                self.capture_error = None;
                self.runtime_message = Some(format!("{label} · 예매창의 대상 위에서 F8"));
                self.record("capture_armed", json!({"field":field}));
                self.sync_overlay_visibility();
                Ok(json!({"field":field,"profile":self.raw_profile,"profileRevision":self.profile_revision,"snapshot":self.snapshot_value()}))
            }
            Command::PrepareShortcut(_) => unreachable!("shortcut preparation retains its received epoch"),
            Command::ApplyShortcut { request_id, profile, error, expected_profile_revision } => {
                if let Some(mode) = self.start_shortcut.expire(self.now(), self.shortcut_epoch.load(Ordering::SeqCst)) {
                    self.report_error(format!("{} 시작 실패 · 설정 화면 응답이 늦었습니다. 예매창에서 {} 키를 다시 누르세요.",
                        start_shortcut_label(mode), start_shortcut_key(mode)));
                }
                let Some(request) = self.start_shortcut.take(request_id, self.now(), self.shortcut_epoch.load(Ordering::SeqCst)) else {
                    return Ok(self.cancelled_profile_reply());
                };
                // Late responses from a shortcut cancelled by this check do
                // not end it. Only a current start request can take ownership.
                if self.keyboard_check.active() { self.finish_keyboard_check(true, None); }
                let StartShortcutRequest { mode, epoch, .. } = request;
                if !self.editable() { return Ok(self.cancelled_profile_reply()); }
                if let Some(error) = error {
                    return Err(format!("{} 시작 실패 · {error}", start_shortcut_label(mode)));
                }
                let profile = profile.ok_or_else(|| format!("{} 설정을 받지 못했습니다.", start_shortcut_label(mode)))?;
                self.save_start_profile(profile, expected_profile_revision)?;
                if self.shortcut_epoch.load(Ordering::SeqCst) != epoch {
                    return Ok(self.cancelled_profile_reply());
                }
                // Global start shortcuts keep the actual foreground guard. Unlike the
                // settings Start button, it never activates a different window.
                let snapshot = self.start_guarded(mode, Some(epoch))?;
                if snapshot.get("cancelled") == Some(&Value::Bool(true))
                    || self.shortcut_epoch.load(Ordering::SeqCst) != epoch {
                    return Ok(self.cancelled_profile_reply());
                }
                self.record("start_shortcut_applied", json!({"requestId":request_id,"mode":mode,"shortcut":start_shortcut_key(mode)}));
                Ok(json!({"profile":self.raw_profile,"profileRevision":self.profile_revision,"snapshot":snapshot}))
            }
            Command::StartFromUi { .. } => unreachable!("UI start retains its received control epoch"),
            Command::Pause => {
                self.target_recovery = None;
                self.interrupt.store(false, Ordering::SeqCst);
                self.cancel_queued_actions();
                self.feed(Event::Pause);
                self.runtime_message = Some("일시정지됨 · 다시 진행을 누르면 탐색합니다.".into());
                if let Ok(adapter) = &self.native { let _ = adapter.beep(); }
                Ok(self.snapshot_value())
            }
            Command::TogglePause => {
                self.interrupt.store(false, Ordering::SeqCst);
                if self.target_recovery.is_some() { self.command(Command::Pause) }
                else if self.engine.snapshot().phase == Phase::Paused { self.command(Command::Resume) }
                else { self.command(Command::Pause) }
            }
            Command::Resume => {
                self.target_recovery = None;
                self.adapter()?.validate_target().map_err(|e| e.to_string())?;
                self.interrupt.store(false, Ordering::SeqCst);
                self.runtime_message = None;
                self.last_sequence = None;
                self.feed(Event::Resume);
                if let Ok(adapter) = &self.native { let _ = adapter.beep(); }
                Ok(self.snapshot_value())
            }
            Command::ResumeFromUi => {
                self.target_recovery = None;
                if self.engine.snapshot().phase != Phase::Paused { return Err("일시정지한 실행만 재개할 수 있습니다.".into()); }
                self.activate_booking_target()?;
                self.command(Command::Resume)
            }
            Command::Stop => {
                self.target_recovery = None;
                // Cancellation must precede the checkpoint: a queued Esc has
                // not been delivered and must not retain a successful-Esc latch.
                self.cancel_queued_actions();
                let current = self.engine.has_unresolved_conflict().then(|| self.capture_restart_context());
                self.restart_context = stopped_conflict_context(current,
                    self.restart_context.take(), &mut self.visual_recovery);
                self.interrupt.store(false, Ordering::SeqCst);
                self.cancel_capture("중지하여 좌표 채집도 취소했습니다.");
                self.capture_armed = false;
                self.feed(Event::Stop);
                self.runtime_message = Some("중지했습니다.".into());
                Ok(self.snapshot_value())
            }
            Command::Continue => {
                let kind = self.default_confirmation().ok_or("현재 단계에서 확인할 작업이 없습니다.")?;
                self.confirm(&kind, None)?;
                Ok(self.snapshot_value())
            }
            Command::ContinueAfterCaptcha => unreachable!("F9 retains its received control epoch"),
            Command::Confirm { kind, run_id, revision, selected_ids } => {
                if run_id != self.run_id || revision != self.revision {
                    return Err("화면 상태가 바뀌었습니다. 현재 상태를 확인한 뒤 다시 누르세요.".into());
                }
                // This command comes from the settings UI. F6 uses Continue
                // and keeps the direct foreground guard without stealing focus.
                self.activate_booking_target()?;
                self.confirm(&kind, selected_ids)?;
                Ok(self.snapshot_value())
            }
            Command::Clock(url) => {
                if !self.editable() { return Err("서버 시간 측정은 실행을 중지한 뒤 진행하세요.".into()); }
                let estimate = clock::synchronize(&url, || self.now())?;
                self.runtime_message = Some(format!("서버 시간 측정 · 응답 {}ms · 추정 오차 ±{}ms",
                    estimate.rtt_ms, estimate.uncertainty_ms));
                self.clock = Some(estimate.clone());
                Ok(serde_json::to_value(estimate).unwrap())
            }
            Command::Overlay(enabled) => {
                if enabled {
                    if let Some(error) = &self.overlay_error { return Err(error.clone()); }
                }
                self.overlay_enabled = enabled;
                self.sync_overlay_visibility();
                Ok(json!({"enabled":enabled}))
            }
            Command::OverlayIds(ids) => {
                if let Err(error) = self.adapter()?.set_overlay_windows(ids) {
                    let message = format!("상단 상태 표시창 준비 실패: {error}");
                    self.overlay_error = Some(message.clone());
                    return Err(message);
                }
                self.overlay_error = None;
                Ok(json!({"ok":true}))
            }
            Command::OverlayError(message) => {
                self.overlay_enabled = false;
                self.overlay_error = Some(message);
                Ok(json!({"ok":false}))
            }
            Command::ShortcutStatus(errors) => { self.shortcut_errors = errors; Ok(json!({"ok":true})) }
            Command::Shutdown => {
                self.target_recovery = None;
                self.cancel_capture("앱이 종료되어 좌표 채집을 취소했습니다.");
                self.shutting_down = true; self.actions.clear(); Ok(json!({"ok":true}))
            }
            Command::CaptureArmed => unreachable!("F8 capture is handled immediately"),
        }
    }

    fn save(&mut self, value: Value) -> Result<Value, String> {
        let profile = Profile::from_json(&value.to_string()).map_err(|e| e.to_string())?;
        let desktop: DesktopConfig = serde_json::from_value(value.get("desktop").cloned().unwrap_or(json!({})))
            .map_err(|e| format!("화면 설정 형식 오류: {e}"))?;
        let paused = self.engine.snapshot().phase == Phase::Paused;
        let changed_geometry = reference_geometry_changed(self.desktop.capture_reference.as_ref(), desktop.capture_reference.as_ref());
        if !self.editable() && !paused {
            return Err("설정을 바꾸려면 먼저 일시정지하세요.".into());
        }
        if paused {
            let mut previous = self.profile.clone();
            previous.vision = profile.vision.clone();
            previous.color = profile.color;
            previous.region_color = profile.region_color;
            previous.legacy.complete = profile.legacy.complete.clone();
            previous.legacy.regions = profile.legacy.regions.clone();
            previous.entry_target = profile.entry_target.clone();
            previous.name = profile.name.clone();
            if changed_geometry {
                // A resized/new booking window invalidates old coordinates. Only
                // clearing them is allowed here; arbitrary edits still fail below.
                if profile.legacy.date.is_none() { previous.legacy.date = None; }
                if profile.legacy.session.is_none() { previous.legacy.session = None; }
                if profile.legacy.next.is_none() { previous.legacy.next = None; }
                if profile.refresh_target.is_none() { previous.refresh_target = None; }
                if matches!(previous.legacy.cancellation_navigation, macro_core::CancellationNavigation::TwoRegions { .. })
                    && matches!(profile.legacy.cancellation_navigation, macro_core::CancellationNavigation::None) {
                    previous.legacy.cancellation_navigation = macro_core::CancellationNavigation::None;
                }
            }
            if serde_json::to_value(&previous).unwrap() != serde_json::to_value(&profile).unwrap() {
                return Err("일시정지 중에는 현재 화면의 탐색 범위·색상·무대 기준·선택 완료 위치만 바꿀 수 있습니다. 다른 설정을 바꾸려면 중지하세요.".into());
            }
        }
        let mut updated_engine = if paused { self.engine.clone() }
            else { Engine::new(profile.clone()).map_err(|e| e.to_string())? };
        if paused {
            if changed_geometry { updated_engine.invalidate_coordinate_geometry().map_err(|e| e.to_string())?; }
            updated_engine.update_calibration(profile.vision.clone(), profile.color, profile.region_color, profile.legacy.complete.clone())
                .map_err(|e| e.to_string())?;
            updated_engine.update_legacy_regions(profile.legacy.regions.clone()).map_err(|e| e.to_string())?;
            let entry_target = if entry_click_enabled(&profile, &desktop, self.engine.snapshot().mode) {
                profile.entry_target.clone()
            } else { None };
            updated_engine.update_entry_target(entry_target).map_err(|e| e.to_string())?;
        }
        storage::save(&self.path, &value)?;
        let previous_area = seat_area_presence(&self.raw_profile);
        let next_area = seat_area_presence(&value);
        if previous_area != next_area {
            // Presence only: a missing value does not prove the user pressed
            // Clear. This also exposes stale full-profile saves after F8.
            self.record("seat_area_settings_changed", json!({"before":previous_area,"after":next_area}));
        }
        self.target_recovery = None;
        if self.profile.server_url != profile.server_url { self.clock = None; }
        self.engine = updated_engine;
        self.profile = profile;
        self.desktop = desktop;
        self.raw_profile = value;
        self.profile_revision = self.profile_revision.wrapping_add(1);
        self.vision_diagnostics = None;
        if changed_geometry {
            self.visual_recovery.reset();
            self.frame = None;
            self.last_sequence = None;
            self.last_scan_generation = None;
            self.candidates.clear();
        }
        self.runtime_message = Some("설정 저장 완료".into());
        if let Err(error) = self.command(Command::Overlay(self.profile.features.status_overlay)) {
            self.runtime_message = Some(format!("설정 저장 완료 · {error}"));
        }
        Ok(self.raw_profile.clone())
    }

    fn profile_reply(&self) -> Value {
        json!({"profile":self.raw_profile,"profileRevision":self.profile_revision,"snapshot":self.snapshot_value()})
    }

    fn cancelled_profile_reply(&self) -> Value {
        let mut reply = self.profile_reply();
        reply["cancelled"] = json!(true);
        reply
    }

    fn current_execution_result(&self, result: Result<Value, String>, epoch: u64) -> Result<Value, String> {
        if epoch == self.shortcut_epoch.load(Ordering::SeqCst) { result }
        else { Ok(self.cancelled_profile_reply()) }
    }

    fn save_start_profile(&mut self, profile: Value, revision: u64) -> Result<(), String> {
        if start_profile_save_needed(&profile, revision, &self.raw_profile, self.profile_revision)? {
            self.save(profile)?;
        }
        Ok(())
    }

    fn capture_restart_context(&self) -> RestartContext {
        let geometry = self.frame.as_ref().map(|frame| FrameReference {
                width: frame.width, height: frame.height, scale_x: frame.scale_x, scale_y: frame.scale_y,
            }).or_else(|| self.desktop.capture_reference.clone());
        RestartContext { engine: self.engine.clone(),
                target_id: self.native.as_ref().ok().and_then(NativeAdapter::target).map(|target| target.id),
                geometry, run_id: self.run_id, started_at: self.run_started_at,
                count_receipt: self.selection_count_after_input }
    }

    fn supersede_execution(&mut self, post_entry: bool) {
        self.cancel_queued_actions();
        if !matches!(self.engine.snapshot().phase, Phase::Idle | Phase::Stopped | Phase::Faulted) {
            self.restart_context = Some(self.capture_restart_context());
        } else if post_entry {
            // A prepared start has stopped the old work while waiting for UI.
            // A newer F9 may still resume that same completed-entry context.
            if let Some(context) = &self.restart_context {
                self.engine = context.engine.clone();
            } else if self.engine.has_retained_selection() {
                self.restart_context = Some(self.capture_restart_context());
            }
        }
        self.deactivate_enter_watch();
        self.deactivate_captcha_input_watch();
        self.cancel_capture("새 실행 명령으로 이전 좌표 지정을 취소했습니다.");
        self.target_recovery = None;
        self.selection_count_input_proof = None;
        self.selection_count_read_cache = None;
        self.nol_captcha.reset();
        self.nol_handoff_pending = false;
        if post_entry {
            if self.active() { self.feed(Event::Pause); }
        } else { self.feed(Event::Stop); }
        self.runtime_message = Some("이전 작업을 중단했습니다 · 새 실행 설정을 확인합니다".into());
        self.interrupt.store(false, Ordering::SeqCst);
    }

    fn start_from_ui(&mut self, mode: RunMode, profile: Value, expected_profile_revision: u64, epoch: u64) -> Result<Value, String> {
        if epoch != self.shortcut_epoch.load(Ordering::SeqCst) { return Ok(self.cancelled_profile_reply()); }
        // One request owns both the submitted settings and the start. F4/F8
        // received during storage, activation or capture invalidate its epoch.
        let next_profile = Profile::from_json(&profile.to_string()).map_err(|e| e.to_string())?;
        let next_desktop = serde_json::from_value(profile.get("desktop").cloned().unwrap_or(json!({})))
            .map_err(|e| format!("화면 설정 형식 오류: {e}"))?;
        validate_entry_setup(&next_profile, &next_desktop, mode)?;
        self.save_start_profile(profile, expected_profile_revision)?;
        if epoch != self.shortcut_epoch.load(Ordering::SeqCst) { return Ok(self.cancelled_profile_reply()); }
        self.activate_booking_target()?;
        if epoch != self.shortcut_epoch.load(Ordering::SeqCst) { return Ok(self.cancelled_profile_reply()); }
        let snapshot = self.start_guarded(mode, Some(epoch))?;
        if snapshot.get("cancelled") == Some(&Value::Bool(true)) { return Ok(self.cancelled_profile_reply()); }
        Ok(json!({"profile":self.raw_profile,"profileRevision":self.profile_revision,"snapshot":snapshot,"controlEpoch":epoch}))
    }

    fn prepare_start_shortcut(&mut self, mode: RunMode, epoch: u64) -> Result<Value, String> {
        if epoch != self.shortcut_epoch.load(Ordering::SeqCst) {
            return Ok(json!({"cancelled":true}));
        }
        if !self.editable() { return Err("이미 실행 중입니다. 새 실행 전 중지하세요.".into()); }
        let Some(id) = self.start_shortcut.prepare(mode, self.now(), epoch) else {
            return Ok(json!({"pending":true}));
        };
        let result = self.app.get_webview_window("main")
            .ok_or_else(|| format!("설정 화면이 없어 {} 시작을 요청할 수 없습니다.", start_shortcut_label(mode)))
            .and_then(|_| self.app.emit_to("main", "shortcut-start-requested",
                json!({"requestId":id,"mode":mode,"shortcut":start_shortcut_key(mode),"controlEpoch":epoch}))
                .map_err(|error| format!("{} 설정 요청 전달 실패: {error}", start_shortcut_key(mode))));
        if let Err(error) = result {
            self.start_shortcut.pending = None;
            return Err(error);
        }
        self.runtime_message = Some(format!("{} · 현재 설정을 읽는 중", start_shortcut_label(mode)));
        self.feedback_until_ms = self.now() + 5000;
        self.record("start_shortcut_requested", json!({"requestId":id,"mode":mode,"shortcut":start_shortcut_key(mode)}));
        Ok(json!({"requestId":id,"controlEpoch":epoch}))
    }

    fn start_guarded(&mut self, mode: RunMode, shortcut_epoch: Option<u64>) -> Result<Value, String> {
        if shortcut_epoch.is_some_and(|epoch| epoch != self.shortcut_epoch.load(Ordering::SeqCst)) {
            return Ok(json!({"cancelled":true}));
        }
        if !self.editable() { return Err("이미 실행 중입니다. 새 실행 전 중지하세요.".into()); }
        validate_entry_setup(&self.profile, &self.desktop, mode)?;
        self.record("start_requested", json!({"optionsSource":"saved_profile","mode":mode,"scheduled":mode == RunMode::Normal && self.desktop.scheduled_start,
            "hasEntryTarget":self.profile.entry_target.is_some(),
            "hasGlobalSeatArea":self.profile.vision.roi.is_some(),
            "entryClickEnabled":entry_click_enabled(&self.profile, &self.desktop, mode),
            "captchaEnterEnabled":mode == RunMode::Normal && self.profile.features.auto_captcha_resume
                && !self.profile.features.nol_captcha_detection,
            "nolCaptchaEnabled":mode == RunMode::Normal && self.profile.features.nol_captcha_detection,
            "regionPriority":self.profile.features.region_priority,
            "targetSeats":self.profile.target_seats,
            "registeredEntryButtons":{
                "date":self.profile.legacy.date.is_some(), "session":self.profile.legacy.session.is_some(),
                "next":self.profile.legacy.next.is_some()
            }}));
        self.feedback_until_ms = self.now() + 8000;
        self.cancel_capture("새 실행을 시작하여 이전 좌표 채집을 취소했습니다.");
        // Display startup failures without making the user cover the booking window.
        self.command(Command::Overlay(self.profile.features.status_overlay))?;
        let permissions = self.adapter()?.permissions();
        self.permissions = Some(permissions.clone());
        if !permissions.screen_capture || !permissions.input {
            return Err(format!("실행 권한을 확인하세요. {}", permissions.details.join(" ")));
        }
        self.adapter()?.validate_target().map_err(|e| e.to_string())?;
        let frame = self.adapter()?.capture().map_err(|e| e.to_string())?;
        if let Some(reference) = &self.desktop.capture_reference {
            if !same_frame_reference(reference, &frame) {
                return Err("화면 크기나 배율이 좌표 채집 때와 달라졌습니다. 현재 화면에서 좌표를 다시 채집하세요.".into());
            }
        }
        self.frame = Some(frame);
        let mut profile = self.profile.clone();
        if mode != RunMode::Normal { profile.features.auto_entry = false; }
        if !entry_click_enabled(&profile, &self.desktop, mode) { profile.entry_target = None; }
        profile.sale_deadline_ms = None;
        if mode == RunMode::Normal && self.desktop.scheduled_start {
            let sale = self.desktop.sale_time.as_deref().ok_or("판매 시각을 입력하세요.")?;
            profile.sale_deadline_ms = if profile.features.server_clock {
                let estimate = self.clock.as_ref().ok_or("먼저 ‘서버 시간 측정’을 실행하세요.")?;
                if profile.server_url.as_deref() != Some(estimate.url.as_str()) {
                    return Err("사이트 주소가 바뀌었습니다. ‘서버 시간 측정’을 다시 실행하세요.".into());
                }
                Some(estimate.deadline(sale, self.now())?)
            } else {
                Some(self.now().saturating_add((clock::parse_sale_time(sale)? - clock::unix_ms()).max(0) as u64))
            };
            // Scheduling is optional even when a clock has been measured.
            profile.features.server_clock = true;
        } else {
            profile.features.server_clock = false;
        }
        // Keep both saved spellings compatible; only explicit cancellation
        // button use with a coordinate authorizes refresh input.
        apply_execution_options(&mut profile, &self.desktop, mode);
        profile.site_auto_refresh = false;
        // Save/capture may block. A cancellation received meanwhile invalidates
        // this request before clearing interrupt or queueing its first action.
        if shortcut_epoch.is_some_and(|epoch| epoch != self.shortcut_epoch.load(Ordering::SeqCst)) {
            return Ok(json!({"cancelled":true}));
        }
        let restart = self.restart_context.clone().filter(|_| mode != RunMode::Normal);
        let preserve = restart.as_ref().is_some_and(|context| {
            context.target_id == self.native.as_ref().ok().and_then(NativeAdapter::target).map(|target| target.id)
                && context.geometry.as_ref().is_some_and(|geometry|
                    self.frame.as_ref().is_some_and(|frame| same_frame_reference(geometry, frame)))
                && same_search_mapping(&context.engine, &profile, mode)
        });
        if restart.as_ref().is_some_and(|context| context.engine.has_retained_selection()) && !preserve {
            return Err("이전 작업은 중단했습니다. 전달한 좌석 기록과 현재 창·좌석 범위가 달라 재클릭을 보류합니다. 기존 화면·설정을 복원해 다시 시작하세요. F4는 입력만 중지하며 좌석 선택을 해제하지 않습니다.".into());
        }
        self.runtime_message = None;
        let restart_actions = if preserve {
            let context = restart.as_ref().unwrap();
            self.engine = context.engine.clone();
            self.run_id = context.run_id;
            self.run_started_at = context.started_at;
            self.selection_count_after_input = context.count_receipt;
            Some(self.engine.restart_search(profile, mode, true, self.now()).map_err(|error| error.to_string())?)
        } else {
            self.engine = Engine::new(profile).map_err(|e| e.to_string())?;
            self.selection_count_after_input = None;
            None
        };
        self.restart_context = None;
        self.run_shortcut_epoch = shortcut_epoch;
        self.interrupt.store(false, Ordering::SeqCst);
        self.actions.clear();
        self.candidates.clear();
        self.last_sequence = None;
        self.last_scan_generation = None;
        self.last_input_sequence = None;
        self.last_input_started_at_ns = None;
        self.seat_frame_after = None;
        self.seat_frame_after_ns = None;
        self.refresh_frame_after = None;
        self.refresh_frame_after_ns = None;
        self.target_recovery = None;
        if !preserve { self.visual_recovery.reset(); }
        self.nol_captcha.reset();
        self.next_nol_observation_ms = 0;
        self.deactivate_enter_watch();
        self.enter_watch_error = None;
        self.deactivate_captcha_input_watch();
        self.captcha_input_watch_error = None;
        self.seat_rescan_requested = false;
        self.vision_diagnostics = None;
        if let Some(actions) = restart_actions {
            self.actions.extend(actions);
        } else {
            self.run_id += 1;
            self.run_started_at = Instant::now();
            self.feed(Event::Start { mode });
        }
        self.record("started", json!({"phase":self.engine.snapshot().phase,"retainedSearchContext":preserve,
            "deadlineMs":self.engine.profile().sale_deadline_ms,
            "effectiveOptions":{
                "regionPriority":self.engine.profile().features.region_priority,
                "automaticRefresh":self.engine.profile().features.automatic_refresh,
                "hasRefreshTarget":self.engine.profile().refresh_target.is_some(),
                "cancellationNavigation":match self.engine.profile().legacy.cancellation_navigation {
                    macro_core::CancellationNavigation::None => "none",
                    macro_core::CancellationNavigation::ArrowKeys => "arrow_keys",
                    macro_core::CancellationNavigation::TwoRegions { .. } => "two_regions",
                }
            }}));
        Ok(self.snapshot_value())
    }

    fn activate_booking_target(&mut self) -> Result<(), String> {
        self.runtime_message = Some("연결한 예매창으로 전환 중".into());
        self.feedback_until_ms = self.now() + 5000;
        self.sync_overlay_visibility();
        self.publish();
        let result = self.adapter()?.activate_target().map_err(|e| e.to_string());
        self.record("target_activation", json!({"success":result.is_ok(),"error":result.as_ref().err()}));
        result
    }

    fn pause_for_context(&mut self, reason: String, blocked_point: Option<Point>) {
        self.pause_for_context_at(reason, blocked_point, false);
    }

    fn pause_for_context_at(&mut self, reason: String, blocked_point: Option<Point>, blocked_image_candidate: bool) {
        let retry = automatic_context_recovery(self.engine.continuous_search(), &self.engine.snapshot())
            && recoverable_context_error(&reason);
        self.cancel_queued_actions();
        self.feed(Event::ContextLost { reason: reason.clone() });
        // A cached pre-interruption image cannot be used as the next observation.
        // The native stream must supply a valid copy after target validation.
        self.frame = None;
        self.last_sequence = None;
        self.last_scan_generation = None;
        self.seat_rescan_requested = false;
        self.vision_diagnostics = None;
        self.target_recovery = retry.then(|| TargetRecovery {
            active: true, reason: reason.clone(), next_check_ms: self.now() + 250, blocked_point, blocked_image_candidate,
        });
        self.runtime_message = retry.then(|| "예매창 복귀 대기 · 클릭 보류 · 확인되면 자동 재개".into());
        self.record("context_wait", json!({"automaticRecovery":retry,"reason":reason}));
    }

    fn recover_target(&mut self) {
        let Some(waiting) = self.target_recovery.as_ref() else { return; };
        if self.engine.snapshot().phase != Phase::Paused || self.capture_armed {
            self.target_recovery = None;
            return;
        }
        if self.interrupt.load(Ordering::SeqCst) || self.now() < waiting.next_check_ms { return; }
        let blocked_point = waiting.blocked_point;
        let blocked_image_candidate = waiting.blocked_image_candidate;
        let next_check_ms = self.now() + 250;
        self.target_recovery.as_mut().unwrap().next_check_ms = next_check_ms;
        let result = (|| -> Result<OwnedFrame, String> {
            self.adapter()?.validate_target().map_err(|e| e.to_string())?;
            let frame = self.adapter()?.capture().map_err(|e| e.to_string())?;
            self.adapter()?.validate_target().map_err(|e| e.to_string())?;
            if self.desktop.capture_reference.as_ref().is_some_and(|reference| !same_frame_reference(reference, &frame)) {
                return Err("화면 크기나 배율이 달라졌습니다. 현재 화면에서 좌표를 다시 채집하세요.".into());
            }
            if let Some(point) = blocked_point {
                let local = if blocked_image_candidate { image_candidate_local(&frame, point, cfg!(target_os = "windows"))? }
                else { macro_platform::Point {
                    x: point.x / frame.scale_x, y: point.y / frame.scale_y,
                }};
                self.adapter()?.validate_point(local).map_err(|e| e.to_string())?;
            }
            validate_visual_recovery_frame(&frame, self.visual_enabled())?;
            Ok(frame)
        })();
        // A stop, pause, or F8 arriving during a capture wins over recovery.
        if self.interrupt.load(Ordering::SeqCst) { return; }
        match result {
            Ok(frame) => {
                if !frame_passes_input_transition(&frame, self.seat_frame_after, self.seat_frame_after_ns) { return; }
                self.frame = Some(frame);
                self.last_sequence = None;
                self.target_recovery = None;
                self.runtime_message = None;
                self.feed(Event::Resume);
                self.record("context_recovered", json!({"mode":self.engine.snapshot().mode}));
            }
            Err(reason) if recoverable_context_error(&reason) => {
                if let Some(waiting) = self.target_recovery.as_mut() { waiting.reason = reason; }
            }
            Err(reason) => {
                self.target_recovery = None;
                self.runtime_message = Some(format!("확인 필요 · {reason}"));
                self.record("context_recovery_blocked", json!({"reason":reason}));
            }
        }
    }

    fn await_post_input_frame(&mut self) {
        self.seat_frame_after = self.last_input_sequence;
        self.seat_frame_after_ns = self.last_input_started_at_ns;
        self.frame = None;
        self.last_sequence = None;
        self.last_scan_generation = None;
        self.seat_rescan_requested = false;
        self.vision_diagnostics = None;
    }

    fn feed(&mut self, event: Event) {
        let resuming = matches!(&event, Event::Resume);
        let reset_nol = matches!(&event, Event::Start { .. } | Event::Stop);
        let interrupt_nol = matches!(&event, Event::Pause | Event::ContextLost { .. });
        let was_awaiting_nol = self.engine.awaits_nol_captcha();
        let now = self.now();
        let actions = self.engine.step(event, now);
        self.actions.extend(actions);
        if reset_nol || interrupt_nol || self.selection_count_input_proof.as_ref().is_some_and(|proof|
            proof.run_id != self.run_id || self.engine.snapshot().pending_action_id != Some(proof.action_id)) {
            self.selection_count_input_proof = None;
        }
        if reset_nol { self.selection_count_read_cache = None; }
        if reset_nol || (!was_awaiting_nol && self.engine.awaits_nol_captcha() && !resuming) {
            self.nol_captcha.reset();
            self.next_nol_observation_ms = 0;
            self.last_nol_log_ms = None;
            self.nol_handoff_pending = false;
        } else if interrupt_nol {
            self.nol_captcha.interrupt();
        }
        if !self.engine.accepts_captcha_enter() { self.deactivate_enter_watch(); }
        if !self.engine.awaits_nol_captcha() || self.nol_captcha.has_reference() {
            self.deactivate_captcha_input_watch();
        }
        if resuming && self.engine.snapshot().phase != Phase::Paused
            && self.capture_state == CaptureState::Captured {
            // Successful calibration must not hide run progress after resuming.
            self.capture_state = CaptureState::Idle;
        }
    }

    fn cancel_queued_actions(&mut self) {
        self.selection_count_input_proof = None;
        let now = self.now();
        cancel_unposted_actions(&mut self.engine, &mut self.actions, now);
    }

    fn cancel_capture(&mut self, reason: &str) {
        if self.capture_armed {
            self.capture_armed = false;
            self.runtime_message = Some(reason.into());
            self.record("capture_cancelled", json!({"field":self.capture_field,"reason":reason}));
        }
        self.capture_state = CaptureState::Idle;
        self.capture_error = None;
    }

    fn capture_point(&mut self, field: &str, epoch: u64) -> Result<Value, String> {
        let label = capture_label(field)?;
        if self.active() {
            self.cancel_queued_actions();
            self.feed(Event::Pause);
        }
        let target = self.adapter()?.target().cloned().ok_or("먼저 브라우저 연결을 실행하세요.")?;
        self.runtime_message = Some(format!("{label} · 예매창 화면 읽는 중"));
        self.publish();
        self.record("capture_frame", json!({"field":field}));
        // Read-only F8 calibration checks the actual window under the pointer.
        // It does not require keyboard focus; execution still uses strict capture/input guards.
        let (global, frame) = self.adapter()?.capture_at_pointer().map_err(|e| e.to_string())?;
        if epoch != self.shortcut_epoch.load(Ordering::SeqCst) { return Ok(self.cancelled_profile_reply()); }
        let local = macro_platform::Point { x: global.x - target.rect.x, y: global.y - target.rect.y };
        let (x,y) = frame.local_to_pixel(local).ok_or("마우스를 연결한 브라우저 창 안에 놓으세요.")?;
        let [r,g,b] = frame.color_at(local).ok_or("마우스 위치의 색상을 읽지 못했습니다.")?;
        let point = Point { x: x as f64, y: y as f64 };
        let reference = FrameReference { width: frame.width, height: frame.height, scale_x: frame.scale_x, scale_y: frame.scale_y };
        let mut value = json!({"field":field,"point":point,"color":{"r":r,"g":g,"b":b},"frame":reference});
        // Updating the current visible screen preserves the paused run and selections.
        let updated = apply_capture(&self.raw_profile, field, point, Color {r,g,b}, &reference)?;
        self.runtime_message = Some(format!("{label} · 좌표 저장 중"));
        self.publish();
        self.record("capture_save", json!({"field":field,"point":point,"frame":reference}));
        // Publishing/logging may block after the first capture guard. Do not
        // commit an old F8 result after a newer F9/F4 has taken ownership.
        if !capture_commit_current(epoch, self.shortcut_epoch.load(Ordering::SeqCst)) {
            return Ok(self.cancelled_profile_reply());
        }
        self.save(updated)?;
        value["profile"] = self.raw_profile.clone();
        value["profileRevision"] = json!(self.profile_revision);
        self.frame = Some(frame);
        self.runtime_message = Some(format!("{label} 지정 완료 · ({x}, {y}) · #{r:02X}{g:02X}{b:02X}"));
        self.record("capture_complete", json!({"field":field,"point":point}));
        Ok(value)
    }

    fn execute_actions(&mut self) {
        for _ in 0..32 {
            let input_epoch = self.shortcut_epoch.load(Ordering::SeqCst);
            if self.interrupt.load(Ordering::SeqCst) || !self.active()
                || self.shortcut_start_cancelled() { return; }
            // Leave unsent input in the queue during capture/validation so a
            // context pause can cancel it before the engine discards its ID.
            let Some(action) = self.actions.front().cloned() else { return; };
            if let ActionKind::Observe { scope } = &action.kind {
                self.actions.pop_front();
                self.scope = scope.clone();
                self.seat_rescan_requested = scope == "seats";
                continue;
            }
            // A later observation can supersede an input before it reaches the OS.
            if self.engine.snapshot().pending_action_id != Some(action.id) {
                self.actions.pop_front();
                continue;
            }
            // Check only the first region/seat handoff after NOL's background
            // changes from its reference. Reuse that frame for seat color and popup
            // baseline checks below, so all three inspect the same image.
            let checked_nol_handoff = self.nol_handoff_pending
                && matches!(&action.kind, ActionKind::Click { purpose, .. }
                    if purpose.starts_with("region:") || purpose.starts_with("seat:"));
            if checked_nol_handoff && !self.recheck_nol_handoff() { return; }
            if let ActionKind::Click { target, purpose } = &action.kind {
                if purpose.starts_with("seat:") {
                    let availability = if checked_nol_handoff { self.candidate_available_on_frame(target) }
                        else { self.candidate_still_available(target) };
                    match availability {
                        Ok(available) => {
                            // A dim backdrop can also change the candidate's color.
                            // Inspect the newly captured image before discarding it.
                            if self.observe_visual_current() { return; }
                            if !available {
                                // Proof of non-delivery: the native click has not been called.
                                self.feed(Event::ActionCancelled { id: action.id });
                                self.last_sequence = None;
                                self.runtime_message = Some("클릭 직전 좌석의 색상·면적이 달라져 최신 후보를 다시 확인합니다.".into());
                                continue;
                            }
                        }
                        Err(reason) => {
                            self.feed(Event::ActionCancelled { id: action.id });
                            self.pause_for_context(reason, None);
                            return;
                        }
                    }
                }
            }
            let count_protected = self.engine.profile().features.nol_selection_count
                && needs_selection_count_proof(&action.kind);
            if count_protected && !matches!(&action.kind, ActionKind::Click { purpose, .. } if purpose.starts_with("seat:")) {
                // Completion and cancellation navigation/refresh need the same
                // latest-frame count guard, including when conflict recovery is off.
                if let Err(reason) = self.refresh_candidate_frame() {
                    self.feed(Event::ActionCancelled { id:action.id });
                    self.pause_for_context(reason, None);
                    return;
                }
                if self.observe_visual_current() { return; }
                if self.engine.snapshot().pending_action_id != Some(action.id) || !self.active() { return; }
            } else if matches!(&action.kind, ActionKind::CompleteSelection { .. })
                && self.visual_recovery.is_armed() {
                // Catch a popup already present before completion. The normal
                // next page after this button must never be interpreted as one.
                self.observe_frame();
                if self.engine.snapshot().pending_action_id != Some(action.id)
                    || self.visual_recovery.is_incident() || !self.active() { return; }
            }
            // Capture can take time. A stop received during it must win before input.
            if self.interrupt.load(Ordering::SeqCst) || self.shortcut_start_cancelled() {
                return;
            }
            let seat_baseline = if self.visual_enabled()
                && matches!(&action.kind, ActionKind::Click { purpose, .. } if purpose.starts_with("seat:")) {
                match self.sample_visual_frame() {
                    Ok(sample) => Some(sample),
                    Err(reason) => {
                        self.feed(Event::ActionCancelled { id: action.id });
                        self.pause_for_context(reason, None);
                        return;
                    }
                }
            } else { None };
            if self.interrupt.load(Ordering::SeqCst) || self.shortcut_start_cancelled() { return; }
            // Restore this watch if validation proves that completion was never
            // delivered. Successful completion/navigation ends visual monitoring.
            if self.poll_captcha_enter() { return; }
            if self.interrupt.load(Ordering::SeqCst) || self.shortcut_start_cancelled() { return; }
            if count_protected {
                let permitted = selection_count_input_allowed(self.selection_count_input_proof.as_ref(),
                    self.frame.as_ref(), self.run_id, action.id);
                // A re-read of a changed frame can take time. Stop still wins.
                if self.interrupt.load(Ordering::SeqCst) || self.shortcut_start_cancelled() { return; }
                if !permitted {
                    self.record("selection_count_input_cancelled", json!({"actionId":action.id,
                        "requestId":self.selection_count_input_proof.as_ref().map(|p|p.request_id),
                        "observationId":self.selection_count_input_proof.as_ref().map(|p|p.observation_id)}));
                    self.feed(Event::ActionCancelled { id:action.id });
                    self.next_selection_count_ms = 0;
                    self.last_sequence = None;
                    self.seat_rescan_requested = true;
                    self.runtime_message = Some("클릭 직전 선택 좌석 수 표시가 달라져 다시 확인합니다.".into());
                    return;
                }
            }
            let navigation_watch = if matches!(&action.kind,
                ActionKind::CompleteSelection { .. } | ActionKind::Refresh { .. })
                || matches!(&action.kind, ActionKind::Click { purpose, .. } if purpose.starts_with("region:")) {
                Some(std::mem::take(&mut self.visual_recovery))
            } else { None };
            self.actions.pop_front();
            let start = Instant::now();
            let interrupt = self.interrupt.clone();
            let control_epoch = self.shortcut_epoch.clone();
            let cancelled = || interrupt.load(Ordering::SeqCst)
                || control_epoch.load(Ordering::SeqCst) != input_epoch;
            let result = match &action.kind {
                ActionKind::Click { target, .. }
                | ActionKind::Refresh { target }
                | ActionKind::CompleteSelection { target } => self.click(target, is_image_candidate_action(&action.kind), &cancelled),
                ActionKind::Key { key, .. } => self.adapter().and_then(|adapter| {
                    adapter.key_guarded(match key {
                        Key::Escape => macro_platform::Key::Escape,
                        Key::ArrowUp => macro_platform::Key::Up,
                        Key::ArrowDown => macro_platform::Key::Down,
                    }, &cancelled).map_err(|e| e.to_string())
                }),
                ActionKind::Observe { .. } => unreachable!(),
            };
            self.last_input_sequence = self.frame.as_ref().map(|f| f.sequence);
            self.last_input_started_at_ns = result.as_ref().ok().map(|receipt| receipt.started_at_ns);
            if let Ok(receipt) = &result {
                self.selection_count_after_input = Some((self.run_id, action.id, receipt.started_at_ns));
            }
            if result.is_ok() && matches!(action.kind, ActionKind::Refresh { .. }) {
                self.refresh_frame_after = self.last_input_sequence;
                self.refresh_frame_after_ns = self.last_input_started_at_ns;
            }
            self.timings.push_back(json!({"actionId":action.id,"inputCallUs":start.elapsed().as_micros() as u64,"delivered":result.is_ok()}));
            while self.timings.len() > 100 { self.timings.pop_front(); }
            let detail = result.as_ref().err().cloned().unwrap_or_default();
            self.record("input", json!({"actionId":action.id,"action":action.kind,
                "delivered":result.is_ok(),"error":detail}));
            if result.is_ok() && matches!(&action.kind, ActionKind::Click { purpose, .. }
                if purpose.starts_with("region:") || purpose.starts_with("seat:")) {
                self.nol_handoff_pending = false;
            }
            if result.as_ref().err().is_some_and(|reason| input_cancelled_before_delivery(reason)) {
                if let Some(watch) = navigation_watch { self.visual_recovery = watch; }
                self.feed(Event::ActionCancelled { id: action.id });
                return;
            }
            if automatic_context_recovery(self.engine.continuous_search(), &self.engine.snapshot())
                && result.as_ref().err().is_some_and(|reason| recoverable_context_error(reason)) {
                // These native errors originate in validation before input is
                // posted. An unknown/partial OS delivery must still fault.
                let point = match &action.kind {
                    ActionKind::Click { target: Target::Point { point }, .. }
                    | ActionKind::Refresh { target: Target::Point { point } }
                    | ActionKind::CompleteSelection { target: Target::Point { point } } => Some(*point),
                    _ => None,
                };
                if let Some(watch) = navigation_watch { self.visual_recovery = watch; }
                self.feed(Event::ActionCancelled { id: action.id });
                self.pause_for_context_at(detail, point, is_image_candidate_action(&action.kind));
                return;
            }
            self.feed(Event::ActionAcknowledged { id: action.id, success: result.is_ok(), detail });
            if navigation_requires_new_pixels(&action.kind, self.engine.continuous_search(), result.is_ok()) {
                self.await_post_input_frame();
            }
            if self.engine.snapshot().phase == Phase::Completed {
                self.visual_recovery.reset();
                self.runtime_message = None;
                self.vision_diagnostics = None;
            }
            if result.is_ok() {
                if let Some(baseline) = seat_baseline {
                    let state = self.engine.snapshot();
                    let manual_completion = state.phase == Phase::Completing
                        && self.engine.effective_completion_target().is_none();
                    if let Some(attempt) = state.delivered_attempts.into_iter()
                        .find(|attempt| attempt.action_id == action.id) {
                        if manual_completion { self.visual_recovery.reset(); }
                        else { self.visual_recovery.arm(baseline, attempt); }
                    }
                }
            }
            self.feedback_until_ms = self.now() + 5000;
            if result.is_err() { self.actions.clear(); return; }
            // Give fresh frames and controls a turn between physical inputs.
            // This adds no success-confirmation wait or per-click sleep.
            if self.engine.continuous_search() { return; }
        }
    }

    fn shortcut_start_cancelled(&self) -> bool {
        self.run_shortcut_epoch.is_some_and(|epoch| epoch != self.shortcut_epoch.load(Ordering::SeqCst))
    }

    fn visual_enabled(&self) -> bool {
        (self.engine.continuous_search() || self.profile.features.nol_selection_count)
            && self.profile.features.conflict_recovery
    }

    fn sample_visual_frame(&self) -> Result<macro_core::visual_dialog::VisualSample, String> {
        let frame = self.frame.as_ref().ok_or("안내창 감지에 사용할 현재 화면이 없습니다.")?;
        macro_core::visual_dialog::sample_frame(macro_core::vision::RgbaView {
            width: frame.width as u32, height: frame.height as u32,
            stride: frame.stride, rgba: &frame.rgba,
        }, frame.sequence).map_err(|error| error.to_string())
    }

    /// Only pixel changes on the bound browser are inspected. No text or DOM
    /// claims are made, and a missing image is never evidence of dialog closure.
    fn observe_visual_current(&mut self) -> bool {
        if !self.visual_enabled() {
            self.visual_recovery.reset();
            return false;
        }
        if !self.visual_recovery.is_armed() { return false; }
        let result = self.sample_visual_frame().and_then(|sample|
            self.visual_recovery.observe(sample).map_err(|error| error.to_string()));
        match result {
            Ok(Some(VisualSignal::Suspected { attempt, report })) => {
                self.feed(Event::DeliveredSeatResult {
                    action_id: attempt.action_id, candidate_id: attempt.candidate_id,
                    result: SeatOutcome::Conflict,
                });
                self.record("visual_dialog_suspected", json!({
                    "actionId":attempt.action_id, "sequence":report.sequence,
                    "changedBasisPoints":report.changed_basis_points,
                    "centralChangedBasisPoints":report.central_changed_basis_points,
                    "surroundingDarkenedBasisPoints":report.surrounding_darkened_basis_points,
                    "trigger":report.trigger_kind,
                }));
                self.runtime_message = Some(visual_change_message(report.trigger_kind).into());
                self.vision_diagnostics = None;
                true
            }
            Ok(Some(VisualSignal::Restored { report })) => {
                if self.engine.snapshot().phase == Phase::Conflict {
                    self.feed(Event::ConflictDialogGone);
                }
                self.runtime_message = None;
                self.vision_diagnostics = None;
                self.seat_rescan_requested = true;
                self.record("visual_dialog_restored", json!({"sequence":report.sequence}));
                true
            }
            Ok(None) => self.visual_recovery.is_incident(),
            Err(reason) => {
                self.pause_for_context(reason, None);
                true
            }
        }
    }

    fn rearm_cancelled_visual_escape(&mut self) {
        let state = self.engine.snapshot();
        if let Some(event) = known_conflict_retry_event(&state, self.visual_recovery.is_incident()) {
            // Core only rearms an unsent or explicitly retried failed Esc;
            // successful delivery stays latched. A legacy confirmation has a
            // current candidate but no visual-attempt ID.
            self.feed(event);
        }
    }

    fn click(&mut self, target: &Target, image_candidate: bool, cancelled: &dyn Fn() -> bool) -> Result<InputReceipt, String> {
        let Target::Point { point } = target else {
            return Err("브라우저 요소 연결이 아직 설정되지 않았습니다. 화면을 확인하세요.".into());
        };
        if image_candidate {
            let frame = self.frame.as_ref().ok_or("좌석 후보의 현재 화면을 받을 수 없습니다.")?;
            let local = image_candidate_local(frame, *point, cfg!(target_os = "windows"))?;
            return self.adapter()?.click_guarded(local, cancelled).map_err(|e| e.to_string());
        }
        let (sx, sy) = self.frame.as_ref().map(|f| (f.scale_x, f.scale_y))
            .or_else(|| self.desktop.capture_reference.as_ref().map(|f| (f.scale_x, f.scale_y)))
            .ok_or("현재 화면의 배율을 확인하지 못했습니다. 좌표 채집 또는 브라우저 연결을 확인하세요.")?;
        if !sx.is_finite() || !sy.is_finite() || sx <= 0.0 || sy <= 0.0 {
            return Err("화면 배율이 올바르지 않습니다. 브라우저 연결을 다시 실행하세요.".into());
        }
        self.adapter()?.click_guarded(macro_platform::Point { x: point.x / sx, y: point.y / sy }, cancelled)
            .map_err(|e| e.to_string())
    }

    fn candidate_still_available(&mut self, target: &Target) -> Result<bool, String> {
        self.refresh_candidate_frame()?;
        self.candidate_available_on_frame(target)
    }

    fn refresh_candidate_frame(&mut self) -> Result<(), String> {
        let sequence = self.frame.as_ref().map(|f| f.sequence).unwrap_or(0);
        let capture = self.adapter()?.capture_if_changed(sequence).map_err(|e| e.to_string());
        apply_candidate_capture(&mut self.frame, self.desktop.capture_reference.as_ref(), capture)
    }

    fn candidate_available_on_frame(&self, target: &Target) -> Result<bool, String> {
        if self.profile.features.seat_objects {
            let Some(frame) = self.frame.as_ref() else { return Err("최신 좌석 화면을 받을 수 없습니다.".into()); };
            if !frame_passes_input_transition(frame, self.refresh_frame_after, self.refresh_frame_after_ns) { return Ok(false); }
            let Target::Point { point } = target else { return Ok(false); };
            let state = self.engine.snapshot();
            let candidate = state.current_candidate_id.as_ref().and_then(|id| self.candidates.get(id))
                .filter(|candidate| candidate.point == *point);
            return Ok(candidate.is_some_and(|candidate| macro_core::seat_objects::seat_object_candidate_supported(
                macro_core::vision::RgbaView { width: frame.width as u32, height: frame.height as u32,
                    stride: frame.stride, rgba: &frame.rgba }, candidate, self.engine.effective_color(),
                    self.profile.color_tolerance, self.engine.effective_vision())));
        }
        seat_candidate_available_in_frame(self.frame.as_ref(), target, self.engine.effective_color(),
            self.profile.color_tolerance, self.engine.effective_vision(), self.refresh_frame_after, self.refresh_frame_after_ns)
    }

    fn default_confirmation(&self) -> Option<String> {
        let mut state = self.engine.snapshot();
        state.phase = context_confirmation_phase(&state, self.engine.confirmation_phase(), self.target_recovery.is_some());
        manual_confirmation_kind(&state, self.engine.confirmation_phase(), &self.scope)
    }

    fn continue_after_captcha(&mut self, received_epoch: u64) -> Result<Value, String> {
        if !manual_captcha_request_current(received_epoch,
            self.shortcut_epoch.load(Ordering::SeqCst), self.interrupt.load(Ordering::SeqCst)) {
            return Ok(json!({"cancelled":true}));
        }
        let resume = manual_captcha_resume_required(&self.engine, self.capture_armed)?;
        let mut profile = self.profile.clone();
        apply_execution_options(&mut profile, &self.desktop, RunMode::Normal);
        profile.sale_deadline_ms = None;
        profile.site_auto_refresh = false;
        if self.engine.has_retained_selection() && !same_post_entry_mapping(&self.engine, &profile) {
            return Err("F9 재시도 보류 · 전달한 좌석 기록과 현재 색상·좌석 범위·판정 옵션이 다릅니다. 기존 설정을 복원하세요. F4는 입력만 중지하며 좌석 선택을 해제하지 않습니다.".into());
        }
        let fresh_run = f9_starts_new_logical_run(&self.engine, self.run_id);
        if fresh_run {
            self.run_id = self.run_id.saturating_add(1);
            self.run_started_at = Instant::now();
            self.selection_count_after_input = None;
        }
        self.cancel_queued_actions();
        self.deactivate_enter_watch();
        self.deactivate_captcha_input_watch();
        self.captcha_input_watch_error = None;
        self.target_recovery = None;
        self.nol_captcha.reset();
        self.nol_handoff_pending = false;
        self.frame = None;
        self.last_sequence = None;
        self.last_scan_generation = None;
        self.last_input_sequence = None;
        self.last_input_started_at_ns = None;
        self.seat_frame_after = None;
        self.seat_frame_after_ns = None;
        self.refresh_frame_after = None;
        self.refresh_frame_after_ns = None;
        self.seat_rescan_requested = false;
        self.candidates.clear();
        self.vision_diagnostics = None;
        let actions = self.engine.restart_after_entry(profile, self.now()).map_err(|error| error.to_string())?;
        self.actions.extend(actions); // Observe only; physical inputs remain guarded below.
        self.run_shortcut_epoch = Some(received_epoch);
        // Commit the post-entry checkpoint before target validation, so a
        // transient context loss can only resume search, never old entry clicks.
        let validation = (|| -> Result<OwnedFrame, String> {
            let permissions = self.adapter()?.permissions();
            self.permissions = Some(permissions.clone());
            if !permissions.screen_capture || !permissions.input {
                return Err(format!("F9 실행 권한을 확인하세요. {}", permissions.details.join(" ")));
            }
            self.adapter()?.validate_target().map_err(|error| error.to_string())?;
            let frame = self.adapter()?.capture().map_err(|error| error.to_string())?;
            self.adapter()?.validate_target().map_err(|error| error.to_string())?;
            if self.desktop.capture_reference.as_ref().is_some_and(|reference| !same_frame_reference(reference, &frame))
                || self.restart_context.as_ref().and_then(|context| context.geometry.as_ref())
                    .is_some_and(|reference| !same_frame_reference(reference, &frame)) {
                return Err("F9 재시도 보류 · 창 크기나 배율이 달라 현재 좌표와 좌석 기록을 사용할 수 없습니다.".into());
            }
            Ok(frame)
        })();
        if !manual_captcha_request_current(received_epoch,
            self.shortcut_epoch.load(Ordering::SeqCst), self.interrupt.load(Ordering::SeqCst)) {
            return Ok(json!({"cancelled":true}));
        }
        self.restart_context = None;
        self.feedback_until_ms = self.now() + 5000;
        match validation {
            Ok(frame) => { self.frame = Some(frame); self.runtime_message = None; }
            Err(error) => {
                self.pause_for_context(error.clone(), None);
                if self.target_recovery.is_none() { return Err(error); }
            }
        }
        self.record("captcha_manual_completed", json!({
            "shortcut":"F9", "resumed":resume, "freshRun":fresh_run,
            "phase":self.engine.snapshot().phase, "recovering":self.target_recovery.is_some(),
        }));
        Ok(self.snapshot_value())
    }

    fn confirm(&mut self, kind: &str, selected_ids: Option<Vec<String>>) -> Result<(), String> {
        let state = self.engine.snapshot();
        let effective_phase = context_confirmation_phase(&state, self.engine.confirmation_phase(), self.target_recovery.is_some());
        if state.phase == Phase::Paused && effective_phase != Phase::Paused
            && ((kind == "conflict" && matches!(effective_phase, Phase::Seats | Phase::Completing))
                || (kind == "dialog_gone" && effective_phase == Phase::Conflict)) {
            // Opening settings to report an error must not hide that report behind
            // the automatic foreground hold. A deliberate Pause is never resumed.
            if kind == "conflict" { conflict_confirmation_event(&state, selected_ids.as_deref())?; }
            self.adapter()?.validate_target().map_err(|e| e.to_string())?;
            self.target_recovery = None;
            self.interrupt.store(false, Ordering::SeqCst);
            self.last_sequence = None;
            self.feed(Event::Resume);
        }
        if kind == "region_map_ready" && self.engine.snapshot().phase == Phase::Paused
            && matches!(self.engine.confirmation_phase(), Phase::Region | Phase::WaitingRegion) {
            // Explicit F6 confirmation that the seat map is already visible.
            // Validate the real foreground before resuming a paused run.
            self.adapter()?.validate_target().map_err(|e| e.to_string())?;
            self.interrupt.store(false, Ordering::SeqCst);
            self.last_sequence = None;
            self.feed(Event::Resume);
        }
        let state = self.engine.snapshot();
        self.runtime_message = None;
        match kind {
            "captcha_completed" if state.phase == Phase::Captcha => {
                if self.engine.awaits_nol_captcha() {
                    return Err("NOL 캡챠 판단 중입니다. 캡챠를 직접 완료했다면 예매창에서 ‘캡챠 이후 진행’ F9를 누르세요.".into());
                }
                self.feed(Event::CaptchaCompleted);
                self.feed(Event::Resume);
            }
            "dialog_gone" if state.phase == Phase::Conflict => {
                self.visual_recovery.reset();
                self.feed(Event::ConflictDialogGone);
            }
            "refresh_completed" if matches!(state.phase, Phase::WaitingRefresh | Phase::Seats) => {
                // Explicit user confirmation can accept an identical map.
                // Merely receiving an input acknowledgement cannot do this.
                self.refresh_frame_after = None;
                self.refresh_frame_after_ns = None;
                self.frame = None;
                self.last_scan_generation = None;
                self.vision_diagnostics = None;
                self.feed(Event::RefreshCompleted { generation: state.generation + 1 });
                self.last_sequence = None;
            }
            "region_map_ready" if matches!(state.phase, Phase::Region | Phase::WaitingRegion) => {
                self.adapter()?.validate_target().map_err(|e| e.to_string())?;
                self.await_post_input_frame();
                self.feed(Event::RegionMapReady);
                self.last_sequence = None;
            }
            "selection" if state.phase == Phase::Seats && !state.optimistic_selection && !state.selection_count_verification => {
                if let Some(ids) = selected_ids {
                    self.feed(Event::SelectionObserved { selected_ids: ids });
                } else if let Some(id) = state.current_candidate_id {
                    self.feed(Event::SeatResult { candidate_id: id, result: SeatOutcome::LocallySelected });
                } else {
                    self.feed(Event::SelectionObserved { selected_ids: state.selected_ids });
                }
            }
            "conflict" if matches!(state.phase, Phase::Seats | Phase::Completing) => {
                self.feed(conflict_confirmation_event(&state, selected_ids.as_deref())?);
            }
            "seat_map" if state.phase == Phase::Entry => {
                self.await_post_input_frame();
                self.feed(Event::EntryObserved {
                    stage: macro_core::EntryStage::SeatMap, target: None, enabled: true,
                    matched_value: None, screen_id: format!("manual:{}", self.run_id),
                });
            }
            _ => return Err("현재 단계에서 사용할 수 없는 확인 동작입니다.".into()),
        }
        // Confirmation is evidence only. Any resulting input still checks target focus.
        Ok(())
    }

    fn recheck_nol_handoff(&mut self) -> bool {
        let state = self.engine.snapshot();
        if state.mode != RunMode::Normal || !self.profile.features.nol_captcha_detection
            || !matches!(state.phase, Phase::Region | Phase::WaitingRegion | Phase::Seats) {
            return false;
        }
        let sequence = self.frame.as_ref().map_or(0, |frame| frame.sequence);
        let result = self.adapter()
            .and_then(|adapter| adapter.capture_observation(sequence).map_err(|error| error.to_string()))
            .and_then(|observation| apply_frame_observation(&mut self.frame, observation));
        if let Err(reason) = result {
            self.pause_for_context(reason, None);
            return false;
        }
        if self.interrupt.load(Ordering::SeqCst) || self.shortcut_start_cancelled() { return false; }
        let checked_at_ms = self.now();
        let frame = self.frame.as_ref().unwrap();
        if self.desktop.capture_reference.as_ref().is_some_and(|reference| !same_frame_reference(reference, frame)) {
            self.pause_for_context("화면 크기나 배율이 채집할 때와 달라졌습니다. 현재 화면에서 좌표를 다시 채집하세요.".into(), None);
            return false;
        }
        let image = macro_core::vision::RgbaView {
            width: frame.width as u32, height: frame.height as u32, stride: frame.stride, rgba: &frame.rgba,
        };
        let result = self.nol_captcha.recheck_dismissal(image, frame.sequence,
            frame.captured_at_ns, frame.last_observed_ns, checked_at_ms);
        let ready = result.as_ref().is_ok_and(|observation| observation.status == NolCaptchaStatus::Dismissed);
        self.record("nol_captcha_handoff", json!({
            "ready":ready,"frameSequence":frame.sequence,"phase":state.phase,
            "observation":result.as_ref().ok(),"error":result.as_ref().err().map(ToString::to_string),
        }));
        if self.interrupt.load(Ordering::SeqCst) || self.shortcut_start_cancelled() { return false; }
        if ready { return true; }

        // No post-CAPTCHA click has been delivered yet. Keep the original color
        // reference, discard only unsent candidates, and return to the same gate.
        // This event cannot replay the already completed entry button queue.
        self.nol_captcha.interrupt();
        let detector = self.nol_captcha.clone();
        self.cancel_queued_actions();
        self.candidates.clear();
        self.visual_recovery.reset();
        self.last_sequence = None;
        self.last_scan_generation = None;
        self.last_input_sequence = None;
        self.last_input_started_at_ns = None;
        self.seat_frame_after = None;
        self.seat_frame_after_ns = None;
        self.refresh_frame_after = None;
        self.refresh_frame_after_ns = None;
        self.vision_diagnostics = None;
        self.seat_rescan_requested = false;
        self.feed(Event::EntryObserved {
            stage: macro_core::EntryStage::Captcha, target: None, enabled: false,
            matched_value: None, screen_id: format!("nol-handoff:{}", self.run_id),
        });
        self.nol_captcha = detector;
        self.runtime_message = Some("NOL 캡챠 판단 · 왼쪽 아래 배경 색이 바뀔 때까지 대기 중".into());
        false
    }

    fn observe_nol_captcha(&mut self) {
        if self.capture_armed || self.now() < self.next_nol_observation_ms { return; }
        // Only the optional CAPTCHA phase uses this paced watch. Unchanged
        // Mac frames return metadata only; region/seat scanning retains its loop.
        self.next_nol_observation_ms = self.now().saturating_add(40);
        let began = Instant::now();
        let sequence = self.frame.as_ref().map_or(0, |frame| frame.sequence);
        let observation = self.adapter()
            .and_then(|adapter| adapter.capture_observation(sequence).map_err(|error| error.to_string()));
        let result = observation.and_then(|observation| apply_frame_observation(&mut self.frame, observation));
        if let Err(reason) = result {
            self.pause_for_context(reason, None);
            return;
        }
        if self.interrupt.load(Ordering::SeqCst) || self.shortcut_start_cancelled() { return; }
        let checked_at_ms = self.now();
        let frame = self.frame.as_ref().unwrap();
        if self.desktop.capture_reference.as_ref().is_some_and(|reference| !same_frame_reference(reference, frame)) {
            self.pause_for_context("화면 크기나 배율이 채집할 때와 달라졌습니다. 현재 화면에서 좌표를 다시 채집하세요.".into(), None);
            return;
        }
        if !frame_passes_input_transition(frame, self.seat_frame_after, self.seat_frame_after_ns) { return; }
        let previous = self.nol_captcha.status();
        let image = macro_core::vision::RgbaView {
            width: frame.width as u32, height: frame.height as u32, stride: frame.stride, rgba: &frame.rgba,
        };
        let result = self.nol_captcha.observe(image, frame.sequence, frame.captured_at_ns, frame.last_observed_ns, checked_at_ms);
        let frame_sequence = frame.sequence;
        // Numeric capture/evidence metadata only: never log the challenge,
        // entered characters or a screenshot. Periodic samples also explain a
        // failed detection which leaves the status unchanged.
        let frame_metadata = json!({
            "width":frame.width,"height":frame.height,
            "capturedAtNs":frame.captured_at_ns,"lastObservedNs":frame.last_observed_ns,
        });
        match result {
            Ok(observation) => {
                self.runtime_message = Some(match observation.backdrop.gate_reason {
                    NolCaptchaGateReason::BaselineNotReady | NolCaptchaGateReason::PatchOverlapsPanel =>
                        "NOL 캡챠 판단 · 왼쪽 아래에서 기준으로 쓸 배경 색을 확인하는 중",
                    NolCaptchaGateReason::TransparentFrame =>
                        "NOL 캡챠 판단 · 유효한 화면을 기다리는 중 · 클릭 보류",
                    NolCaptchaGateReason::StaleObservation =>
                        "NOL 캡챠 판단 · 최신 화면을 기다리는 중 · 클릭 보류",
                    _ => nol_captcha_message(observation.status),
                }.into());
                if previous != observation.status
                    || self.last_nol_log_ms.is_none_or(|last| checked_at_ms.saturating_sub(last) >= 1000) {
                    self.record("nol_captcha_observation", json!({
                        "observation": observation, "frameSequence": frame_sequence,
                        "frame":frame_metadata,
                        "analysisUs": began.elapsed().as_micros() as u64,
                    }));
                    self.last_nol_log_ms = Some(checked_at_ms);
                }
                if observation.dismissed {
                    // No Enter/F6, original entry-button replay, or result-page
                    // confirmation. This evidence releases only the CAPTCHA gate.
                    self.cancel_queued_actions();
                    self.visual_recovery.reset();
                    self.candidates.clear();
                    self.vision_diagnostics = None;
                    self.last_sequence = None;
                    self.last_scan_generation = None;
                    self.last_input_sequence = None;
                    self.last_input_started_at_ns = None;
                    self.seat_frame_after = None;
                    self.seat_frame_after_ns = None;
                    self.refresh_frame_after = None;
                    self.refresh_frame_after_ns = None;
                    self.seat_rescan_requested = false;
                    if self.interrupt.load(Ordering::SeqCst) || self.shortcut_start_cancelled() {
                        self.nol_captcha.interrupt();
                        return;
                    }
                    self.feed(Event::NolCaptchaDismissed);
                    self.nol_handoff_pending = true;
                    self.runtime_message = None;
                    self.record("nol_captcha_dismissed", json!({
                        "frameSequence": frame_sequence, "phase": self.engine.snapshot().phase,
                    }));
                }
            }
            Err(error) => {
                self.nol_captcha.interrupt();
                self.runtime_message = Some(format!("NOL 캡챠 판단 · {error}"));
            }
        }
    }

    fn observe_frame(&mut self) {
        if self.engine.awaits_nol_captcha() {
            self.observe_nol_captcha();
            return;
        }
        let state = self.engine.snapshot();
        if state.phase == Phase::Completing && self.engine.effective_completion_target().is_none() {
            // A manually clicked completion cannot be intercepted here. End the
            // picking watch before the user may navigate to the next page.
            self.visual_recovery.reset();
        }
        let watching_dialog = self.visual_enabled() && self.visual_recovery.is_armed()
            && matches!(state.phase, Phase::Seats | Phase::Conflict | Phase::Completing);
        let count_request = self.engine.selection_count_request();
        if !matches!(state.phase, Phase::Region | Phase::Seats) && !watching_dialog && count_request.is_none() { return; }
        let began = Instant::now();
        let read_count = count_request.is_some() && self.now() >= self.next_selection_count_ms;
        let last_sequence = self.last_sequence.unwrap_or(0);
        let first_legacy_scan = first_legacy_scan_requested(self.engine.profile(),
            state.generation, self.last_scan_generation);
        let reuse_seat_frame = can_rescan_seat_frame(&state, &self.scope,
            self.seat_rescan_requested || first_legacy_scan);
        // A static but freshly observed counter still matters (e.g. after the
        // configured click delay). Native observation validates the window and
        // supplies observation time without pretending old pixels just changed.
        let scan = if read_count {
            self.next_selection_count_ms = self.now().saturating_add(40);
            let result = self.adapter().and_then(|adapter| adapter.capture_observation(last_sequence).map_err(|e| e.to_string()));
            result.and_then(|observation| apply_frame_observation(&mut self.frame, observation)).map(|_| {
                self.last_sequence = self.frame.as_ref().map(|frame| frame.sequence);
                true
            })
        } else {
            let result = self.adapter().and_then(|adapter| adapter.capture_if_changed(last_sequence).map_err(|e| e.to_string()));
            apply_scan_capture(&mut self.frame, &mut self.last_sequence, result, reuse_seat_frame)
        };
        match scan {
            Ok(true) => {}
            Ok(false) => return,
            Err(reason) => {
                self.pause_for_context(reason, None);
                return;
            }
        }
        if !frame_passes_input_transition(self.frame.as_ref().unwrap(), self.seat_frame_after, self.seat_frame_after_ns) {
            self.runtime_message = Some("Enter·구역 클릭 뒤 새 화면 대기 · 계속 확인 중".into());
            return;
        }
        self.seat_frame_after = None;
        self.seat_frame_after_ns = None;
        let frame = self.frame.as_ref().unwrap();
        if let Some(reference) = &self.desktop.capture_reference {
            if !same_frame_reference(reference, frame) {
                self.cancel_queued_actions();
                self.feed(Event::ContextLost { reason: "화면 크기나 배율이 채집할 때와 달라졌습니다. 현재 화면에서 좌표를 다시 채집하세요.".into() });
                return;
            }
        }
        if watching_dialog && self.observe_visual_current() { return; }
        if !matches!(state.phase, Phase::Region | Phase::Seats) && !read_count { return; }
        let candidate_frame_current = read_count && state.phase == Phase::Seats
            && self.observe_candidates_on_frame(true);
        if read_count {
            let request = count_request.unwrap();
            let frame = self.frame.as_ref().unwrap();
            let input_fresh = selection_count_frame_fresh(frame, self.run_id, request.after_action_id,
                self.selection_count_after_input);
            if input_fresh && frame_passes_input_transition(frame, self.refresh_frame_after, self.refresh_frame_after_ns)
                && self.engine.selection_count_request() == Some(request) {
                // This identifies a successful read of the currently validated
                // native stream, not a fabricated frame/capture timestamp. SCK
                // may stop emitting idle callbacks for a static count display.
                self.selection_count_poll_id = self.selection_count_poll_id.saturating_add(1);
                let observation_id = self.selection_count_poll_id;
                let (diagnosis, frame_digest) = cached_selection_count_read(frame, &mut self.selection_count_read_cache,
                    macro_core::selection_count::diagnose_selection_count);
                let count = diagnosis.read;
                if self.interrupt.load(Ordering::SeqCst) || self.shortcut_start_cancelled() { return; }
                let evidence = if let macro_core::selection_count::CountRead::Confirmed { count, heading, evidence } = count {
                    SelectionCountEvidence::capture_with_digest(frame, count, heading, evidence, frame_digest)
                } else { None };
                if self.interrupt.load(Ordering::SeqCst) || self.shortcut_start_cancelled() { return; }
                let evidence = evidence.filter(|evidence| selection_count_observation_has_map(
                    &self.engine, evidence.count, candidate_frame_current));
                if let Some(evidence) = evidence {
                    self.runtime_message = None;
                    self.feed(Event::SelectionCountObserved { request_id: request.id, observation_id, count:evidence.count });
                    // Only this observation may grant a protected action. No
                    // proof is carried forward to a later action or run.
                    if let Some(action) = self.actions.iter().find(|action|
                        self.engine.snapshot().pending_action_id == Some(action.id) && needs_selection_count_proof(&action.kind)) {
                        self.selection_count_input_proof = Some(SelectionCountInputProof {
                            run_id:self.run_id, action_id:action.id, request_id:request.id, observation_id, evidence });
                    }
                } else {
                    if candidate_frame_current || state.phase != Phase::Seats {
                        self.runtime_message = Some(selection_count_issue_message(diagnosis.issue).into());
                    }
                    self.feed(Event::SelectionCountUnavailable { request_id: request.id, observation_id });
                }
                if self.engine.snapshot().phase != Phase::Seats { return; }
            }
        }
        if !read_count || state.phase != Phase::Seats {
            self.observe_candidates_on_frame(reuse_seat_frame);
        }
        self.timings.push_back(json!({"captureAndAnalysisUs":began.elapsed().as_micros() as u64}));
        while self.timings.len() > 100 { self.timings.pop_front(); }
    }

    // With count verification, the current frame's candidates must be installed
    // before the count event can grant navigation or another seat. Otherwise an
    // old empty map could refresh away a seat which just appeared in this frame.
    fn observe_candidates_on_frame(&mut self, reuse_seat_frame: bool) -> bool {
        let state = self.engine.snapshot();
        if self.last_input_sequence == self.last_sequence && !reuse_seat_frame { return false; }
        let frame = self.frame.as_ref().unwrap();
        if state.phase == Phase::Seats && !state.optimistic_selection && !state.selection_count_verification {
            if let (Some(id), Some(selected_color)) = (&state.current_candidate_id, self.desktop.selected_color) {
                if let Some(candidate) = self.candidates.get(id) {
                    if let Some([r,g,b]) = image_candidate_local(frame, candidate.point, false).ok().and_then(|local| frame.color_at(local)) {
                        let observed = Color {r,g,b};
                        if macro_core::vision::color_matches(observed, selected_color, self.profile.color_tolerance)
                            && !macro_core::vision::color_matches(observed, self.engine.effective_color(), self.profile.color_tolerance)
                        {
                            self.feed(Event::SeatResult { candidate_id: id.clone(), result: SeatOutcome::LocallySelected });
                            return true;
                        }
                    }
                }
            }
        }
        let config = if state.phase == Phase::Region {
            let Some(roi) = self.desktop.region_roi else {
                self.runtime_message = Some("구역 탐색 범위를 현재 화면에서 지정하세요. 범례는 제외하세요.".into()); return false;
            };
            if self.desktop.stage_center.is_none() {
                self.runtime_message = Some("현재 구역 화면의 무대 중앙 기준을 채집하세요.".into()); return false;
            }
            VisionConfig { roi: Some(roi), ..self.profile.vision.clone() }
        } else {
            if self.engine.effective_vision().roi.is_none() {
                self.vision_diagnostics = None;
                self.runtime_message = Some(seat_area_guidance(&self.engine)); return false;
            }
            self.engine.effective_vision().clone()
        };
        let color = if state.phase == Phase::Region {
            self.profile.region_color.unwrap_or(self.profile.color)
        } else { self.engine.effective_color() };
        let frame = self.frame.as_ref().unwrap();
        let image = macro_core::vision::RgbaView { width: frame.width as u32, height: frame.height as u32, stride: frame.stride, rgba: &frame.rgba };
        let mut object_stats = None;
        let mut object_reasons: HashMap<String, usize> = HashMap::new();
        let extraction = if state.phase == Phase::Seats && self.profile.features.seat_objects {
            macro_core::seat_objects::extract_seat_objects_view_with_stats(image, color, self.profile.color_tolerance, &config, state.generation)
                .map(|extraction| {
                    for object in &extraction.objects {
                        if let Some(reason) = serde_json::to_value(&object.reason).ok().and_then(|v| v.as_str().map(str::to_owned)) {
                            *object_reasons.entry(reason).or_default() += 1;
                        }
                    }
                    object_stats = Some(extraction.object_stats);
                    macro_core::vision::CandidateExtraction { candidates: extraction.candidates, stats: extraction.stats }
                })
        } else if state.phase == Phase::Seats {
            macro_core::vision::extract_seat_candidates_view_with_stats(image, color, self.profile.color_tolerance, &config, state.generation)
        } else {
            macro_core::vision::extract_candidates_view_with_stats(image, color, self.profile.color_tolerance, &config, state.generation)
        };
        match extraction {
            Ok(extraction) => {
                let mut candidates = extraction.candidates;
                let awaiting_refresh_image = state.phase == Phase::Seats
                    && !frame_passes_input_transition(frame, self.refresh_frame_after, self.refresh_frame_after_ns);
                let diagnostic = json!({
                    "phase":state.phase,
                    "source":if state.phase == Phase::Region { "region_map".to_string() }
                        else { state.active_region.map(|index| format!("legacy_region_{index}")).unwrap_or("global".into()) },
                    "color":color,"tolerance":self.profile.color_tolerance,"roi":config.roi,
                    "frame":{"width":frame.width,"height":frame.height,"scaleX":frame.scale_x,"scaleY":frame.scale_y},
                    "stats":extraction.stats,"objectStats":object_stats,"objectReasons":object_reasons,
                    "awaitingRefreshImage":awaiting_refresh_image,
                    "candidatePreview":candidates.iter().take(8).map(|candidate| json!({
                        "id":candidate.id,"point":candidate.point,"geometry":candidate.geometry
                    })).collect::<Vec<_>>()
                });
                let sequence = frame.sequence;
                if self.vision_diagnostics.as_ref() != Some(&diagnostic)
                    || self.now().saturating_sub(self.last_vision_log_ms) >= 5000 {
                    self.record("vision_scan", json!({"diagnostic":diagnostic,"frameSequence":sequence}));
                    self.last_vision_log_ms = self.now();
                }
                self.vision_diagnostics = Some(diagnostic);
                self.last_scan_generation = Some(state.generation);
                self.seat_rescan_requested = false;
                if !self.profile.features.nol_selection_count || !state.selection_count_pending {
                    self.runtime_message = None;
                }
                if awaiting_refresh_image {
                    // Continue observing and timing refreshes, but never turn
                    // pre-refresh pixels into a new seat click.
                    candidates.clear();
                    self.runtime_message = Some("새로고침 뒤 새 화면 대기 · 계속 탐색 중".into());
                } else if state.phase == Phase::Seats {
                    self.refresh_frame_after = None;
                    self.refresh_frame_after_ns = None;
                }
                if state.phase == Phase::Region {
                    let stage = self.desktop.stage_center.unwrap();
                    let regions = candidates.into_iter().map(|candidate| {
                        let dx = (candidate.point.x - stage.x).abs();
                        let dy = candidate.point.y - stage.y;
                        RegionCandidate { candidate, center_rank: (dx * 100.0).round().min(u32::MAX as f64) as u32,
                            distance_to_stage: dx.hypot(dy) }
                    }).collect();
                    self.feed(Event::RegionsObserved { candidates: regions });
                } else {
                    if let Some(index) = state.active_region {
                        for candidate in &mut candidates { candidate.id = format!("zone{index}:{}", candidate.id); }
                    }
                    update_candidate_cache(&mut self.candidates, &candidates, state.current_candidate_id.as_deref());
                    self.feed(Event::CandidatesObserved { generation: state.generation, candidates });
                }
                true
            }
            Err(error) => { self.runtime_message = Some(error.to_string()); false }
        }
    }

    fn snapshot_value(&self) -> Value {
        let state = self.engine.snapshot();
        let waiting_for_seat_area = waiting_for_seat_area(&self.engine);
        let permissions = &self.permissions;
        let target = self.native.as_ref().ok().and_then(NativeAdapter::target);
        let mut checks = vec![
            json!({"id":"target","label":"브라우저 연결","ok":target.is_some(),"message":target.map(|w| w.title.clone()).unwrap_or("연결 안 됨".into())}),
            json!({"id":"capture","label":"화면 기록 권한","ok":permissions.as_ref().is_some_and(|p|p.screen_capture),"message":""}),
            json!({"id":"input","label":"입력 권한","ok":permissions.as_ref().is_some_and(|p|p.input),"message":permissions.as_ref().map(|p|p.details.join(" ")).unwrap_or_default()}),
        ];
        checks.push(json!({"id":"seat_area","label":"좌석 영역",
            "ok":self.engine.effective_vision().roi.is_some(),
            "message":if self.engine.effective_vision().roi.is_some() { "지정됨".to_string() }
                else { seat_area_guidance(&self.engine) }}));
        if requested_refresh_button_missing(&self.profile, &self.desktop, state.mode) {
            checks.push(json!({"id":"refresh_button","label":"취소표 새로고침","ok":false,
                "message":"새로고침 버튼 미지정 · 탐색만 진행합니다"}));
        }
        checks.push(json!({"id":"hotkeys","label":"단축키","ok":self.shortcut_errors.is_empty(),"message":self.shortcut_errors.join(" · ")}));
        checks.push(json!({"id":"overlay","label":"상단 상태 표시창","ok":self.overlay_error.is_none(),"message":self.overlay_error.clone().unwrap_or_default()}));
        checks.push(json!({"id":"observer","label":"날짜·회차 화면 자동 인식","ok":false,"message":"날짜·회차는 등록한 버튼 또는 직접 진행을 사용합니다. NOL 캡챠 판단은 별도 옵션입니다."}));
        let nol_enabled = state.mode == RunMode::Normal && self.profile.features.nol_captcha_detection;
        if nol_enabled {
            checks.push(json!({"id":"nol_captcha","label":"NOL 캡챠 판단",
                "ok":self.permissions.as_ref().is_some_and(|permissions| permissions.screen_capture)
                    && self.captcha_input_watch_error.is_none(),
                "message":self.captcha_input_watch_error.as_deref().unwrap_or(
                    if self.engine.awaits_nol_captcha() { nol_captcha_message(self.nol_captcha.status()) }
                    else { "일반 예매 진입 뒤 첫 문자 입력에 기준 색을 저장합니다" })}));
        }
        if state.mode == RunMode::Normal && self.profile.features.auto_captcha_resume && !nol_enabled {
            checks.push(json!({"id":"captcha_enter","label":"캡챠 Enter 감지","ok":self.enter_watch_error.is_none(),
                "message":self.enter_watch_error.as_deref().unwrap_or(if self.enter_watch_token.is_some() { "Enter 감시 중" } else { "시작 가능한 단계에서 감시합니다" })}));
        }
        let mut manual_actions: Vec<String> = self.default_confirmation().into_iter().collect();
        let confirmation_phase = context_confirmation_phase(&state, self.engine.confirmation_phase(), self.target_recovery.is_some());
        if matches!(confirmation_phase, Phase::Seats | Phase::Completing)
            && (!(state.optimistic_selection || state.selection_count_verification) || !state.delivered_attempts.is_empty()) {
            manual_actions.push("conflict".into());
        }
        let estimated_count = state.selected_ids.len() + state.attempted_ids.iter()
            .filter(|id| !state.selected_ids.contains(id)).count();
        let visual_enabled = self.visual_enabled();
        let visual_status = if !visual_enabled { "off" }
            else if self.visual_recovery.is_incident() {
                if state.pending_action_id.is_some() { "closing" } else { "waiting_restore" }
            } else if self.visual_recovery.is_armed() { "watching" } else { "off" };
        let visual_message = match visual_status {
            "closing" => visual_change_message(self.visual_recovery.latest_report().and_then(|report| report.trigger_kind)),
            "waiting_restore" => "안내창 후보 처리됨 · 화면 복귀를 감지하면 자동으로 탐색합니다",
            "watching" => "좌석 선택 중 · 중앙 화면 변화 또는 주변 어두워짐 감시",
            _ if visual_enabled && state.phase == Phase::Completing => "선택 완료 단계 · 안내창 자동 대응 종료",
            _ => "",
        };
        let seat_area_message = seat_area_guidance(&self.engine);
        let message = if waiting_for_seat_area && !self.capture_armed && self.capture_error.is_none() {
            &seat_area_message
        } else if self.visual_recovery.is_incident() && state.phase == Phase::Conflict {
            visual_message
        } else { self.runtime_message.as_deref().unwrap_or(&state.message) };
        let nol_captcha = json!({
            "enabled":nol_enabled,
            "inputWatchArmed":self.captcha_input_watch_token.is_some(),
            "referenceStored":self.nol_captcha.has_reference(),
            "status":if nol_enabled { serde_json::to_value(self.nol_captcha.status()).unwrap() } else { json!("off") },
            "message":self.captcha_input_watch_error.as_deref().unwrap_or(
                if self.engine.awaits_nol_captcha() { nol_captcha_message(self.nol_captcha.status()) } else { "" })
        });
        let mut snapshot = json!({
            "phase":state.phase,"mode":state.mode,
            "message":message,
            "running":self.active(),"paused":state.phase == Phase::Paused,
            "waitingForSeatArea":waiting_for_seat_area,
            "selectedCount":state.selected_ids.len(),"attemptedCount":state.attempted_ids.len(),
            "estimatedCount":estimated_count,
            "selectionPolicy":if state.optimistic_selection { "optimistic" } else { "observed" },
            "targetCount":state.target_seats,
            "selected_ids":state.selected_ids,"attempted_ids":state.attempted_ids,
            "delivered_attempts":state.delivered_attempts,
            "current_candidate_id":state.current_candidate_id,
            "active_region":state.active_region,
            "rejectedCount":state.rejected_ids.len(),"generation":state.generation,
            "pending_action_id":state.pending_action_id,
            "scope":self.scope,"manualActions":manual_actions,
            "runId":self.run_id,"revision":self.revision,"checks":checks,
            "boundWindow":target,"clock":self.clock,"overlay":self.overlay_enabled,
            "captureField":self.capture_field,"captureArmed":self.capture_armed,
            "captureState":self.capture_state,"captureError":self.capture_error,
            "timings":self.timings.back(),
            "visionDiagnostics":if waiting_for_seat_area { None } else { self.vision_diagnostics.as_ref() },
            "targetRecovery":self.target_recovery,
            "visualDialog":{
                "enabled":visual_enabled,"status":visual_status,"message":visual_message,
                "changedPercent":self.visual_recovery.latest_report().map(|report| f64::from(report.changed_basis_points) / 100.0),
                "centralChangedPercent":self.visual_recovery.latest_report().map(|report| f64::from(report.central_changed_basis_points) / 100.0),
                "surroundingDarkenedPercent":self.visual_recovery.latest_report().map(|report| f64::from(report.surrounding_darkened_basis_points) / 100.0)
            },
            "enterWatch":{
                "enabled":state.mode == RunMode::Normal && self.profile.features.auto_captcha_resume && !nol_enabled,
                "armed":self.enter_watch_token.is_some(),
                "message":self.enter_watch_error.as_deref().unwrap_or(if self.enter_watch_token.is_some() {
                    "캡챠 Enter 감시 중 · 다시 누르면 구역부터 다시 탐색합니다"
                } else { "" })
            },
            "nolCaptcha":nol_captcha,
            "keyboardCheck":self.keyboard_check.snapshot(self.now()),
            "browserObserverConnected":false
        });
        snapshot["controlEpoch"] = json!(self.shortcut_epoch.load(Ordering::SeqCst));
        snapshot["manualEntry"] = json!(state.mode == RunMode::Normal && self.engine.profile().features.manual_entry);
        snapshot["selectionCountVerification"] = json!(state.selection_count_verification);
        snapshot["siteSelectionCount"] = json!(state.site_selection_count);
        snapshot["selectionCountPending"] = json!(state.selection_count_pending);
        snapshot["profileRevision"] = json!(self.profile_revision);
        snapshot
    }

    fn publish(&mut self) {
        let mut value = self.snapshot_value();
        // Timing samples do not force a UI round trip for every frame.
        value.as_object_mut().unwrap().remove("revision");
        value.as_object_mut().unwrap().remove("timings");
        let signature = value.to_string();
        if signature == self.last_published { return; }
        self.last_published = signature;
        self.revision += 1;
        self.record("state", json!({"phase":value["phase"],"message":value["message"],
            "waitingForSeatArea":value["waitingForSeatArea"],
            "captureArmed":self.capture_armed,"captureState":self.capture_state,
            "captureError":self.capture_error,"pendingActionId":value["pending_action_id"]}));
        let _ = self.app.emit("state-change", self.snapshot_value());
    }
}

#[cfg(test)]
#[path = "reentrant_runtime_tests.rs"]
mod reentrant_runtime_tests;

#[cfg(test)]
#[path = "f9_any_state_tests.rs"]
mod f9_any_state_tests;
