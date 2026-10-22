//! Baseline native Windows adapter. GDI capture is synchronous and intentionally
//! not represented as a measured DXGI/Windows Graphics Capture performance win.
use super::*;
use std::{
    cell::RefCell, ffi::c_void, mem::size_of, ptr,
    sync::{atomic::{AtomicBool, AtomicU32, Ordering}, mpsc, Arc},
    thread::{self, JoinHandle}, time::Duration,
};

type Handle = *mut c_void;
type Hwnd = Handle;
type Hdc = Handle;
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct WinPoint { x: i32, y: i32 }
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct WinRect { left: i32, top: i32, right: i32, bottom: i32 }
#[repr(C)]
#[derive(Clone, Copy)]
struct MouseInput { dx: i32, dy: i32, mouse_data: u32, flags: u32, time: u32, extra_info: usize }
#[repr(C)]
#[derive(Clone, Copy)]
struct KeyboardInput { vk: u16, scan: u16, flags: u32, time: u32, extra_info: usize }
#[repr(C)]
#[derive(Clone, Copy)]
union InputData { mouse: MouseInput, keyboard: KeyboardInput }
#[repr(C)]
#[derive(Clone, Copy)]
struct Input { kind: u32, data: InputData }
#[repr(C)]
struct BitmapInfoHeader {
    size: u32, width: i32, height: i32, planes: u16, bit_count: u16,
    compression: u32, size_image: u32, x_pixels_per_meter: i32, y_pixels_per_meter: i32,
    colors_used: u32, colors_important: u32,
}
#[repr(C)]
struct BitmapInfo { header: BitmapInfoHeader, colors: [u32; 1] }
#[repr(C)]
struct SidAndAttributes { sid: *mut c_void, attributes: u32 }
#[repr(C)]
#[derive(Default)]
struct WinMessage {
    window: Hwnd, message: u32, wparam: usize, lparam: isize,
    time: u32, point: WinPoint, private: u32,
}
#[repr(C)]
struct LowLevelKey { vk: u32, scan: u32, flags: u32, time: u32, extra_info: usize }

#[link(name = "user32")]
extern "system" {
    fn EnumWindows(callback: unsafe extern "system" fn(Hwnd, isize) -> i32, data: isize) -> i32;
    fn GetForegroundWindow() -> Hwnd;
    fn SetForegroundWindow(window: Hwnd) -> i32;
    fn ShowWindowAsync(window: Hwnd, command: i32) -> i32;
    fn GetWindowRect(window: Hwnd, rect: *mut WinRect) -> i32;
    fn GetWindowThreadProcessId(window: Hwnd, pid: *mut u32) -> u32;
    fn GetWindowTextW(window: Hwnd, text: *mut u16, max_count: i32) -> i32;
    fn GetWindowTextLengthW(window: Hwnd) -> i32;
    fn IsWindowVisible(window: Hwnd) -> i32;
    fn IsIconic(window: Hwnd) -> i32;
    fn IsWindow(window: Hwnd) -> i32;
    fn GetTopWindow(window: Hwnd) -> Hwnd;
    fn GetWindow(window: Hwnd, command: u32) -> Hwnd;
    fn GetCursorPos(point: *mut WinPoint) -> i32;
    fn GetAsyncKeyState(key: i32) -> i16;
    fn GetSystemMetrics(index: i32) -> i32;
    fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
    fn MessageBeep(kind: u32) -> i32;
    fn GetDC(window: Hwnd) -> Hdc;
    fn ReleaseDC(window: Hwnd, dc: Hdc) -> i32;
    fn SetThreadDpiAwarenessContext(context: Handle) -> Handle;
    fn SetWindowDisplayAffinity(window: Hwnd, affinity: u32) -> i32;
    fn OpenInputDesktop(flags: u32, inherit: i32, access: u32) -> Handle;
    fn CloseDesktop(desktop: Handle) -> i32;
    fn SetWindowsHookExW(kind: i32, callback: unsafe extern "system" fn(i32, usize, isize) -> isize,
        module: Handle, thread: u32) -> Handle;
    fn UnhookWindowsHookEx(hook: Handle) -> i32;
    fn CallNextHookEx(hook: Handle, code: i32, message: usize, data: isize) -> isize;
    fn PeekMessageW(message: *mut WinMessage, window: Hwnd, first: u32, last: u32, remove: u32) -> i32;
    fn TranslateMessage(message: *const WinMessage) -> i32;
    fn DispatchMessageW(message: *const WinMessage) -> isize;
    fn PostThreadMessageW(thread: u32, message: u32, wparam: usize, lparam: isize) -> i32;
    fn MsgWaitForMultipleObjectsEx(count: u32, handles: *const Handle, milliseconds: u32,
        wake_mask: u32, flags: u32) -> u32;
}
#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(dc: Hdc) -> Hdc;
    fn CreateDIBSection(dc: Hdc, info: *const BitmapInfo, usage: u32, bits: *mut *mut c_void, section: Handle, offset: u32) -> Handle;
    fn SelectObject(dc: Hdc, object: Handle) -> Handle;
    fn DeleteObject(object: Handle) -> i32;
    fn DeleteDC(dc: Hdc) -> i32;
    fn BitBlt(destination: Hdc, x: i32, y: i32, width: i32, height: i32, source: Hdc, sx: i32, sy: i32, rop: u32) -> i32;
    fn GdiFlush() -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetLastError() -> u32;
    fn GetCurrentProcessId() -> u32;
    fn GetCurrentProcess() -> Handle;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
    fn QueryPerformanceCounter(value: *mut i64) -> i32;
    fn QueryPerformanceFrequency(value: *mut i64) -> i32;
    fn GetCurrentThreadId() -> u32;
    fn GetModuleHandleW(name: *const u16) -> Handle;
}
#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(token: Handle, kind: u32, buffer: *mut c_void, length: u32, needed: *mut u32) -> i32;
    fn GetSidSubAuthorityCount(sid: *const c_void) -> *mut u8;
    fn GetSidSubAuthority(sid: *const c_void, index: u32) -> *mut u32;
}

fn error(code: &str, action: &str) -> PlatformError {
    PlatformError::new(code, format!("{action} (Windows 오류 {})", unsafe { GetLastError() }))
}
struct DpiGuard(Handle);
impl DpiGuard {
    fn new() -> Result<Self> {
        let previous = unsafe { SetThreadDpiAwarenessContext(-4isize as Handle) };
        if previous.is_null() { return Err(error("dpi_context_failed", "화면 배율 설정 실패. Windows 10 1703 이상에서 실행하세요.")); }
        Ok(Self(previous))
    }
}
impl Drop for DpiGuard {
    fn drop(&mut self) { unsafe { SetThreadDpiAwarenessContext(self.0); } }
}
fn hwnd(id: u64) -> Result<Hwnd> {
    usize::try_from(id).ok().filter(|id| *id != 0).map(|id| id as Hwnd)
        .ok_or_else(|| PlatformError::new("invalid_target", "창 ID가 올바르지 않습니다."))
}
unsafe fn window_info(window: Hwnd) -> Option<TargetWindow> {
    if IsWindowVisible(window) == 0 || IsIconic(window) != 0 { return None; }
    let mut rect = WinRect::default();
    let mut pid = 0;
    if GetWindowRect(window, &mut rect) == 0 || GetWindowThreadProcessId(window, &mut pid) == 0 { return None; }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 { return None; }
    let length = GetWindowTextLengthW(window).clamp(0, 16384);
    let mut text = vec![0u16; length as usize + 1];
    let count = GetWindowTextW(window, text.as_mut_ptr(), text.len() as i32).max(0) as usize;
    Some(TargetWindow { id: window as usize as u64, pid, title: String::from_utf16_lossy(&text[..count]),
        rect: Rect { x: rect.left as f64, y: rect.top as f64, width: width as f64, height: height as f64 } })
}
unsafe extern "system" fn collect_window(window: Hwnd, context: isize) -> i32 {
    if let Some(info) = window_info(window) {
        if !info.title.is_empty() { (*(context as *mut Vec<TargetWindow>)).push(info); }
    }
    1
}
pub fn windows() -> Result<Vec<TargetWindow>> {
    let _dpi = DpiGuard::new()?;
    let mut result = Vec::new();
    if unsafe { EnumWindows(collect_window, &mut result as *mut Vec<TargetWindow> as isize) } == 0 {
        return Err(error("window_list_failed", "실행 중인 창 목록을 읽지 못했습니다."));
    }
    Ok(result)
}
pub fn foreground_window() -> Result<TargetWindow> {
    let _dpi = DpiGuard::new()?;
    unsafe { window_info(GetForegroundWindow()) }
        .ok_or_else(|| PlatformError::new("no_foreground_window", "예매창을 전면에 두고 ‘브라우저 연결’을 실행하세요."))
}
pub fn permissions() -> Permissions {
    unsafe {
        let screen = GetDC(ptr::null_mut());
        let screen_capture = !screen.is_null();
        if screen_capture { ReleaseDC(ptr::null_mut(), screen); }
        let desktop = OpenInputDesktop(0, 0, 0x0001 | 0x0080); // READOBJECTS | WRITEOBJECTS
        let input = !desktop.is_null();
        if input { CloseDesktop(desktop); }
        let mut details = Vec::new();
        if !screen_capture { details.push("현재 데스크톱 화면을 읽을 수 없습니다. 잠금 화면을 해제해 주세요.".into()); }
        if !input { details.push("현재 입력 데스크톱에 접근할 수 없습니다. 잠금 화면 또는 UAC 화면을 닫아 주세요.".into()); }
        Permissions { screen_capture, input, details }
    }
}
pub fn keyboard_input_status() -> Result<KeyboardInputStatus> {
    // These macOS checks have no equivalent in this adapter. None must not
    // be displayed as a denied Windows permission or as confirmed delivery.
    Ok(KeyboardInputStatus { platform: "windows".into(),
        accessibility_trusted: None, listen_event_access: None, secure_event_input: None })
}
pub fn configure_overlays(ids: &[u64]) -> Result<()> {
    for &id in ids {
        let window = hwnd(id)?;
        let mut pid = 0;
        unsafe {
            if IsWindow(window) == 0 || GetWindowThreadProcessId(window, &mut pid) == 0 || pid != GetCurrentProcessId() {
                return Err(PlatformError::new("invalid_overlay", "이 앱 소유의 상태 표시창만 가림 검사에서 제외할 수 있습니다."));
            }
            if SetWindowDisplayAffinity(window, 0x00000011) == 0 {
                return Err(error("overlay_capture_exclusion_failed", "상태 표시창을 화면 캡처에서 제외하지 못했습니다. 표시창을 끄거나 좌석 영역 밖으로 옮겨 주세요."));
            }
        }
    }
    Ok(())
}
pub unsafe fn owned_overlay_window_id(handle: usize) -> Result<u64> {
    configure_overlays(&[handle as u64])?;
    Ok(handle as u64)
}
unsafe fn process_integrity(process: Handle) -> Result<u32> {
    let mut token = ptr::null_mut();
    if OpenProcessToken(process, 0x0008, &mut token) == 0 { return Err(error("input_integrity_unknown", "브라우저 입력 권한을 확인하지 못했습니다.")); }
    let result = (|| {
        let mut needed = 0;
        GetTokenInformation(token, 25, ptr::null_mut(), 0, &mut needed); // TokenIntegrityLevel
        if needed == 0 || needed > 65536 { return Err(error("input_integrity_unknown", "브라우저 입력 권한 정보를 읽지 못했습니다.")); }
        // usize storage guarantees SID_AND_ATTRIBUTES pointer alignment.
        let mut storage = vec![0usize; (needed as usize + size_of::<usize>() - 1) / size_of::<usize>()];
        if GetTokenInformation(token, 25, storage.as_mut_ptr().cast(), needed, &mut needed) == 0 {
            return Err(error("input_integrity_unknown", "브라우저 입력 권한 정보를 읽지 못했습니다."));
        }
        let sid = (*(storage.as_ptr() as *const SidAndAttributes)).sid;
        if sid.is_null() { return Err(error("input_integrity_unknown", "브라우저 입력 권한 SID가 비어 있습니다.")); }
        let count = GetSidSubAuthorityCount(sid);
        if count.is_null() || *count == 0 { return Err(error("input_integrity_unknown", "브라우저 입력 권한 SID가 올바르지 않습니다.")); }
        let authority = GetSidSubAuthority(sid, u32::from(*count - 1));
        if authority.is_null() { return Err(error("input_integrity_unknown", "브라우저 입력 권한을 확인하지 못했습니다.")); }
        Ok(*authority)
    })();
    CloseHandle(token);
    result
}
fn check_integrity(pid: u32) -> Result<()> {
    unsafe {
        let target = OpenProcess(0x1000, 0, pid); // PROCESS_QUERY_LIMITED_INFORMATION
        if target.is_null() { return Err(error("input_target_access", "브라우저 프로세스에 접근하지 못했습니다.")); }
        let target_level = process_integrity(target);
        CloseHandle(target);
        if target_level? > process_integrity(GetCurrentProcess())? {
            return Err(PlatformError::new("input_integrity_blocked", "브라우저가 더 높은 관리자 권한으로 실행 중이라 입력이 차단됩니다. 브라우저를 일반 권한으로 다시 실행하세요."));
        }
    }
    Ok(())
}
pub fn validate(target: &TargetWindow, point: Option<Point>, overlays: &[u64]) -> Result<()> {
    validate_window(target, point, overlays, true)
}
pub fn activate(target: &TargetWindow) -> Result<()> {
    let _dpi = DpiGuard::new()?;
    let handle = hwnd(target.id)?;
    let check_identity = || -> Result<()> {
        let mut pid = 0;
        unsafe {
            if IsWindow(handle) == 0 || GetWindowThreadProcessId(handle, &mut pid) == 0 {
                return Err(PlatformError::new("target_closed", "연결한 예매창이 닫혔습니다. 실제 예매창에서 F7로 다시 연결하세요."));
            }
        }
        if pid != target.pid {
            return Err(PlatformError::new("target_identity_changed", "연결한 창의 프로세스가 바뀌었습니다. 실제 예매창에서 F7로 다시 연결하세요."));
        }
        Ok(())
    };
    check_identity()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
    unsafe {
        if IsIconic(handle) != 0 {
            // Minimized Windows bounds are not the saved on-screen bounds.
            // Restore the exact HWND, then verify geometry before activation.
            if ShowWindowAsync(handle, 9) == 0 { // SW_RESTORE
                return Err(error("target_activation_failed", "연결한 예매창의 최소화를 해제하지 못했습니다. 예매창을 직접 열고 F1으로 시작하세요."));
            }
            while IsIconic(handle) != 0 && std::time::Instant::now() < deadline {
                check_identity()?;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
    check_identity()?;
    validate_window(target, None, &[], false)?;
    // Windows may deny foreground transfer. Never bypass that restriction with
    // injected keys or attached input threads; confirm the actual foreground.
    unsafe { SetForegroundWindow(handle); }
    loop {
        check_identity()?;
        validate_window(target, None, &[], false)?;
        if unsafe { GetForegroundWindow() } == handle {
            return validate(target, None, &[]);
        }
        if std::time::Instant::now() >= deadline {
            return Err(PlatformError::new("target_activation_failed", "연결한 예매창을 전면으로 가져오지 못했습니다. 예매창을 직접 클릭한 뒤 F1으로 시작하세요."));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
pub fn validate_observation(target: &TargetWindow, overlays: &[u64]) -> Result<()> {
    validate(target, None, overlays)
}
pub fn validate_capture(target: &TargetWindow, point: Point, overlays: &[u64]) -> Result<()> {
    let relative = Point { x: point.x - target.rect.x, y: point.y - target.rect.y };
    if !target.rect.contains_relative(relative) {
        return Err(PlatformError::new("point_outside_target", "마우스가 연결한 예매창 밖에 있습니다. 대상 위에 놓고 F8을 다시 누르세요."));
    }
    validate_window(target, Some(point), overlays, false)
}
fn validate_window(target: &TargetWindow, point: Option<Point>, overlays: &[u64], require_foreground: bool) -> Result<()> {
    let _dpi = DpiGuard::new()?;
    let handle = hwnd(target.id)?;
    unsafe {
        if require_foreground && GetForegroundWindow() != handle {
            return Err(PlatformError::new("target_not_foreground", "연결한 예매창이 전면이 아닙니다. 예매창으로 돌아온 뒤 다시 시작하세요."));
        }
        let current = window_info(handle).ok_or_else(|| PlatformError::new("target_closed", "연결한 창이 닫혔거나 최소화되었습니다. ‘브라우저 연결’을 다시 실행하세요."))?;
        if current.pid != target.pid {
            return Err(PlatformError::new("target_identity_changed", "연결한 창의 프로세스가 바뀌었습니다. ‘브라우저 연결’을 다시 실행하세요."));
        }
        if current.rect != target.rect {
            return Err(PlatformError::new("target_geometry_changed", "브라우저 창 위치·크기·배율이 바뀌었습니다. ‘브라우저 연결’과 좌표 채집을 다시 실행하세요."));
        }
        if let Some(point) = point {
            let mut window = GetTopWindow(ptr::null_mut());
            let mut examined = 0;
            while !window.is_null() && examined < 10000 {
                examined += 1;
                if IsWindowVisible(window) != 0 && IsIconic(window) == 0 {
                    let mut rect = WinRect::default();
                    if GetWindowRect(window, &mut rect) != 0
                        && point.x >= f64::from(rect.left) && point.x < f64::from(rect.right)
                        && point.y >= f64::from(rect.top) && point.y < f64::from(rect.bottom) {
                        let id = window as usize as u64;
                        if id == target.id { return Ok(()); }
                        let mut pid = 0;
                        GetWindowThreadProcessId(window, &mut pid);
                        if !(overlays.contains(&id) && pid == GetCurrentProcessId()) {
                            return Err(PlatformError::new("target_occluded", "클릭 위치가 다른 창에 가려져 있습니다. 해당 창을 옮긴 뒤 다시 시작하세요."));
                        }
                    }
                }
                window = GetWindow(window, 2); // GW_HWNDNEXT
            }
            return Err(PlatformError::new("target_not_visible", "클릭 위치에서 연결한 예매창을 확인할 수 없습니다."));
        }
    }
    Ok(())
}
pub fn pointer() -> Result<Point> {
    let _dpi = DpiGuard::new()?;
    let mut point = WinPoint::default();
    if unsafe { GetCursorPos(&mut point) } == 0 { return Err(error("pointer_unavailable", "마우스 위치를 읽지 못했습니다.")); }
    Ok(Point { x: point.x as f64, y: point.y as f64 })
}
fn send(inputs: &[Input], cancelled: &dyn Fn() -> bool) -> Result<InputReceipt> {
    if cancelled() { return Err(input_cancelled()); }
    if [0x10, 0x11, 0x12, 0x5b, 0x5c].iter().any(|key| unsafe { GetAsyncKeyState(*key) } < 0) {
        return Err(PlatformError::new("modifier_pressed", "Shift·Ctrl·Alt·Windows 키를 놓은 뒤 다시 시작하세요."));
    }
    // Keep the final guard adjacent to the OS batch, after modifier checks.
    if cancelled() { return Err(input_cancelled()); }
    let started_at_ns = now_ns();
    if started_at_ns == 0 {
        return Err(PlatformError::new("input_clock_unavailable", "입력 시각을 확인하지 못해 전송을 보류했습니다."));
    }
    if cancelled() { return Err(input_cancelled()); }
    let accepted = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<Input>() as i32) };
    if accepted as usize != inputs.len() {
        let failure = error("input_rejected", &format!("실제 입력 전송이 차단되었습니다 ({accepted}/{}개). 브라우저 권한과 잠금 화면을 확인하세요.", inputs.len()));
        if accepted > 0 {
            // The last item is always the matching release. Avoid leaving a
            // pressed key/button behind after a partially accepted batch.
            if let Some(release) = inputs.last() {
                unsafe { SendInput(1, release, size_of::<Input>() as i32); }
            }
        }
        // Do not claim browser acknowledgement even if every event was queued.
        return Err(failure);
    }
    Ok(InputReceipt { started_at_ns })
}
pub fn click_guarded(point: Point, cancelled: &dyn Fn() -> bool) -> Result<InputReceipt> {
    let _dpi = DpiGuard::new()?;
    let (left, top, width, height) = unsafe {
        (GetSystemMetrics(76), GetSystemMetrics(77), GetSystemMetrics(78), GetSystemMetrics(79))
    };
    if width <= 1 || height <= 1 { return Err(PlatformError::new("desktop_size_invalid", "전체 모니터 크기를 읽지 못했습니다.")); }
    let dx = (((point.x - f64::from(left) + 0.5) * 65536.0 / f64::from(width)).floor()) as i32;
    let dy = (((point.y - f64::from(top) + 0.5) * 65536.0 / f64::from(height)).floor()) as i32;
    if !(0..=65535).contains(&dx) || !(0..=65535).contains(&dy) {
        return Err(PlatformError::new("point_outside_desktop", "클릭 위치가 모니터 영역 밖입니다."));
    }
    let input = |flags| Input { kind: 0, data: InputData { mouse: MouseInput {
        dx, dy, mouse_data: 0, flags, time: 0, extra_info: 0,
    }}};
    // MOVE | ABSOLUTE | VIRTUALDESK, then left down/up at that position.
    send(&[input(0x0001 | 0x8000 | 0x4000), input(0x0002), input(0x0004)], cancelled)
}
pub fn key_guarded(key: Key, cancelled: &dyn Fn() -> bool) -> Result<InputReceipt> {
    let vk = match key { Key::Escape => 0x1b, Key::Up => 0x26, Key::Down => 0x28, Key::Enter => 0x0d };
    let extended = if matches!(key, Key::Up | Key::Down) { 0x0001 } else { 0 };
    let input = |flags| Input { kind: 1, data: InputData { keyboard: KeyboardInput {
        vk, scan: 0, flags, time: 0, extra_info: 0,
    }}};
    send(&[input(extended), input(extended | 0x0002)], cancelled)
}
pub fn beep() -> Result<()> {
    if unsafe { MessageBeep(0) } == 0 {
        return Err(error("beep_failed", "상태 알림음을 재생하지 못했습니다."));
    }
    Ok(())
}
fn now_ns() -> u64 {
    unsafe {
        let mut counter = 0;
        let mut frequency = 0;
        if QueryPerformanceCounter(&mut counter) == 0 || QueryPerformanceFrequency(&mut frequency) == 0 || frequency <= 0 { return 0; }
        ((counter as u128 * 1_000_000_000) / frequency as u128) as u64
    }
}

struct EnterContext {
    token: u64,
    sender: mpsc::SyncSender<EnterPress>,
    pressed: [bool; 2],
    held_when_armed: bool,
}

impl EnterContext {
    fn transition(&mut self, key: &LowLevelKey, down: bool) -> bool {
        // Never count synthesized keys or let them alter physical repeat state.
        if key.vk != 0x0d || key.flags & (0x10 | 0x02) != 0 { return false; }
        let index = usize::from(key.flags & 0x01 != 0); // main / numeric Enter
        if !down {
            self.pressed[index] = false;
            self.held_when_armed = false;
            return false;
        }
        let repeated = self.pressed[index] || self.held_when_armed;
        self.pressed[index] = true;
        !repeated
    }
}

thread_local! {
    // Only the dedicated hook thread accesses this state. The callback never
    // waits for the runtime worker or takes a cross-thread mutex.
    static ENTER_CONTEXT: RefCell<Option<EnterContext>> = const { RefCell::new(None) };
}

unsafe extern "system" fn enter_hook(code: i32, message: usize, data: isize) -> isize {
    if code == 0 && data != 0 && matches!(message, 0x0100 | 0x0101 | 0x0104 | 0x0105) {
        let key = &*(data as *const LowLevelKey);
        if key.vk == 0x0d {
            ENTER_CONTEXT.with(|slot| {
                // A nested hook notification must never panic across the FFI.
                let Ok(mut context) = slot.try_borrow_mut() else { return; };
                let Some(context) = context.as_mut() else { return; };
                if !context.transition(key, matches!(message, 0x0100 | 0x0104)) { return; }
                // GetAsyncKeyState is only consulted for previously delivered
                // modifier keys, never for the current Enter transition.
                if key.flags & 0x20 != 0
                    || [0x10, 0x11, 0x12, 0x5b, 0x5c].iter().any(|key| GetAsyncKeyState(*key) < 0) {
                    return;
                }
                let window = GetForegroundWindow();
                let mut pid = 0;
                if window.is_null() || GetWindowThreadProcessId(window, &mut pid) == 0
                    || pid == 0 || pid == GetCurrentProcessId() { return; }
                // Only Enter metadata is retained. A full queue drops this
                // notification rather than ever delaying delivery to Chrome.
                let _ = context.sender.try_send(EnterPress { token: context.token,
                    window_id: window as usize as u64, pid, observed_at_ns: now_ns() });
            });
        }
    }
    // This listener never consumes a key, including keys it does not inspect.
    CallNextHookEx(ptr::null_mut(), code, message, data)
}

struct EnterWatch {
    token: Option<u64>,
    receiver: Option<mpsc::Receiver<EnterPress>>,
    thread: Option<JoinHandle<()>>,
    stop: Option<Arc<AtomicBool>>,
    failure: Option<Arc<AtomicU32>>,
    finished: Option<mpsc::Receiver<()>>,
    thread_id: u32,
}

impl EnterWatch {
    fn new() -> Self {
        Self { token: None, receiver: None, thread: None, stop: None, failure: None,
            finished: None, thread_id: 0 }
    }
    fn stop(&mut self) -> Result<()> {
        self.token = None;
        self.receiver = None;
        if let Some(stop) = self.stop.take() { stop.store(true, Ordering::Release); }
        if self.thread_id != 0 {
            // Wake the message loop. Its 50 ms timeout also observes stop if
            // posting fails because the queue has already disappeared.
            unsafe { PostThreadMessageW(self.thread_id, 0, 0, 0); }
        }
        let stopped = self.finished.take().map_or(true, |finished| {
            finished.recv_timeout(Duration::from_millis(500)).is_ok()
        });
        let mut panicked = false;
        if let Some(thread) = self.thread.take() {
            // join() has no timeout. Only join an already finished thread;
            // a stalled native hook must not trap the runtime's Stop command.
            if thread.is_finished() { panicked = thread.join().is_err(); }
        }
        self.thread_id = 0;
        let failure = self.failure.as_ref().map_or(0, |value| value.load(Ordering::Acquire));
        self.failure = None;
        if !stopped || panicked || failure != 0 {
            return Err(PlatformError::new("enter_watch_stop_failed",
                format!("Enter 감지 종료 확인에 실패했습니다. 입력 신호는 차단했습니다. (Windows 오류 {failure})")));
        }
        Ok(())
    }
    fn set(&mut self, token: Option<u64>) -> Result<()> {
        if self.token == token {
            return if token.is_some() { self.health() } else { Ok(()) };
        }
        self.stop()?;
        let Some(token) = token else { return Ok(()); };
        if !permissions().input {
            return Err(PlatformError::new("enter_watch_permission", "현재 Windows 데스크톱에서 Enter를 감지할 수 없습니다."));
        }
        let (sender, receiver) = mpsc::sync_channel(ENTER_QUEUE_CAPACITY);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let (finished_sender, finished_receiver) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let failure = Arc::new(AtomicU32::new(0));
        let thread_stop = stop.clone();
        let thread_failure = failure.clone();
        let worker = thread::Builder::new().name("macro-enter-watch".into()).spawn(move || unsafe {
            let mut message = WinMessage::default();
            // Ensure a queue exists before publishing this thread's ID.
            PeekMessageW(&mut message, ptr::null_mut(), 0, 0, 0);
            let id = GetCurrentThreadId();
            ENTER_CONTEXT.with(|slot| {
                *slot.borrow_mut() = Some(EnterContext { token, sender,
                    pressed: [false; 2], held_when_armed: GetAsyncKeyState(0x0d) < 0 });
            });
            let hook = SetWindowsHookExW(13, enter_hook, GetModuleHandleW(ptr::null()), 0); // WH_KEYBOARD_LL
            if hook.is_null() {
                let failure = error("enter_watch_install_failed", "브라우저 Enter 감지를 설치하지 못했습니다.");
                let _ = ready_sender.send(Err(failure));
                ENTER_CONTEXT.with(|slot| *slot.borrow_mut() = None);
                let _ = finished_sender.try_send(());
                return;
            }
            let _ = ready_sender.send(Ok(id));
            'pump: while !thread_stop.load(Ordering::Acquire) {
                while PeekMessageW(&mut message, ptr::null_mut(), 0, 0, 1) != 0 {
                    if message.message == 0x0012 { break 'pump; } // WM_QUIT
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                    if thread_stop.load(Ordering::Acquire) { break 'pump; }
                }
                if MsgWaitForMultipleObjectsEx(0, ptr::null(), 50, 0x04ff, 0x0004) == u32::MAX {
                    thread_failure.store(GetLastError().max(1), Ordering::Release);
                    break;
                }
            }
            if UnhookWindowsHookEx(hook) == 0 {
                thread_failure.store(GetLastError().max(1), Ordering::Release);
            }
            ENTER_CONTEXT.with(|slot| *slot.borrow_mut() = None);
            let _ = finished_sender.try_send(());
        }).map_err(|e| PlatformError::new("enter_watch_thread_failed", format!("Enter 감지 스레드를 시작하지 못했습니다: {e}")))?;
        match ready_receiver.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(id)) => {
                self.token = Some(token);
                self.receiver = Some(receiver);
                self.thread = Some(worker);
                self.stop = Some(stop);
                self.failure = Some(failure);
                self.finished = Some(finished_receiver);
                self.thread_id = id;
                Ok(())
            }
            Ok(Err(error)) => {
                if worker.is_finished() { let _ = worker.join(); }
                Err(error)
            }
            Err(_) => {
                // Do not wait forever if the OS stalled during installation.
                // The captured stop flag makes a late installation unhook
                // immediately; dropping receiver makes late sends inert.
                stop.store(true, Ordering::Release);
                drop(worker);
                Err(PlatformError::new("enter_watch_timeout", "Enter 감지 설치 응답이 없습니다. 앱을 다시 시작하세요."))
            }
        }
    }
    fn health(&self) -> Result<()> {
        // This detects our thread/pump failures, not the OS's documented
        // silent removal of a timed-out WH_KEYBOARD_LL hook.
        let failure = self.failure.as_ref().map_or(0, |value| value.load(Ordering::Acquire));
        if failure != 0 || self.thread.as_ref().map_or(true, |thread| thread.is_finished()) {
            return Err(PlatformError::new("enter_watch_stopped",
                format!("Enter 감지가 중단되었습니다. 다시 시작하세요. (Windows 오류 {failure})")));
        }
        Ok(())
    }
    fn take(&mut self) -> Result<Vec<EnterPress>> {
        let Some(token) = self.token else { return Ok(Vec::new()); };
        self.health()?;
        let Some(receiver) = self.receiver.as_ref() else {
            return Err(PlatformError::new("enter_watch_stopped", "Enter 감지 수신기가 없습니다."));
        };
        let entries: Vec<_> = receiver.try_iter().take(ENTER_QUEUE_CAPACITY).collect();
        let now = now_ns();
        Ok(entries.into_iter().filter(|press| fresh_enter_press(press, token, now)).collect())
    }
}

impl Drop for EnterWatch {
    fn drop(&mut self) { let _ = self.stop(); }
}

// A separate hook, token and bounded metadata queue keep this listener
// independent of Enter submission. No character, virtual key or scan code is
// retained in a notification or diagnostic. Windows does not expose a portable
// Fn modifier: firmware Fn combinations indistinguishable from letters at this
// hook cannot be separately recognized.
fn captcha_candidate_key(key: u32) -> bool {
    matches!(key, 0x30..=0x39 | 0x41..=0x5a | 0x60..=0x69)
}

const CAPTCHA_SHIFT: u8 = 1;
const CAPTCHA_CONTROL: u8 = 2;
const CAPTCHA_ALT: u8 = 4;
const CAPTCHA_WINDOWS: u8 = 8;

struct CaptchaInputContext {
    token: u64,
    sender: mpsc::SyncSender<CaptchaInput>,
    pressed: [bool; 256],
}

impl CaptchaInputContext {
    fn transition(&mut self, key: &LowLevelKey, down: bool, modifiers: u8) -> bool {
        // Injected releases must not clear physical held/repeat protection.
        if !captcha_candidate_key(key.vk) || key.flags & (0x10 | 0x02) != 0 { return false; }
        let held = &mut self.pressed[key.vk as usize];
        if !down { *held = false; return false; }
        let repeated = *held;
        *held = true;
        !repeated && key.flags & 0x20 == 0
            && modifiers & (CAPTCHA_CONTROL | CAPTCHA_ALT | CAPTCHA_WINDOWS) == 0
    }
}

thread_local! {
    static CAPTCHA_INPUT_CONTEXT: RefCell<Option<CaptchaInputContext>> = const { RefCell::new(None) };
}

unsafe extern "system" fn captcha_input_hook(code: i32, message: usize, data: isize) -> isize {
    if code == 0 && data != 0 && matches!(message, 0x0100 | 0x0101 | 0x0104 | 0x0105) {
        let key = &*(data as *const LowLevelKey);
        if captcha_candidate_key(key.vk) {
            CAPTCHA_INPUT_CONTEXT.with(|slot| {
                let Ok(mut context) = slot.try_borrow_mut() else { return; };
                let Some(context) = context.as_mut() else { return; };
                let modifiers = if GetAsyncKeyState(0x10) < 0 { CAPTCHA_SHIFT } else { 0 }
                    | if GetAsyncKeyState(0x11) < 0 { CAPTCHA_CONTROL } else { 0 }
                    | if GetAsyncKeyState(0x12) < 0 { CAPTCHA_ALT } else { 0 }
                    | if GetAsyncKeyState(0x5b) < 0 || GetAsyncKeyState(0x5c) < 0 { CAPTCHA_WINDOWS } else { 0 };
                if !context.transition(key, matches!(message, 0x0100 | 0x0104), modifiers) { return; }
                let window = GetForegroundWindow();
                let mut pid = 0;
                if window.is_null() || GetWindowThreadProcessId(window, &mut pid) == 0
                    || pid == 0 || pid == GetCurrentProcessId() { return; }
                let _ = context.sender.try_send(CaptchaInput { token: context.token,
                    window_id: window as usize as u64, pid, observed_at_ns: now_ns() });
            });
        }
    }
    // Observe only; never consume or synthesize a browser key.
    CallNextHookEx(ptr::null_mut(), code, message, data)
}

struct CaptchaInputWatch {
    token: Option<u64>,
    receiver: Option<mpsc::Receiver<CaptchaInput>>,
    thread: Option<JoinHandle<()>>,
    stop: Option<Arc<AtomicBool>>,
    failure: Option<Arc<AtomicU32>>,
    finished: Option<mpsc::Receiver<()>>,
    thread_id: u32,
}

impl CaptchaInputWatch {
    fn new() -> Self {
        Self { token: None, receiver: None, thread: None, stop: None, failure: None,
            finished: None, thread_id: 0 }
    }
    fn stop(&mut self) -> Result<()> {
        self.token = None;
        self.receiver = None;
        if let Some(stop) = self.stop.take() { stop.store(true, Ordering::Release); }
        if self.thread_id != 0 {
            // Wake the message loop. Its 50 ms timeout also observes stop if
            // posting fails because the queue has already disappeared.
            unsafe { PostThreadMessageW(self.thread_id, 0, 0, 0); }
        }
        let stopped = self.finished.take().map_or(true, |finished| {
            finished.recv_timeout(Duration::from_millis(500)).is_ok()
        });
        let mut panicked = false;
        if let Some(thread) = self.thread.take() {
            // join() has no timeout. Only join an already finished thread;
            // a stalled native hook must not trap the runtime's Stop command.
            if thread.is_finished() { panicked = thread.join().is_err(); }
        }
        self.thread_id = 0;
        let failure = self.failure.as_ref().map_or(0, |value| value.load(Ordering::Acquire));
        self.failure = None;
        if !stopped || panicked || failure != 0 {
            return Err(PlatformError::new("captcha_input_watch_stop_failed",
                format!("첫 문자 감지 종료 확인에 실패했습니다. 입력 신호는 차단했습니다. (Windows 오류 {failure})")));
        }
        Ok(())
    }
    fn set(&mut self, token: Option<u64>) -> Result<()> {
        if self.token == token {
            return if token.is_some() { self.health() } else { Ok(()) };
        }
        self.stop()?;
        let Some(token) = token else { return Ok(()); };
        if !permissions().input {
            return Err(PlatformError::new("captcha_input_watch_permission", "현재 Windows 데스크톱에서 첫 문자를 감지할 수 없습니다."));
        }
        let (sender, receiver) = mpsc::sync_channel(ENTER_QUEUE_CAPACITY);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let (finished_sender, finished_receiver) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let failure = Arc::new(AtomicU32::new(0));
        let thread_stop = stop.clone();
        let thread_failure = failure.clone();
        let worker = thread::Builder::new().name("macro-captcha-input-watch".into()).spawn(move || unsafe {
            let mut message = WinMessage::default();
            // Ensure a queue exists before publishing this thread's ID.
            PeekMessageW(&mut message, ptr::null_mut(), 0, 0, 0);
            let id = GetCurrentThreadId();
            CAPTCHA_INPUT_CONTEXT.with(|slot| {
                *slot.borrow_mut() = Some(CaptchaInputContext { token, sender,
                    pressed: std::array::from_fn(|key| captcha_candidate_key(key as u32)
                        && GetAsyncKeyState(key as i32) < 0) });
            });
            let hook = SetWindowsHookExW(13, captcha_input_hook, GetModuleHandleW(ptr::null()), 0); // WH_KEYBOARD_LL
            if hook.is_null() {
                let failure = error("captcha_input_watch_install_failed", "브라우저 첫 문자 감지를 설치하지 못했습니다.");
                let _ = ready_sender.send(Err(failure));
                CAPTCHA_INPUT_CONTEXT.with(|slot| *slot.borrow_mut() = None);
                let _ = finished_sender.try_send(());
                return;
            }
            let _ = ready_sender.send(Ok(id));
            'pump: while !thread_stop.load(Ordering::Acquire) {
                while PeekMessageW(&mut message, ptr::null_mut(), 0, 0, 1) != 0 {
                    if message.message == 0x0012 { break 'pump; } // WM_QUIT
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                    if thread_stop.load(Ordering::Acquire) { break 'pump; }
                }
                if MsgWaitForMultipleObjectsEx(0, ptr::null(), 50, 0x04ff, 0x0004) == u32::MAX {
                    thread_failure.store(GetLastError().max(1), Ordering::Release);
                    break;
                }
            }
            if UnhookWindowsHookEx(hook) == 0 {
                thread_failure.store(GetLastError().max(1), Ordering::Release);
            }
            CAPTCHA_INPUT_CONTEXT.with(|slot| *slot.borrow_mut() = None);
            let _ = finished_sender.try_send(());
        }).map_err(|e| PlatformError::new("captcha_input_watch_thread_failed", format!("첫 문자 감지 스레드를 시작하지 못했습니다: {e}")))?;
        match ready_receiver.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(id)) => {
                self.token = Some(token);
                self.receiver = Some(receiver);
                self.thread = Some(worker);
                self.stop = Some(stop);
                self.failure = Some(failure);
                self.finished = Some(finished_receiver);
                self.thread_id = id;
                Ok(())
            }
            Ok(Err(error)) => {
                if worker.is_finished() { let _ = worker.join(); }
                Err(error)
            }
            Err(_) => {
                // Do not wait forever if the OS stalled during installation.
                // The captured stop flag makes a late installation unhook
                // immediately; dropping receiver makes late sends inert.
                stop.store(true, Ordering::Release);
                drop(worker);
                Err(PlatformError::new("captcha_input_watch_timeout", "첫 문자 감지 설치 응답이 없습니다. 앱을 다시 시작하세요."))
            }
        }
    }
    fn health(&self) -> Result<()> {
        // This detects our thread/pump failures, not the OS's documented
        // silent removal of a timed-out WH_KEYBOARD_LL hook.
        let failure = self.failure.as_ref().map_or(0, |value| value.load(Ordering::Acquire));
        if failure != 0 || self.thread.as_ref().map_or(true, |thread| thread.is_finished()) {
            return Err(PlatformError::new("captcha_input_watch_stopped",
                format!("첫 문자 감지가 중단되었습니다. 다시 시작하세요. (Windows 오류 {failure})")));
        }
        Ok(())
    }
    fn take(&mut self) -> Result<Vec<CaptchaInput>> {
        let Some(token) = self.token else { return Ok(Vec::new()); };
        self.health()?;
        let Some(receiver) = self.receiver.as_ref() else {
            return Err(PlatformError::new("captcha_input_watch_stopped", "첫 문자 감지 수신기가 없습니다."));
        };
        let entries: Vec<_> = receiver.try_iter().take(ENTER_QUEUE_CAPACITY).collect();
        let now = now_ns();
        Ok(entries.into_iter().filter(|press| fresh_enter_press(press, token, now)).collect())
    }
}

impl Drop for CaptchaInputWatch {
    fn drop(&mut self) { let _ = self.stop(); }
}

struct CaptureBuffer {
    screen: Hdc, memory: Hdc, bitmap: Handle, previous: Handle, pixels: *mut u8,
    width: i32, height: i32,
}
unsafe impl Send for CaptureBuffer {}
impl CaptureBuffer {
    fn new(width: i32, height: i32) -> Result<Self> {
        if width <= 0 || height <= 0 || i64::from(width) * i64::from(height) * 4 > 512 * 1024 * 1024 {
            return Err(PlatformError::new("capture_size_invalid", "캡처 화면 크기가 지원 범위를 벗어났습니다."));
        }
        unsafe {
            let screen = GetDC(ptr::null_mut());
            if screen.is_null() { return Err(error("capture_permission", "현재 데스크톱 화면을 읽지 못했습니다.")); }
            let memory = CreateCompatibleDC(screen);
            if memory.is_null() { ReleaseDC(ptr::null_mut(), screen); return Err(error("capture_init_failed", "캡처 메모리를 만들지 못했습니다.")); }
            let info = BitmapInfo { header: BitmapInfoHeader { size: size_of::<BitmapInfoHeader>() as u32,
                width, height: -height, planes: 1, bit_count: 32, compression: 0, size_image: 0,
                x_pixels_per_meter: 0, y_pixels_per_meter: 0, colors_used: 0, colors_important: 0 }, colors: [0] };
            let mut pixels = ptr::null_mut();
            let bitmap = CreateDIBSection(screen, &info, 0, &mut pixels, ptr::null_mut(), 0);
            if bitmap.is_null() || pixels.is_null() {
                if !bitmap.is_null() { DeleteObject(bitmap); }
                DeleteDC(memory); ReleaseDC(ptr::null_mut(), screen);
                return Err(error("capture_init_failed", "캡처 픽셀 버퍼를 만들지 못했습니다."));
            }
            let previous = SelectObject(memory, bitmap);
            if previous.is_null() || previous as isize == -1 {
                DeleteObject(bitmap); DeleteDC(memory); ReleaseDC(ptr::null_mut(), screen);
                return Err(error("capture_init_failed", "캡처 픽셀 버퍼를 연결하지 못했습니다."));
            }
            Ok(Self { screen, memory, bitmap, previous, pixels: pixels.cast(), width, height })
        }
    }
}
impl Drop for CaptureBuffer {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.memory, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.memory);
            ReleaseDC(ptr::null_mut(), self.screen);
        }
    }
}
pub struct Driver { buffer: Option<CaptureBuffer>, sequence: u64, enter: EnterWatch, captcha_input: CaptchaInputWatch }
impl Driver {
    pub fn new() -> Result<Self> { Ok(Self { buffer: None, sequence: 0, enter: EnterWatch::new(), captcha_input: CaptchaInputWatch::new() }) }
    pub fn set_enter_watch(&mut self, token: Option<u64>) -> Result<()> { self.enter.set(token) }
    pub fn take_enter_presses(&mut self) -> Result<Vec<EnterPress>> { self.enter.take() }
    pub fn set_captcha_input_watch(&mut self, token: Option<u64>) -> Result<()> { self.captcha_input.set(token) }
    pub fn take_captcha_inputs(&mut self) -> Result<Vec<CaptchaInput>> { self.captcha_input.take() }
    // Native callback diagnostics are currently implemented only on macOS.
    pub fn take_captcha_input_diagnostics(&mut self) -> Result<Vec<EnterDiagnostic>> { Ok(Vec::new()) }
    // Native rejection diagnostics are currently implemented on macOS only.
    pub fn take_enter_diagnostics(&mut self) -> Result<Vec<EnterDiagnostic>> { Ok(Vec::new()) }
    pub fn bind(&mut self, target: &TargetWindow) -> Result<()> {
        let _dpi = DpiGuard::new()?;
        // Process identity is rechecked before each action. A running process
        // cannot elevate in place, so token queries stay out of the hot loop.
        check_integrity(target.pid)?;
        self.buffer = Some(CaptureBuffer::new(target.rect.width as i32, target.rect.height as i32)?);
        self.sequence = 0;
        Ok(())
    }
    pub fn capture(&mut self, target: &TargetWindow) -> Result<OwnedFrame> {
        let _dpi = DpiGuard::new()?;
        let buffer = self.buffer.as_ref().ok_or_else(|| PlatformError::new("capture_not_bound", "‘브라우저 연결’을 먼저 실행하세요."))?;
        unsafe {
            if BitBlt(buffer.memory, 0, 0, buffer.width, buffer.height, buffer.screen,
                target.rect.x as i32, target.rect.y as i32, 0x00CC0020 | 0x40000000) == 0 {
                return Err(error("capture_failed", "화면 캡처에 실패했습니다. 잠금 화면과 브라우저 상태를 확인하세요."));
            }
            if GdiFlush() == 0 { return Err(error("capture_flush_failed", "화면 캡처 완료를 확인하지 못했습니다.")); }
            let width = buffer.width as usize;
            let height = buffer.height as usize;
            let mut rgba = std::slice::from_raw_parts(buffer.pixels, width * height * 4).to_vec();
            for pixel in rgba.chunks_exact_mut(4) { pixel.swap(0, 2); pixel[3] = 255; }
            self.sequence = self.sequence.wrapping_add(1);
            let timestamp = now_ns();
            Ok(OwnedFrame { width, height, stride: width * 4, rgba,
                origin: Point { x: target.rect.x, y: target.rect.y }, scale_x: 1.0, scale_y: 1.0,
                sequence: self.sequence, captured_at_ns: timestamp, last_observed_ns: timestamp })
        }
    }
    pub fn capture_if_changed(&mut self, target: &TargetWindow, _after_sequence: u64) -> Result<Option<OwnedFrame>> {
        self.capture(target).map(Some)
    }
    pub fn capture_observation(&mut self, target: &TargetWindow, _after_sequence: u64) -> Result<FrameObservation> {
        self.capture(target).map(FrameObservation::Changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_send_never_queries_or_posts_windows_input() {
        let input = Input { kind: 1, data: InputData { keyboard: KeyboardInput {
            vk: 0x1b, scan: 0, flags: 0, time: 0, extra_info: 0,
        }}};
        assert_eq!(send(&[input], &|| true).unwrap_err().code, "input_cancelled");
    }
    #[test]
    fn ffi_input_layout_matches_windows_abi() {
        assert_eq!(size_of::<Input>(), if size_of::<usize>() == 8 { 40 } else { 28 });
        assert_eq!(size_of::<BitmapInfoHeader>(), 40);
        assert_eq!(size_of::<WinMessage>(), if size_of::<usize>() == 8 { 48 } else { 32 });
        assert_eq!(size_of::<LowLevelKey>(), if size_of::<usize>() == 8 { 24 } else { 20 });
    }
    #[test]
    fn enter_hook_filters_repeats_and_injection_without_consuming_key_releases() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let mut context = EnterContext { token: 7, sender, pressed: [false; 2], held_when_armed: false };
        let enter = LowLevelKey { vk: 0x0d, scan: 28, flags: 0, time: 0, extra_info: 0 };
        assert!(context.transition(&enter, true));
        assert!(!context.transition(&enter, true));
        let injected_up = LowLevelKey { flags: 0x10, ..enter };
        assert!(!context.transition(&injected_up, false));
        assert!(!context.transition(&enter, true));
        assert!(!context.transition(&enter, false));
        assert!(context.transition(&enter, true));
        let numeric = LowLevelKey { flags: 0x01, ..enter };
        assert!(context.transition(&numeric, true));
        assert!(!context.transition(&numeric, true));
    }
    #[test]
    fn enter_held_at_arm_must_be_released_before_it_can_trigger() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let mut context = EnterContext { token: 7, sender, pressed: [false; 2], held_when_armed: true };
        let enter = LowLevelKey { vk: 0x0d, scan: 28, flags: 0, time: 0, extra_info: 0 };
        assert!(!context.transition(&enter, true));
        assert!(!context.transition(&enter, false));
        assert!(context.transition(&enter, true));
    }

    fn captcha_context() -> CaptchaInputContext {
        let (sender, _) = mpsc::sync_channel(ENTER_QUEUE_CAPACITY);
        CaptchaInputContext { token: 9, sender, pressed: [false; 256] }
    }

    fn physical_key(vk: u32) -> LowLevelKey {
        LowLevelKey { vk, scan: 0, flags: 0, time: 0, extra_info: 0 }
    }

    #[test]
    fn captcha_accepts_letters_digits_and_keypad_with_optional_shift_only() {
        for vk in (0x30..=0x39).chain(0x41..=0x5a).chain(0x60..=0x69) {
            for modifiers in [0, CAPTCHA_SHIFT] {
                assert!(captcha_context().transition(&physical_key(vk), true, modifiers));
            }
        }
        for vk in [0x08, 0x0d, 0x10, 0x20, 0x25, 0x26, 0x28, 0x6e, 0x70, 0x78, 0xff] {
            assert!(!captcha_context().transition(&physical_key(vk), true, 0));
        }
        for modifiers in [CAPTCHA_CONTROL, CAPTCHA_ALT, CAPTCHA_WINDOWS,
            CAPTCHA_SHIFT | CAPTCHA_CONTROL, CAPTCHA_SHIFT | CAPTCHA_ALT, CAPTCHA_SHIFT | CAPTCHA_WINDOWS] {
            let mut context = captcha_context();
            let key = physical_key(0x41);
            assert!(!context.transition(&key, true, modifiers));
            assert!(!context.transition(&key, true, 0), "Releasing only a modifier is not a new physical press");
            assert!(!context.transition(&key, false, 0));
            assert!(context.transition(&key, true, 0));
        }
        assert!(!captcha_context().transition(&LowLevelKey { flags: 0x20, ..physical_key(0x41) }, true, 0));
    }

    #[test]
    fn captcha_repeats_injected_releases_and_held_keys_do_not_trigger() {
        let mut context = captcha_context();
        let key = physical_key(0x41);
        context.pressed[0x41] = true; // GetAsyncKeyState found it held when armed.
        assert!(!context.transition(&key, true, 0));
        for flags in [0x10, 0x02, 0x12] {
            assert!(!context.transition(&LowLevelKey { flags, ..key }, false, 0));
            assert!(!context.transition(&key, true, 0));
        }
        assert!(!context.transition(&key, false, CAPTCHA_CONTROL));
        assert!(context.transition(&key, true, CAPTCHA_SHIFT));
        assert!(!context.transition(&key, true, 0));
        assert!(context.transition(&physical_key(0x42), true, 0), "Another key has its own physical state");
        let mut context = captcha_context();
        assert!(!context.transition(&LowLevelKey { flags: 0x10, ..key }, true, 0));
        assert!(context.transition(&key, true, 0), "Injected down must not suppress a later physical down");
    }

    #[test]
    fn disabling_one_watch_preserves_the_other_watch_token() {
        let mut driver = Driver::new().unwrap();
        driver.enter.token = Some(7);
        driver.set_captcha_input_watch(None).unwrap();
        assert_eq!(driver.enter.token, Some(7));
        driver.enter.token = None;
        driver.captcha_input.token = Some(9);
        driver.set_enter_watch(None).unwrap();
        assert_eq!(driver.captcha_input.token, Some(9));
    }

    #[test]
    fn captcha_queue_is_bounded_and_exports_only_fresh_metadata() {
        let (sender, receiver) = mpsc::sync_channel(ENTER_QUEUE_CAPACITY);
        let input = CaptchaInput { token: 9, window_id: 44, pid: 123, observed_at_ns: 1_000_000_000 };
        for _ in 0..ENTER_QUEUE_CAPACITY { sender.try_send(input).unwrap(); }
        assert!(matches!(sender.try_send(input), Err(mpsc::TrySendError::Full(_))));
        let entries: Vec<_> = receiver.try_iter().collect();
        assert_eq!(entries.len(), ENTER_QUEUE_CAPACITY);
        assert!(fresh_enter_press(&input, 9, 1_500_000_000));
        assert!(!fresh_enter_press(&input, 9, 1_500_000_001));
        assert!(!fresh_enter_press(&input, 8, 1_000_000_000));
        assert!(!fresh_enter_press(&input, 9, 999_999_999));
        let encoded = serde_json::to_value(input).unwrap();
        let fields = encoded.as_object().unwrap();
        assert_eq!(fields.len(), 4);
        for field in ["token", "window_id", "pid", "observed_at_ns"] { assert!(fields.contains_key(field)); }
    }
}
