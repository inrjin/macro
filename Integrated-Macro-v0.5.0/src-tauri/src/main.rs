#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clock;
mod keyboard_check;
mod runtime;
mod storage;
mod visual_recovery;

use macro_core::RunMode;
use runtime::{Command, RuntimeHandle};
use serde_json::Value;
use tauri::{Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};

const SHORTCUT_KEYS: [Code; 9] = [
    Code::F1, Code::F2, Code::F3, Code::F4, Code::F5, Code::F6, Code::F7, Code::F8, Code::F9,
];

async fn dispatch(handle: RuntimeHandle, command: Command) -> Result<Value, String> {
    // Capture the control epoch before scheduling a possibly delayed waiter.
    // Otherwise F4 could run first and an old Start would receive a fresh epoch.
    let pending = handle.enqueue(command)?;
    tauri::async_runtime::spawn_blocking(move || pending.wait())
        .await.map_err(|e| format!("실행 엔진 요청 실패: {e}"))?
}

#[tauri::command]
async fn get_bootstrap(state: State<'_, RuntimeHandle>) -> Result<Value, String> {
    dispatch(state.inner().clone(), Command::Bootstrap).await
}

#[tauri::command]
async fn save_profile(profile: Value, expected_profile_revision: u64, state: State<'_, RuntimeHandle>) -> Result<Value, String> {
    dispatch(state.inner().clone(), Command::Save { profile, expected_profile_revision }).await
}

#[tauri::command]
async fn list_windows(state: State<'_, RuntimeHandle>) -> Result<Value, String> {
    dispatch(state.inner().clone(), Command::Windows).await
}

/// Minimize the settings window before checking the target's actual foreground state.
/// We never click through the settings window or exempt it from the occlusion guard.
async fn return_to_booking(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_focused().map_err(|e| e.to_string())? {
            window.minimize().map_err(|e| e.to_string())?;
            tauri::async_runtime::spawn_blocking(|| std::thread::sleep(std::time::Duration::from_millis(200)))
                .await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn reveal_error(app: &tauri::AppHandle, result: &Result<Value, String>) {
    if result.is_err() {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

fn handoff_is_current(result: &Result<Value, String>, epoch_is_current: impl FnOnce(u64) -> bool) -> bool {
    result.as_ref().is_ok_and(|value| value.get("cancelled") != Some(&Value::Bool(true))
        && value.get("controlEpoch").and_then(Value::as_u64).is_none_or(epoch_is_current))
}

fn execution_reply_if_current(result: Result<Value, String>, epoch: u64, current: bool) -> Result<Value, String> {
    if current { result } else { Ok(serde_json::json!({"cancelled":true,"controlEpoch":epoch})) }
}

fn minimize_after_handoff(app: &tauri::AppHandle, result: &Result<Value, String>, runtime: &RuntimeHandle) {
    if handoff_is_current(result, |epoch| runtime.control_epoch_is_current(epoch)) {
        if let Some(window) = app.get_webview_window("main") {
            // Activation was verified by the worker. Minimize afterward so
            // macOS can yield focus while our app is still the active source.
            let _ = window.minimize();
        }
    }
}

#[tauri::command]
async fn bind_target(id: String, app: tauri::AppHandle, state: State<'_, RuntimeHandle>) -> Result<Value, String> {
    let id = id.parse::<u64>().map_err(|_| "브라우저 창을 선택하세요.".to_string())?;
    return_to_booking(&app).await?;
    let result = dispatch(state.inner().clone(), Command::Bind(Some(id))).await;
    reveal_error(&app, &result);
    result
}

#[tauri::command]
async fn arm_capture(field: String, profile: Option<Value>, expected_profile_revision: u64, app: tauri::AppHandle, state: State<'_, RuntimeHandle>) -> Result<Value, String> {
    let result = dispatch(state.inner().clone(), Command::ArmCapture { field, profile, expected_profile_revision }).await;
    if result.is_ok() { return_to_booking(&app).await?; }
    reveal_error(&app, &result);
    result
}

#[tauri::command]
async fn start_keyboard_check(app: tauri::AppHandle, state: State<'_, RuntimeHandle>) -> Result<Value, String> {
    let result = dispatch(state.inner().clone(), Command::StartKeyboardCheck).await;
    if result.is_ok() {
        if let Err(error) = return_to_booking(&app).await {
            let _ = dispatch(state.inner().clone(), Command::StopKeyboardCheck).await;
            return Err(format!("키보드 수신 검사 중단 · 예매창으로 돌아가지 못했습니다. {error}"));
        }
    }
    reveal_error(&app, &result);
    result
}

#[tauri::command]
async fn stop_keyboard_check(state: State<'_, RuntimeHandle>) -> Result<Value, String> {
    dispatch(state.inner().clone(), Command::StopKeyboardCheck).await
}

#[tauri::command]
async fn start_run(mode: RunMode, profile: Value, expected_profile_revision: u64, app: tauri::AppHandle, state: State<'_, RuntimeHandle>) -> Result<Value, String> {
    let handle = state.inner().clone();
    let pending = handle.enqueue(Command::StartFromUi { mode, profile, expected_profile_revision })?;
    let epoch = pending.control_epoch();
    let result = tauri::async_runtime::spawn_blocking(move || pending.wait())
        .await.map_err(|error| format!("실행 엔진 요청 실패: {error}"))?;
    // Even an already-produced error may arrive after a newer F-key request.
    // Do not let its delayed UI handoff cover the newer booking window.
    let result = execution_reply_if_current(result, epoch, handle.control_epoch_is_current(epoch));
    minimize_after_handoff(&app, &result, state.inner());
    reveal_error(&app, &result);
    result
}

#[tauri::command]
async fn start_shortcut(
    request_id: u64, profile: Option<Value>, error: Option<String>, expected_profile_revision: u64,
    state: State<'_, RuntimeHandle>,
) -> Result<Value, String> {
    // The global shortcut belongs to the foreground booking window. Reporting
    // errors must not activate the settings window over that target.
    dispatch(state.inner().clone(), Command::ApplyShortcut { request_id, profile, error, expected_profile_revision }).await
}

#[tauri::command]
async fn pause_run(state: State<'_, RuntimeHandle>) -> Result<Value, String> {
    dispatch(state.inner().clone(), Command::Pause).await
}

#[tauri::command]
async fn resume_run(app: tauri::AppHandle, state: State<'_, RuntimeHandle>) -> Result<Value, String> {
    let result = dispatch(state.inner().clone(), Command::ResumeFromUi).await;
    minimize_after_handoff(&app, &result, state.inner());
    reveal_error(&app, &result);
    result
}

#[tauri::command]
async fn stop_run(state: State<'_, RuntimeHandle>) -> Result<Value, String> {
    dispatch(state.inner().clone(), Command::Stop).await
}

#[tauri::command]
async fn synchronize_clock(url: String, state: State<'_, RuntimeHandle>) -> Result<Value, String> {
    dispatch(state.inner().clone(), Command::Clock(url)).await
}

#[tauri::command]
async fn set_overlay(enabled: bool, state: State<'_, RuntimeHandle>) -> Result<Value, String> {
    dispatch(state.inner().clone(), Command::Overlay(enabled)).await
}

#[tauri::command]
async fn confirm_observation(
    kind: String, run_id: u64, revision: u64, selected_ids: Option<Vec<String>>,
    app: tauri::AppHandle, state: State<'_, RuntimeHandle>,
) -> Result<Value, String> {
    let result = dispatch(state.inner().clone(), Command::Confirm { kind, run_id, revision, selected_ids }).await;
    minimize_after_handoff(&app, &result, state.inner());
    reveal_error(&app, &result);
    result
}

fn shortcut_command(shortcut: &Shortcut) -> Option<Command> {
    [
        (Code::F1, Command::PrepareShortcut(RunMode::Normal)),
        (Code::F2, Command::PrepareShortcut(RunMode::Cancellation)),
        (Code::F3, Command::TogglePause),
        (Code::F4, Command::Stop),
        (Code::F5, Command::PrepareShortcut(RunMode::Seats)),
        (Code::F6, Command::Continue),
        (Code::F7, Command::Bind(None)),
        (Code::F8, Command::CaptureArmed),
        (Code::F9, Command::ContinueAfterCaptcha),
    ].into_iter().find_map(|(code, command)| (shortcut == &Shortcut::new(None, code)).then_some(command))
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, shortcut, event| {
                if event.state() != ShortcutState::Pressed { return; }
                if let (Some(state), Some(command)) = (app.try_state::<RuntimeHandle>(), shortcut_command(shortcut)) {
                    state.send(command);
                }
            }).build())
        .invoke_handler(tauri::generate_handler![
            get_bootstrap, save_profile, list_windows, bind_target,
            arm_capture, start_keyboard_check, stop_keyboard_check,
            start_run, start_shortcut, pause_run, resume_run, stop_run,
            synchronize_clock, set_overlay, confirm_observation,
        ])
        .setup(|app| {
            let path = app.path().app_data_dir()?.join("settings.json");
            let handle = RuntimeHandle::launch(app.handle().clone(), path);
            app.manage(handle.clone());
            let mut errors = Vec::new();
            for code in SHORTCUT_KEYS {
                if let Err(error) = app.global_shortcut().register(Shortcut::new(None, code)) {
                    errors.push(format!("{code:?} 등록 실패: {error}"));
                }
            }
            handle.send(Command::ShortcutStatus(errors));
            if let Some(status) = app.get_webview_window("status") {
                let overlay_result = (|| -> Result<(), String> {
                    status.set_ignore_cursor_events(true).map_err(|e| e.to_string())?;
                    // Establish the native window number without taking keyboard focus.
                    status.show().map_err(|e| e.to_string())?;
                    #[cfg(target_os = "macos")]
                    let native_handle = status.ns_window().map_err(|e| e.to_string())? as usize;
                    #[cfg(target_os = "windows")]
                    let native_handle = status.hwnd().map_err(|e| e.to_string())?.0 as usize;
                    #[cfg(any(target_os = "macos", target_os = "windows"))]
                    {
                        // setup runs on the UI thread; this is our own live status window.
                        let id = unsafe { macro_platform::owned_overlay_window_id(native_handle) }
                            .map_err(|e| e.to_string())?;
                        handle.send(Command::OverlayIds(vec![id]));
                    }
                    Ok(())
                })();
                let _ = status.hide();
                if let Err(error) = overlay_result {
                    handle.send(Command::OverlayError(format!("상단 상태 표시창 준비 실패: {error}")));
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("통합 매크로를 시작하지 못했습니다.");

    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            if let Some(state) = app.try_state::<RuntimeHandle>() { state.send(Command::Shutdown); }
        }
    });
}

#[cfg(test)]
mod shortcut_tests {
    #[test]
    fn stopped_or_cancelled_start_reply_cannot_minimize_settings_later() {
        let success = Ok(serde_json::json!({"controlEpoch":3}));
        assert!(super::handoff_is_current(&success, |epoch| epoch == 3));
        assert!(!super::handoff_is_current(&success, |epoch| epoch == 4));
        assert!(!super::handoff_is_current(&Ok(serde_json::json!({"cancelled":true})), |_| true));
        assert!(!super::handoff_is_current(&Err("timeout".into()), |_| true));
    }

    use super::*;

    #[test]
    fn f1_requests_current_normal_settings_and_does_not_dispatch_seats() {
        assert!(matches!(shortcut_command(&Shortcut::new(None, Code::F1)),
            Some(Command::PrepareShortcut(RunMode::Normal))));
        assert!(matches!(shortcut_command(&Shortcut::new(None, Code::F2)),
            Some(Command::PrepareShortcut(RunMode::Cancellation))));
        assert!(matches!(shortcut_command(&Shortcut::new(None, Code::F3)), Some(Command::TogglePause)));
        assert!(matches!(shortcut_command(&Shortcut::new(None, Code::F4)), Some(Command::Stop)));
        assert!(matches!(shortcut_command(&Shortcut::new(None, Code::F6)), Some(Command::Continue)));
        assert!(matches!(shortcut_command(&Shortcut::new(None, Code::F7)), Some(Command::Bind(None))));
        assert!(matches!(shortcut_command(&Shortcut::new(None, Code::F8)), Some(Command::CaptureArmed)));
    }

    #[test]
    fn f5_requests_current_seat_settings_without_dispatching_browser_refresh() {
        assert!(matches!(shortcut_command(&Shortcut::new(None, Code::F5)),
            Some(Command::PrepareShortcut(RunMode::Seats))));
        assert!(shortcut_command(&Shortcut::new(Some(tauri_plugin_global_shortcut::Modifiers::SHIFT), Code::F5)).is_none());
    }

    #[test]
    fn f2_requests_current_cancellation_options_before_starting() {
        assert!(matches!(shortcut_command(&Shortcut::new(None, Code::F2)),
            Some(Command::PrepareShortcut(RunMode::Cancellation))));
    }

    #[test]
    fn f9_requests_manual_captcha_continuation_without_starting_a_run() {
        assert!(matches!(shortcut_command(&Shortcut::new(None, Code::F9)),
            Some(Command::ContinueAfterCaptcha)));
        for modifier in [tauri_plugin_global_shortcut::Modifiers::SHIFT,
            tauri_plugin_global_shortcut::Modifiers::CONTROL,
            tauri_plugin_global_shortcut::Modifiers::ALT,
            tauri_plugin_global_shortcut::Modifiers::SUPER] {
            assert!(shortcut_command(&Shortcut::new(Some(modifier), Code::F9)).is_none());
        }
        assert!(shortcut_command(&Shortcut::new(None, Code::F10)).is_none());
    }

    #[test]
    fn registered_keys_include_f9_and_all_have_commands() {
        assert_eq!(SHORTCUT_KEYS.iter().filter(|key| **key == Code::F9).count(), 1);
        for code in SHORTCUT_KEYS {
            assert!(shortcut_command(&Shortcut::new(None, code)).is_some(), "{code:?}");
        }
    }
}

#[cfg(test)]
mod reentrant_handoff_tests {
    use super::*;
    #[test]
    fn obsolete_success_and_error_neither_focus_nor_minimize_over_a_new_execution() {
        for result in [Ok(serde_json::json!({"controlEpoch":1})), Err("late target validation".into())] {
            let filtered = execution_reply_if_current(result, 1, false);
            assert!(!filtered.is_err());
            assert!(!handoff_is_current(&filtered, |_| true));
            assert_eq!(filtered.unwrap()["cancelled"], true);
        }
        assert!(execution_reply_if_current(Err("current validation".into()), 2, true).is_err());
    }
}
