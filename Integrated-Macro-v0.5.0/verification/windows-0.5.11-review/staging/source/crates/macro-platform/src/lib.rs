//! Native I/O only. Booking decisions belong to the shared engine.
//!
//! Coordinates are target-relative points on macOS and target-relative physical
//! pixels on Windows. Capture scale converts them to image pixels explicitly.
//! All adapter calls should run on the same dedicated worker, never the UI thread.

use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod unsupported;

#[cfg(target_os = "macos")]
use macos as os;
#[cfg(target_os = "windows")]
use windows as os;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use unsupported as os;

pub type Result<T> = std::result::Result<T, PlatformError>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlatformError {
    pub code: String,
    pub message: String,
}

impl PlatformError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}
impl Error for PlatformError {}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Point { pub x: f64, pub y: f64 }

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Rect { pub x: f64, pub y: f64, pub width: f64, pub height: f64 }

impl Rect {
    pub fn contains_relative(&self, point: Point) -> bool {
        point.x.is_finite() && point.y.is_finite()
            && point.x >= 0.0 && point.y >= 0.0
            && point.x < self.width && point.y < self.height
    }
    pub fn global(&self, point: Point) -> Result<Point> {
        if !self.contains_relative(point) {
            return Err(PlatformError::new("point_outside_target", "클릭 위치가 연결한 창 밖입니다. 좌표 채집을 다시 해 주세요."));
        }
        Ok(Point { x: self.x + point.x, y: self.y + point.y })
    }
    pub fn is_valid(&self) -> bool {
        [self.x, self.y, self.width, self.height].iter().all(|v| v.is_finite())
            && self.width > 0.0 && self.height > 0.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TargetWindow {
    pub id: u64,
    pub pid: u32,
    pub title: String,
    pub rect: Rect,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Permissions {
    pub screen_capture: bool,
    pub input: bool,
    /// Windows permission is checked against the desktop here and against the
    /// selected process's integrity level when binding it.
    pub details: Vec<String>,
}

/// Read-only keyboard diagnostics for the calling app, never a permission
/// request or proof that a callback was delivered. Secure event input is a
/// session-wide state; unsupported platform-specific checks remain None.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KeyboardInputStatus {
    pub platform: String,
    pub accessibility_trusted: Option<bool>,
    pub listen_event_access: Option<bool>,
    pub secure_event_input: Option<bool>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Key { Escape, Up, Down, Enter }

/// An Enter notification; it never consumes the browser's key.
/// `window_id == 0` means the OS event did not expose an exact window ID.
/// The caller must check token/PID/known window ID and freshly validate its
/// bound window before using this signal. It is not CAPTCHA success evidence.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnterPress {
    pub token: u64,
    pub window_id: u64,
    pub pid: u32,
    /// OS monotonic time at notification, using the same clock as
    /// OwnedFrame::captured_at_ns (Mac uptime / Windows QPC). These fields can
    /// be compared directly; neither is UNIX or runtime-relative time.
    pub observed_at_ns: u64,
}

/// Passive character-key activity, never the entered character or key
/// code. The caller must validate the same token/PID/window/freshness constraints
/// as EnterPress. This does not prove CAPTCHA completion or even field focus.
pub type CaptchaInput = EnterPress;

/// Native monotonic time immediately before posting click-down/key-down on
/// macOS, or the SendInput batch on Windows. This is not browser acknowledgement
/// or proof that a later capture contains the site's finished rendering.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputReceipt { pub started_at_ns: u64 }

/// Metadata for an Enter/character candidate callback or discarded notification;
/// never typed text or a character's key code.
/// macOS exposes a bounded diagnostic queue. Other platforms currently return
/// no native diagnostics; callers can still record signal validation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnterDiagnostic {
    pub stage: String,
    pub reason: String,
    pub token: Option<u64>,
    pub window_id: Option<u64>,
    pub source_pid: Option<i64>,
    pub recipient_pid: Option<i64>,
    pub front_pid: Option<i64>,
    pub repeated: Option<bool>,
    pub modifiers: Option<u64>,
    pub observed_at_ns: Option<u64>,
    pub age_ns: Option<u64>,
}

const ENTER_QUEUE_CAPACITY: usize = 16;
const ENTER_MAX_AGE_NS: u64 = 500_000_000;

fn fresh_enter_press(press: &EnterPress, token: u64, now_ns: u64) -> bool {
    press.token == token && press.pid != 0 && press.observed_at_ns != 0
        && now_ns.checked_sub(press.observed_at_ns).is_some_and(|age| age <= ENTER_MAX_AGE_NS)
}

fn input_cancelled() -> PlatformError {
    PlatformError::new("input_cancelled", "중지 또는 실행 변경으로 아직 전달하지 않은 입력을 취소했습니다.")
}

fn guarded_input<T>(cancelled: &dyn Fn() -> bool, validate: impl FnOnce() -> Result<()>,
    post: impl FnOnce() -> Result<T>) -> Result<T> {
    if cancelled() { return Err(input_cancelled()); }
    validate()?;
    // AX/window checks can block while F4 or a new run invalidates this input.
    if cancelled() { return Err(input_cancelled()); }
    post()
}

#[derive(Clone, Debug)]
pub struct OwnedFrame {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub rgba: Vec<u8>,
    pub origin: Point,
    pub scale_x: f64,
    pub scale_y: f64,
    /// Changes only when ScreenCaptureKit delivers an image; a static Mac
    /// screen remains usable without pretending that it is a new observation.
    pub sequence: u64,
    /// Monotonic native timestamp, comparable with EnterPress::observed_at_ns.
    /// macOS stamps arrival of a new SCK image, Windows stamps capture return;
    /// this is not a guarantee of when the site finished rendering.
    pub captured_at_ns: u64,
    pub last_observed_ns: u64,
}

/// A native idle notification may confirm that an unchanged image is still
/// current. It does not manufacture a new image sequence or capture timestamp.
pub enum FrameObservation {
    Changed(OwnedFrame),
    Unchanged { sequence: u64, captured_at_ns: u64, last_observed_ns: u64 },
}

impl OwnedFrame {
    pub fn local_to_pixel(&self, point: Point) -> Option<(usize, usize)> {
        if !point.x.is_finite() || !point.y.is_finite() || point.x < 0.0 || point.y < 0.0
            || !self.scale_x.is_finite() || !self.scale_y.is_finite()
            || self.scale_x <= 0.0 || self.scale_y <= 0.0 {
            return None;
        }
        let x = (point.x * self.scale_x).floor() as usize;
        let y = (point.y * self.scale_y).floor() as usize;
        (x < self.width && y < self.height).then_some((x, y))
    }
    pub fn pixel_to_local(&self, x: usize, y: usize) -> Option<Point> {
        if x >= self.width || y >= self.height || !self.scale_x.is_finite() || !self.scale_y.is_finite()
            || self.scale_x <= 0.0 || self.scale_y <= 0.0 {
            return None;
        }
        Some(Point { x: (x as f64 + 0.5) / self.scale_x, y: (y as f64 + 0.5) / self.scale_y })
    }
    pub fn color_at(&self, point: Point) -> Option<[u8; 3]> {
        let (x, y) = self.local_to_pixel(point)?;
        let offset = y.checked_mul(self.stride)?.checked_add(x.checked_mul(4)?)?;
        let bytes = self.rgba.get(offset..offset.checked_add(4)?)?;
        (bytes[3] >= 250).then_some([bytes[0], bytes[1], bytes[2]])
    }
}

pub struct NativeAdapter {
    driver: os::Driver,
    target: Option<TargetWindow>,
    overlays: Vec<u64>,
}

impl NativeAdapter {
    pub fn new() -> Result<Self> {
        Ok(Self { driver: os::Driver::new()?, target: None, overlays: Vec::new() })
    }
    pub fn permissions(&self) -> Permissions { os::permissions() }
    pub fn keyboard_input_status(&self) -> Result<KeyboardInputStatus> { os::keyboard_input_status() }
    pub fn windows(&self) -> Result<Vec<TargetWindow>> { os::windows() }
    pub fn foreground_window(&self) -> Result<TargetWindow> { os::foreground_window() }
    pub fn target(&self) -> Option<&TargetWindow> { self.target.as_ref() }
    pub fn bind(&mut self, target: TargetWindow) -> Result<()> {
        if !target.rect.is_valid() {
            return Err(PlatformError::new("invalid_target", "연결할 창의 크기나 위치가 올바르지 않습니다."));
        }
        os::validate(&target, None, &self.overlays)?;
        self.target = None;
        self.driver.bind(&target)?;
        self.target = Some(target);
        Ok(())
    }
    /// Only windows owned by this process are accepted; arbitrary occluding
    /// applications can never be bypassed by adding their IDs to this list.
    pub fn set_overlay_windows(&mut self, ids: Vec<u64>) -> Result<()> {
        os::configure_overlays(&ids)?;
        self.overlays = ids;
        Ok(())
    }
    pub fn validate_target(&self) -> Result<()> {
        os::validate(self.bound()?, None, &self.overlays)
    }
    /// Read-only retry check for a previously blocked input point.
    pub fn validate_point(&self, point: Point) -> Result<()> {
        let target = self.bound()?;
        let global = target.rect.global(point)?;
        os::validate(target, Some(global), &self.overlays)
    }
    /// Explicit UI action only: return to the exact previously bound window,
    /// then require fresh identity, geometry and foreground validation. Never
    /// invoke from observation/input loops or silently replace the binding.
    pub fn activate_target(&self) -> Result<()> {
        os::activate(self.bound()?)?;
        self.validate_target()
    }
    pub fn capture(&mut self) -> Result<OwnedFrame> {
        let target = self.bound()?.clone();
        os::validate_observation(&target, &self.overlays)?;
        self.driver.capture(&target)
    }
    /// Read-only F8 calibration. Returns the sampled global pointer and frame
    /// only while that point remains visible in the bound window. Keyboard
    /// focus is not required; all input and normal capture guards stay strict.
    pub fn capture_at_pointer(&mut self) -> Result<(Point, OwnedFrame)> {
        let point = os::pointer()?;
        let target = self.bound()?.clone();
        os::validate_capture(&target, point, &self.overlays)?;
        let frame = self.driver.capture(&target)?;
        // Reject a move, resize or occlusion that occurred while reading.
        os::validate_capture(&target, point, &self.overlays)?;
        Ok((point, frame))
    }
    /// macOS returns None without allocating/copying pixels if its persistent
    /// stream has not delivered a new image. Windows captures synchronously.
    /// Clock/state deadlines still need processing when this returns None.
    pub fn capture_if_changed(&mut self, after_sequence: u64) -> Result<Option<OwnedFrame>> {
        let target = self.bound()?.clone();
        os::validate_observation(&target, &self.overlays)?;
        self.driver.capture_if_changed(&target, after_sequence)
    }
    /// Used by the optional CAPTCHA watch to confirm a stable disappearance
    /// without copying a static Mac image on every poll.
    pub fn capture_observation(&mut self, after_sequence: u64) -> Result<FrameObservation> {
        let target = self.bound()?.clone();
        os::validate_observation(&target, &self.overlays)?;
        let observation = self.driver.capture_observation(&target, after_sequence)?;
        os::validate_observation(&target, &self.overlays)?;
        Ok(observation)
    }
    /// Posts explicit move/down/up events after checking focus, identity,
    /// geometry and the window at the click point. Success means OS acceptance,
    /// not successful seat selection; the engine must observe the result.
    pub fn click(&mut self, point: Point) -> Result<()> {
        self.click_guarded(point, &|| false).map(|_| ())
    }
    /// The synchronous guard is checked after window validation and again at
    /// native posting. A down already posted still receives its matching up.
    /// Use a nonblocking predicate (for example, atomic interrupt/epoch reads).
    /// The native callback borrows it only until this method returns.
    pub fn click_guarded(&mut self, point: Point, cancelled: &dyn Fn() -> bool) -> Result<InputReceipt> {
        let target = self.bound()?.clone();
        let global = target.rect.global(point)?;
        guarded_input(cancelled, || os::validate(&target, Some(global), &self.overlays),
            || os::click_guarded(global, cancelled))
    }
    pub fn key(&mut self, key: Key) -> Result<()> {
        self.key_guarded(key, &|| false).map(|_| ())
    }
    pub fn key_guarded(&mut self, key: Key, cancelled: &dyn Fn() -> bool) -> Result<InputReceipt> {
        let target = self.bound()?;
        guarded_input(cancelled, || os::validate(target, None, &self.overlays),
            || os::key_guarded(key, cancelled))
    }
    pub fn pointer(&self) -> Result<Point> { os::pointer() }
    /// Enables a passive Enter-only listener for this run token. Changing the
    /// token or disabling clears pending events; None removes the OS listener.
    /// Modifier combinations and repeats are excluded by the native listener.
    pub fn set_enter_watch(&mut self, token: Option<u64>) -> Result<()> {
        self.driver.set_enter_watch(token)
    }
    /// Drains at most 16 Enter signals. Events older than 500 ms, future
    /// timestamps and stale tokens are discarded using the same OS clock.
    pub fn take_enter_presses(&mut self) -> Result<Vec<EnterPress>> {
        self.driver.take_enter_presses()
    }
    /// Drains diagnostic metadata without affecting pending Enter signals.
    /// Native diagnostic details are currently implemented on macOS only.
    pub fn take_enter_diagnostics(&mut self) -> Result<Vec<EnterDiagnostic>> {
        self.driver.take_enter_diagnostics()
    }
    /// Watches letter/digit/numpad-digit activity without consuming it. Shift is
    /// allowed; shortcut modifiers and repeats are excluded. Origin checks reject
    /// own-process events on macOS and flagged injected events on Windows.
    /// Paste/autofill are not supported. This watch is independent of
    /// the Enter watch; disabling either does not remove the other listener.
    pub fn set_captcha_input_watch(&mut self, token: Option<u64>) -> Result<()> {
        self.driver.set_captcha_input_watch(token)
    }
    /// Drains at most 16 metadata-only signals with the same 500 ms age limit.
    /// The runtime disables this watch after storing its first valid baseline.
    pub fn take_captcha_inputs(&mut self) -> Result<Vec<CaptchaInput>> {
        self.driver.take_captcha_inputs()
    }
    /// Drains at most 16 character-candidate callback/queue diagnostics.
    /// macOS supplies routing/filter metadata only; Windows currently returns
    /// an empty list. This does not establish that an OS listener received keys.
    pub fn take_captcha_input_diagnostics(&mut self) -> Result<Vec<EnterDiagnostic>> {
        self.driver.take_captcha_input_diagnostics()
    }
    /// Non-blocking pause/resume acknowledgement. No input permission required.
    pub fn beep(&self) -> Result<()> { os::beep() }
    fn bound(&self) -> Result<&TargetWindow> {
        self.target.as_ref().ok_or_else(|| PlatformError::new(
            "target_not_bound", "브라우저 연결이 필요합니다. 예매창을 전면에 둔 뒤 ‘브라우저 연결’을 실행하세요."
        ))
    }
}

/// Converts an application's own native window handle to an overlay ID.
///
/// # Safety
/// On macOS `handle` must be a live NSWindow pointer belonging to this process
/// and the call must run on the main thread. Windows expects a live own HWND.
pub unsafe fn owned_overlay_window_id(handle: usize) -> Result<u64> {
    os::owned_overlay_window_id(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_cancelled_during_window_validation_never_reaches_post() {
        use std::sync::{atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering}, Arc, Barrier};
        for change_epoch in [false, true] {
            let interrupted = Arc::new(AtomicBool::new(false));
            let epoch = Arc::new(AtomicU64::new(7));
            let entered = Arc::new(Barrier::new(2));
            let release = Arc::new(Barrier::new(2));
            let posted = Arc::new(AtomicUsize::new(0));
            let worker = {
                let (interrupted, epoch, entered, release, posted) =
                    (interrupted.clone(), epoch.clone(), entered.clone(), release.clone(), posted.clone());
                std::thread::spawn(move || guarded_input(
                    &|| interrupted.load(Ordering::SeqCst) || epoch.load(Ordering::SeqCst) != 7,
                    || { entered.wait(); release.wait(); Ok(()) },
                    || { posted.fetch_add(1, Ordering::SeqCst); Ok(()) }))
            };
            entered.wait();
            if change_epoch { epoch.store(8, Ordering::SeqCst); }
            else { interrupted.store(true, Ordering::SeqCst); }
            release.wait();
            assert_eq!(worker.join().unwrap().unwrap_err().code, "input_cancelled");
            assert_eq!(posted.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn cancelled_input_skips_validation_and_current_input_posts_once() {
        assert_eq!(guarded_input::<()>(&|| true, || panic!("cancelled input validated"),
            || panic!("cancelled input posted")).unwrap_err().code, "input_cancelled");
        let posted = std::cell::Cell::new(0);
        guarded_input(&|| false, || Ok(()), || { posted.set(posted.get() + 1); Ok(()) }).unwrap();
        assert_eq!(posted.get(), 1);
    }

    #[test]
    fn validation_failure_is_not_hidden_by_a_simultaneous_cancellation() {
        let cancelled = std::cell::Cell::new(false);
        let error = guarded_input::<()>(&|| cancelled.get(), || {
            cancelled.set(true);
            Err(PlatformError::new("target_closed", "synthetic validation failure"))
        }, || panic!("invalid target posted")).unwrap_err();
        assert_eq!(error.code, "target_closed");
    }

    #[test]
    fn keyboard_input_status_serializes_app_checks_and_unknowns_without_claiming_delivery() {
        let status = KeyboardInputStatus { platform: "macos".into(),
            accessibility_trusted: Some(true), listen_event_access: Some(false),
            secure_event_input: Some(false) };
        assert_eq!(serde_json::to_value(&status).unwrap(), serde_json::json!({
            "platform":"macos", "accessibilityTrusted":true,
            "listenEventAccess":false, "secureEventInput":false,
        }));
        let unknown = KeyboardInputStatus { platform: "windows".into(),
            accessibility_trusted: None, listen_event_access: None, secure_event_input: None };
        assert_eq!(serde_json::to_value(&unknown).unwrap(), serde_json::json!({
            "platform":"windows", "accessibilityTrusted":null,
            "listenEventAccess":null, "secureEventInput":null,
        }));
    }

    #[test]
    fn enter_signals_require_current_token_and_recent_monotonic_time() {
        let press = EnterPress { token: 4, window_id: 0, pid: 123, observed_at_ns: 1_000_000_000 };
        assert!(fresh_enter_press(&press, 4, 1_500_000_000));
        assert!(!fresh_enter_press(&press, 4, 1_500_000_001));
        assert!(!fresh_enter_press(&press, 5, 1_100_000_000));
        assert!(!fresh_enter_press(&press, 4, 999_999_999));
        assert!(!fresh_enter_press(&EnterPress { pid: 0, ..press }, 4, 1_100_000_000));
    }

    #[test]
    fn captcha_input_contract_has_only_routing_and_monotonic_metadata() {
        let input = CaptchaInput { token: 4, window_id: 44, pid: 123, observed_at_ns: 1_000_000_000 };
        let value = serde_json::to_value(input).unwrap();
        let fields: std::collections::BTreeSet<_> = value.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(fields, ["token", "window_id", "pid", "observed_at_ns"].into_iter().collect());
        assert!(fresh_enter_press(&input, 4, 1_500_000_000));
        assert!(!fresh_enter_press(&input, 4, 1_500_000_001));
        assert!(!fresh_enter_press(&input, 5, 1_100_000_000));
        assert!(!fresh_enter_press(&input, 4, 999_999_999));
    }

    #[test]
    fn enter_diagnostic_json_supports_callback_and_expired_queue_metadata_without_text() {
        let entries: Vec<EnterDiagnostic> = serde_json::from_value(serde_json::json!([
            {"stage":"callback","reason":"recipient_mismatch","token":7,"window_id":44,
                "source_pid":0,"recipient_pid":20,"front_pid":10,"repeated":false,
                "modifiers":0,"observed_at_ns":1_000_000_000u64},
            {"stage":"queue","reason":"stale_event","token":7,"window_id":0,
                "front_pid":10,"observed_at_ns":1_000_000_000u64,"age_ns":500_000_001u64}
        ])).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].recipient_pid, Some(20));
        assert_eq!(entries[0].age_ns, None);
        assert_eq!(entries[1].source_pid, None);
        assert_eq!(entries[1].age_ns, Some(500_000_001));
        let serialized = serde_json::to_value(&entries).unwrap();
        for record in serialized.as_array().unwrap() {
            assert!(!record.as_object().unwrap().contains_key("characters"));
            assert!(!record.as_object().unwrap().contains_key("key_code"));
        }
    }

    #[test]
    fn native_error_keeps_its_code_when_runtime_converts_it_to_text() {
        let error = PlatformError::new("target_not_foreground", "연결한 브라우저를 전면에 두세요.");
        assert_eq!(error.to_string(), "[target_not_foreground] 연결한 브라우저를 전면에 두세요.");
    }

    #[test]
    fn negative_monitor_origin_is_preserved_and_outside_points_rejected() {
        let rect = Rect { x: -1920.0, y: -200.0, width: 1000.0, height: 800.0 };
        assert_eq!(rect.global(Point { x: 1.0, y: 20.0 }).unwrap(), Point { x: -1919.0, y: -180.0 });
        for point in [Point { x: 1000.0, y: 10.0 }, Point { x: -1.0, y: 10.0 }, Point { x: f64::NAN, y: 0.0 }] {
            assert!(rect.global(point).is_err());
        }
    }

    #[test]
    fn retina_frame_uses_actual_scale_and_stride() {
        let mut rgba = vec![0; 32];
        rgba[20..24].copy_from_slice(&[10, 20, 30, 255]);
        let frame = OwnedFrame { width: 3, height: 2, stride: 16, rgba,
            origin: Point { x: -500.0, y: 0.0 }, scale_x: 2.0, scale_y: 2.0,
            sequence: 1, captured_at_ns: 1, last_observed_ns: 1 };
        assert_eq!(frame.color_at(Point { x: 0.5, y: 0.5 }), Some([10, 20, 30]));
        let local = frame.pixel_to_local(1, 1).unwrap();
        assert_eq!(frame.local_to_pixel(local), Some((1, 1)));
        assert_eq!(frame.color_at(Point { x: 1.5, y: 0.0 }), None);
    }

    #[test]
    fn invalid_frame_coordinates_never_read_another_row() {
        let frame = OwnedFrame { width: 2, height: 2, stride: 8, rgba: vec![0; 3],
            origin: Point::default(), scale_x: 1.0, scale_y: 1.0,
            sequence: 1, captured_at_ns: 1, last_observed_ns: 1 };
        assert_eq!(frame.color_at(Point { x: f64::INFINITY, y: 0.0 }), None);
        assert_eq!(frame.color_at(Point { x: 1.0, y: 1.0 }), None);
    }

    #[test]
    fn color_at_rejects_transparent_and_truncated_pixels() {
        let mut frame = OwnedFrame { width: 1, height: 1, stride: 4,
            rgba: vec![80, 90, 100, 255], origin: Point::default(),
            scale_x: 1.0, scale_y: 1.0, sequence: 1,
            captured_at_ns: 1, last_observed_ns: 1 };
        for alpha in [0, 1, 249] {
            frame.rgba[3] = alpha;
            assert_eq!(frame.color_at(Point::default()), None);
        }
        for alpha in [250, 255] {
            frame.rgba[3] = alpha;
            assert_eq!(frame.color_at(Point::default()), Some([80, 90, 100]));
        }
        frame.rgba.truncate(3);
        assert_eq!(frame.color_at(Point::default()), None);
    }
}
