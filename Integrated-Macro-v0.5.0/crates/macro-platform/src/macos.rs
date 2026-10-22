use super::*;
use std::{ffi::{c_char, c_void, CStr}, ptr};

extern "C" {
    fn mmp_free_string(value: *mut c_char);
    fn mmp_permissions() -> u32;
    fn mmp_keyboard_input_status() -> u32;
    fn mmp_windows_json(foreground: i32) -> *mut c_char;
    fn mmp_pointer(x: *mut f64, y: *mut f64) -> i32;
    fn mmp_owned_window_id(handle: *mut c_void) -> u64;
    fn mmp_overlay_owned(id: u64) -> i32;
    fn mmp_activate(id: u64, pid: u32, x: f64, y: f64, width: f64, height: f64) -> *mut c_char;
    fn mmp_validate(id: u64, pid: u32, x: f64, y: f64, width: f64, height: f64,
        has_point: i32, px: f64, py: f64, overlays: *const u64, count: i32) -> *mut c_char;
    fn mmp_validate_frame(id: u64, pid: u32, x: f64, y: f64, width: f64, height: f64,
        has_point: i32, px: f64, py: f64, overlays: *const u64, count: i32) -> *mut c_char;
    fn mmp_validate_capture(id: u64, pid: u32, x: f64, y: f64, width: f64, height: f64,
        px: f64, py: f64, overlays: *const u64, count: i32) -> *mut c_char;
    fn mmp_click_guarded(x: f64, y: f64, cancelled: unsafe extern "C" fn(*const c_void) -> i32,
        context: *const c_void, started_at_ns: *mut u64) -> *mut c_char;
    fn mmp_key_guarded(key: u16, cancelled: unsafe extern "C" fn(*const c_void) -> i32,
        context: *const c_void, started_at_ns: *mut u64) -> *mut c_char;
    fn mmp_beep();
    fn mmp_enter_new() -> *mut c_void;
    fn mmp_enter_free(handle: *mut c_void);
    fn mmp_enter_watch(handle: *mut c_void, enabled: i32, token: u64) -> *mut c_char;
    fn mmp_enter_take(handle: *mut c_void, entries: *mut u64, capacity: u64,
        count: *mut u64, now_ns: *mut u64) -> *mut c_char;
    fn mmp_enter_diagnostics_json(handle: *mut c_void) -> *mut c_char;
    fn mmp_captcha_input_new() -> *mut c_void;
    fn mmp_captcha_input_free(handle: *mut c_void);
    fn mmp_captcha_input_watch(handle: *mut c_void, enabled: i32, token: u64) -> *mut c_char;
    fn mmp_captcha_input_take(handle: *mut c_void, entries: *mut u64, capacity: u64,
        count: *mut u64, now_ns: *mut u64) -> *mut c_char;
    fn mmp_captcha_input_diagnostics_json(handle: *mut c_void) -> *mut c_char;
    fn mmp_capture_new() -> *mut c_void;
    fn mmp_capture_free(handle: *mut c_void);
    fn mmp_capture_start(handle: *mut c_void, id: u64) -> *mut c_char;
    fn mmp_capture_copy(handle: *mut c_void, buffer: *mut u8, capacity: u64, metadata: *mut u64) -> *mut c_char;
}

unsafe fn take_string(pointer: *mut c_char) -> String {
    let message = CStr::from_ptr(pointer).to_string_lossy().into_owned();
    mmp_free_string(pointer);
    message
}
unsafe fn native_result(error: *mut c_char) -> Result<()> {
    if error.is_null() { return Ok(()); }
    let message = take_string(error);
    let (code, detail) = message.split_once('|').unwrap_or(("native_error", &message));
    Err(PlatformError::new(code, detail))
}

pub fn permissions() -> Permissions {
    let flags = unsafe { mmp_permissions() };
    let mut details = Vec::new();
    if flags & 1 == 0 { details.push("시스템 설정 → 개인정보 보호 및 보안 → 화면 및 시스템 오디오 녹음: 이 앱 허용 후 다시 실행".into()); }
    if flags & 2 == 0 { details.push("시스템 설정 → 개인정보 보호 및 보안 → 손쉬운 사용: 이 앱 허용".into()); }
    Permissions { screen_capture: flags & 1 != 0, input: flags & 2 != 0, details }
}
pub fn keyboard_input_status() -> Result<KeyboardInputStatus> {
    // Each bit is independent. Accessibility/listen access alone does not
    // establish that the active monitor receives the browser's key events.
    let flags = unsafe { mmp_keyboard_input_status() };
    Ok(KeyboardInputStatus { platform: "macos".into(),
        accessibility_trusted: Some(flags & 1 != 0),
        listen_event_access: Some(flags & 2 != 0),
        secure_event_input: Some(flags & 4 != 0) })
}
fn list(foreground: bool) -> Result<Vec<TargetWindow>> {
    let pointer = unsafe { mmp_windows_json(i32::from(foreground)) };
    if pointer.is_null() { return Err(PlatformError::new("window_list_failed", "실행 중인 창 목록을 읽지 못했습니다.")); }
    parse_window_list(&unsafe { take_string(pointer) })
}
fn parse_window_list(message: &str) -> Result<Vec<TargetWindow>> {
    // Native AX failures use the same code|detail contract as input errors.
    // JSON window titles can contain pipes, so parse the structured form first.
    if !message.trim_start().starts_with('[') {
        if let Some((code, detail)) = message.split_once('|') {
            return Err(PlatformError::new(code, detail));
        }
    }
    serde_json::from_str(message)
        .map_err(|e| PlatformError::new("window_list_invalid", format!("창 정보 형식 오류: {e}")))
}
pub fn windows() -> Result<Vec<TargetWindow>> { list(false) }
pub fn foreground_window() -> Result<TargetWindow> {
    list(true)?.into_iter().next().ok_or_else(|| PlatformError::new("no_foreground_window", "예매창을 전면에 두고 ‘브라우저 연결’을 실행하세요."))
}
pub fn configure_overlays(ids: &[u64]) -> Result<()> {
    for &id in ids {
        if id > u32::MAX as u64 || unsafe { mmp_overlay_owned(id) } == 0 {
            return Err(PlatformError::new("invalid_overlay", "이 앱 소유의 상태 표시창만 가림 검사에서 제외할 수 있습니다."));
        }
    }
    Ok(())
}
pub unsafe fn owned_overlay_window_id(handle: usize) -> Result<u64> {
    let id = mmp_owned_window_id(handle as *mut c_void);
    configure_overlays(&[id])?;
    Ok(id)
}
pub fn activate(target: &TargetWindow) -> Result<()> {
    if target.id > u32::MAX as u64 { return Err(PlatformError::new("invalid_target", "연결한 창 ID가 올바르지 않습니다.")); }
    unsafe { native_result(mmp_activate(target.id, target.pid, target.rect.x, target.rect.y,
        target.rect.width, target.rect.height)) }
}
pub fn validate(target: &TargetWindow, point: Option<Point>, overlays: &[u64]) -> Result<()> {
    if target.id > u32::MAX as u64 { return Err(PlatformError::new("invalid_target", "연결한 창 ID가 올바르지 않습니다.")); }
    let p = point.unwrap_or_default();
    unsafe { native_result(mmp_validate(target.id, target.pid, target.rect.x, target.rect.y,
        target.rect.width, target.rect.height, i32::from(point.is_some()), p.x, p.y, overlays.as_ptr(), overlays.len() as i32)) }
}
pub fn validate_observation(target: &TargetWindow, overlays: &[u64]) -> Result<()> {
    if target.id > u32::MAX as u64 { return Err(PlatformError::new("invalid_target", "연결한 창 ID가 올바르지 않습니다.")); }
    // Only frame observation may reuse short-lived focus evidence. Binding,
    // starting, clicking and typing always call the fresh mmp_validate path.
    unsafe { native_result(mmp_validate_frame(target.id, target.pid, target.rect.x, target.rect.y,
        target.rect.width, target.rect.height, 0, 0.0, 0.0, overlays.as_ptr(), overlays.len() as i32)) }
}
pub fn validate_capture(target: &TargetWindow, point: Point, overlays: &[u64]) -> Result<()> {
    if target.id > u32::MAX as u64 { return Err(PlatformError::new("invalid_target", "연결한 창 ID가 올바르지 않습니다.")); }
    unsafe { native_result(mmp_validate_capture(target.id, target.pid, target.rect.x, target.rect.y,
        target.rect.width, target.rect.height, point.x, point.y, overlays.as_ptr(), overlays.len() as i32)) }
}
pub fn pointer() -> Result<Point> {
    let mut point = Point::default();
    if unsafe { mmp_pointer(&mut point.x, &mut point.y) } == 0 {
        return Err(PlatformError::new("pointer_unavailable", "마우스 위치를 읽지 못했습니다."));
    }
    Ok(point)
}
struct InputCancellation<'a> { cancelled: &'a dyn Fn() -> bool }
unsafe extern "C" fn input_is_cancelled(context: *const c_void) -> i32 {
    if context.is_null() { return 1; }
    let context = &*(context as *const InputCancellation<'_>);
    // Never unwind across Swift/C, and do not post input after a broken guard.
    i32::from(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (context.cancelled)())).unwrap_or(true))
}
pub fn click_guarded(point: Point, cancelled: &dyn Fn() -> bool) -> Result<InputReceipt> {
    let context = InputCancellation { cancelled };
    let mut started_at_ns = 0;
    // Swift invokes this borrowed context synchronously and never retains it.
    unsafe { native_result(mmp_click_guarded(point.x, point.y, input_is_cancelled,
        (&context as *const InputCancellation<'_>).cast(), &mut started_at_ns))?; }
    Ok(InputReceipt { started_at_ns })
}
pub fn key_guarded(key: Key, cancelled: &dyn Fn() -> bool) -> Result<InputReceipt> {
    let code = match key { Key::Escape => 53, Key::Up => 126, Key::Down => 125, Key::Enter => 36 };
    let context = InputCancellation { cancelled };
    let mut started_at_ns = 0;
    unsafe { native_result(mmp_key_guarded(code, input_is_cancelled,
        (&context as *const InputCancellation<'_>).cast(), &mut started_at_ns))?; }
    Ok(InputReceipt { started_at_ns })
}
pub fn beep() -> Result<()> { unsafe { mmp_beep(); } Ok(()) }

pub struct Driver {
    handle: *mut c_void,
    enter: *mut c_void,
    enter_token: Option<u64>,
    captcha_input: *mut c_void,
    captcha_input_token: Option<u64>,
}
// CaptureSession protects its persistent stream/frame with an NSLock. The
// adapter remains exclusively owned by the runtime worker (no shared &mut).
unsafe impl Send for Driver {}
impl Driver {
    pub fn new() -> Result<Self> {
        let handle = unsafe { mmp_capture_new() };
        if handle.is_null() { return Err(PlatformError::new("capture_init_failed", "화면 캡처 모듈을 초기화하지 못했습니다.")); }
        let enter = unsafe { mmp_enter_new() };
        if enter.is_null() {
            unsafe { mmp_capture_free(handle); }
            return Err(PlatformError::new("enter_watch_init_failed", "Enter 감지 모듈을 초기화하지 못했습니다."));
        }
        let captcha_input = unsafe { mmp_captcha_input_new() };
        if captcha_input.is_null() {
            unsafe { mmp_enter_free(enter); mmp_capture_free(handle); }
            return Err(PlatformError::new("captcha_input_watch_init_failed", "캡챠 첫 입력 감지 모듈을 초기화하지 못했습니다."));
        }
        Ok(Self { handle, enter, enter_token: None, captcha_input, captcha_input_token: None })
    }
    pub fn set_enter_watch(&mut self, token: Option<u64>) -> Result<()> {
        // Native code also checks permission/installation for an unchanged token.
        let result = unsafe { native_result(mmp_enter_watch(self.enter, i32::from(token.is_some()), token.unwrap_or(0))) };
        self.enter_token = if result.is_ok() { token } else { None };
        result
    }
    pub fn take_enter_presses(&mut self) -> Result<Vec<EnterPress>> {
        let Some(token) = self.enter_token else { return Ok(Vec::new()); };
        let mut entries = [0u64; ENTER_QUEUE_CAPACITY * 4];
        let mut count = 0;
        let mut now_ns = 0;
        unsafe { native_result(mmp_enter_take(self.enter, entries.as_mut_ptr(),
            ENTER_QUEUE_CAPACITY as u64, &mut count, &mut now_ns))?; }
        if count > ENTER_QUEUE_CAPACITY as u64 {
            return Err(PlatformError::new("enter_watch_invalid", "Enter 감지 응답 형식이 올바르지 않습니다."));
        }
        Ok(entries[..count as usize * 4].chunks_exact(4).filter_map(|entry| {
            let press = EnterPress { token: entry[0], window_id: entry[1],
                pid: u32::try_from(entry[2]).ok()?, observed_at_ns: entry[3] };
            fresh_enter_press(&press, token, now_ns).then_some(press)
        }).collect())
    }
    pub fn take_enter_diagnostics(&mut self) -> Result<Vec<EnterDiagnostic>> {
        let pointer = unsafe { mmp_enter_diagnostics_json(self.enter) };
        // Empty is the usual 4 ms polling path; native code allocates no JSON.
        if pointer.is_null() { return Ok(Vec::new()); }
        let message = unsafe { take_string(pointer) };
        serde_json::from_str(&message).map_err(|error| PlatformError::new(
            "enter_diagnostics_invalid", format!("Enter 진단 형식 오류: {error}")))
    }
    pub fn set_captcha_input_watch(&mut self, token: Option<u64>) -> Result<()> {
        let result = unsafe { native_result(mmp_captcha_input_watch(self.captcha_input,
            i32::from(token.is_some()), token.unwrap_or(0))) };
        self.captcha_input_token = if result.is_ok() { token } else { None };
        result
    }
    pub fn take_captcha_inputs(&mut self) -> Result<Vec<CaptchaInput>> {
        let Some(token) = self.captcha_input_token else { return Ok(Vec::new()); };
        let mut entries = [0u64; ENTER_QUEUE_CAPACITY * 4];
        let mut count = 0;
        let mut now_ns = 0;
        unsafe { native_result(mmp_captcha_input_take(self.captcha_input, entries.as_mut_ptr(),
            ENTER_QUEUE_CAPACITY as u64, &mut count, &mut now_ns))?; }
        if count > ENTER_QUEUE_CAPACITY as u64 {
            return Err(PlatformError::new("captcha_input_watch_invalid", "캡챠 첫 입력 감지 응답 형식이 올바르지 않습니다."));
        }
        Ok(entries[..count as usize * 4].chunks_exact(4).filter_map(|entry| {
            let input = CaptchaInput { token: entry[0], window_id: entry[1],
                pid: u32::try_from(entry[2]).ok()?, observed_at_ns: entry[3] };
            fresh_enter_press(&input, token, now_ns).then_some(input)
        }).collect())
    }
    pub fn take_captcha_input_diagnostics(&mut self) -> Result<Vec<EnterDiagnostic>> {
        let pointer = unsafe { mmp_captcha_input_diagnostics_json(self.captcha_input) };
        if pointer.is_null() { return Ok(Vec::new()); }
        let message = unsafe { take_string(pointer) };
        serde_json::from_str(&message).map_err(|error| PlatformError::new(
            "captcha_input_diagnostics_invalid", format!("캡챠 첫 입력 진단 형식 오류: {error}")))
    }
    pub fn bind(&mut self, target: &TargetWindow) -> Result<()> {
        unsafe { native_result(mmp_capture_start(self.handle, target.id)) }
    }
    pub fn capture_if_changed(&mut self, target: &TargetWindow, after_sequence: u64) -> Result<Option<OwnedFrame>> {
        let mut metadata = [0u64; 7];
        unsafe { native_result(mmp_capture_copy(self.handle, ptr::null_mut(), 0, metadata.as_mut_ptr()))?; }
        if metadata[3] == after_sequence { return Ok(None); }
        self.capture(target).map(Some)
    }
    pub fn capture_observation(&mut self, target: &TargetWindow, after_sequence: u64) -> Result<FrameObservation> {
        let mut metadata = [0u64; 7];
        // The native copy validates that a usable image still exists before
        // returning metadata, including on idle status notifications.
        unsafe { native_result(mmp_capture_copy(self.handle, ptr::null_mut(), 0, metadata.as_mut_ptr()))?; }
        if metadata[3] == after_sequence {
            return Ok(FrameObservation::Unchanged {
                sequence: metadata[3], captured_at_ns: metadata[4], last_observed_ns: metadata[5],
            });
        }
        self.capture(target).map(FrameObservation::Changed)
    }
    pub fn capture(&mut self, target: &TargetWindow) -> Result<OwnedFrame> {
        let mut metadata = [0u64; 7];
        unsafe { native_result(mmp_capture_copy(self.handle, ptr::null_mut(), 0, metadata.as_mut_ptr()))?; }
        let length = usize::try_from(metadata[6]).ok().filter(|n| *n > 0 && *n <= 512 * 1024 * 1024)
            .ok_or_else(|| PlatformError::new("capture_size_invalid", "캡처 화면 크기가 지원 범위를 벗어났습니다."))?;
        let mut rgba = vec![0u8; length];
        unsafe { native_result(mmp_capture_copy(self.handle, rgba.as_mut_ptr(), length as u64, metadata.as_mut_ptr()))?; }
        if metadata[6] as usize != length {
            return Err(PlatformError::new("capture_geometry_changed", "캡처 중 화면 배율이 바뀌었습니다. ‘브라우저 연결’을 다시 실행하세요."));
        }
        for pixel in rgba.chunks_exact_mut(4) { pixel.swap(0, 2); }
        let width = metadata[0] as usize;
        let height = metadata[1] as usize;
        Ok(OwnedFrame { width, height, stride: metadata[2] as usize, rgba,
            origin: Point { x: target.rect.x, y: target.rect.y },
            scale_x: width as f64 / target.rect.width, scale_y: height as f64 / target.rect.height,
            sequence: metadata[3], captured_at_ns: metadata[4], last_observed_ns: metadata[5] })
    }
}
impl Drop for Driver {
    fn drop(&mut self) { unsafe {
        mmp_enter_free(self.enter);
        mmp_captcha_input_free(self.captcha_input);
        mmp_capture_free(self.handle);
    } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_cancellation_callback_is_synchronous_and_fails_closed_on_panic() {
        let calls = std::cell::Cell::new(0);
        let cancelled = || { calls.set(calls.get() + 1); true };
        assert_eq!(click_guarded(Point::default(), &cancelled).unwrap_err().code, "input_cancelled");
        assert_eq!(key_guarded(Key::Escape, &cancelled).unwrap_err().code, "input_cancelled");
        assert_eq!(calls.get(), 2);
        let panicking = || -> bool { panic!("synthetic guard failure") };
        assert_eq!(key_guarded(Key::Escape, &panicking).unwrap_err().code, "input_cancelled");
    }

    #[test]
    fn focused_window_error_keeps_actionable_native_code() {
        let error = parse_window_list("accessibility_permission|손쉬운 사용에서 Integrated Macro를 허용하세요.").unwrap_err();
        assert_eq!(error.code, "accessibility_permission");
        assert_eq!(error.message, "손쉬운 사용에서 Integrated Macro를 허용하세요.");
    }

    #[test]
    fn window_title_pipe_is_not_mistaken_for_native_error() {
        let windows = parse_window_list(r#"[{"id":123013,"pid":19961,"title":"Chrome | 예매","rect":{"x":0,"y":33,"width":1512,"height":891}}]"#).unwrap();
        assert_eq!(windows[0].id, 123013);
        assert_eq!(windows[0].title, "Chrome | 예매");
        assert_eq!(parse_window_list("broken").unwrap_err().code, "window_list_invalid");
    }
}
