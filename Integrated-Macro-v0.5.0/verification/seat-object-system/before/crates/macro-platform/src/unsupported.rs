use super::*;
fn unavailable<T>() -> Result<T> {
    Err(PlatformError::new("unsupported_platform", "실제 화면 캡처와 입력은 Windows와 macOS에서 지원합니다."))
}
pub fn permissions() -> Permissions {
    Permissions { screen_capture: false, input: false, details: vec!["Windows 또는 macOS에서 실행하세요.".into()] }
}
pub fn keyboard_input_status() -> Result<KeyboardInputStatus> {
    Ok(KeyboardInputStatus { platform: std::env::consts::OS.into(),
        accessibility_trusted: None, listen_event_access: None, secure_event_input: None })
}
pub fn windows() -> Result<Vec<TargetWindow>> { unavailable() }
pub fn foreground_window() -> Result<TargetWindow> { unavailable() }
pub fn configure_overlays(_: &[u64]) -> Result<()> { unavailable() }
pub unsafe fn owned_overlay_window_id(_: usize) -> Result<u64> { unavailable() }
pub fn activate(_: &TargetWindow) -> Result<()> { unavailable() }
pub fn validate(_: &TargetWindow, _: Option<Point>, _: &[u64]) -> Result<()> { unavailable() }
pub fn validate_observation(target: &TargetWindow, overlays: &[u64]) -> Result<()> { validate(target, None, overlays) }
pub fn validate_capture(_: &TargetWindow, _: Point, _: &[u64]) -> Result<()> { unavailable() }
pub fn pointer() -> Result<Point> { unavailable() }
pub fn click_guarded(_: Point, cancelled: &dyn Fn() -> bool) -> Result<InputReceipt> {
    if cancelled() { Err(input_cancelled()) } else { unavailable() }
}
pub fn key_guarded(_: Key, cancelled: &dyn Fn() -> bool) -> Result<InputReceipt> {
    if cancelled() { Err(input_cancelled()) } else { unavailable() }
}
pub fn beep() -> Result<()> { unavailable() }
pub struct Driver;
impl Driver {
    pub fn new() -> Result<Self> { unavailable() }
    pub fn bind(&mut self, _: &TargetWindow) -> Result<()> { unavailable() }
    pub fn capture(&mut self, _: &TargetWindow) -> Result<OwnedFrame> { unavailable() }
    pub fn capture_if_changed(&mut self, _: &TargetWindow, _: u64) -> Result<Option<OwnedFrame>> { unavailable() }
    pub fn capture_observation(&mut self, _: &TargetWindow, _: u64) -> Result<FrameObservation> { unavailable() }
    pub fn set_enter_watch(&mut self, token: Option<u64>) -> Result<()> {
        if token.is_none() { Ok(()) } else { unavailable() }
    }
    pub fn take_enter_presses(&mut self) -> Result<Vec<EnterPress>> { unavailable() }
    pub fn take_enter_diagnostics(&mut self) -> Result<Vec<EnterDiagnostic>> { Ok(Vec::new()) }
    pub fn set_captcha_input_watch(&mut self, token: Option<u64>) -> Result<()> {
        if token.is_none() { Ok(()) } else { unavailable() }
    }
    pub fn take_captcha_inputs(&mut self) -> Result<Vec<CaptchaInput>> { unavailable() }
    pub fn take_captcha_input_diagnostics(&mut self) -> Result<Vec<EnterDiagnostic>> { Ok(Vec::new()) }
}
