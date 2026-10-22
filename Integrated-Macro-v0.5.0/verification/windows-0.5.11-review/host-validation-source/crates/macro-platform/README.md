# Native platform adapter

`macro-platform` contains capture and input mechanisms; booking states and retry
decisions belong to `macro-core`. Use one `NativeAdapter` on a dedicated runtime
worker. It is not a browser extension and does not attach to browser debugging.

## Coordinate contract

`TargetWindow.rect` uses global top-left screen coordinates. Local click points
are relative to this rectangle: macOS uses points, Windows uses physical pixels.
`OwnedFrame.scale_x/scale_y` convert local points to capture pixels; never assume
Retina is 1× or add a second Windows DPI conversion. `pixel_to_local` returns a
pixel center and `color_at` respects stride. Negative monitor origins are valid.

Window PID, ID, geometry and foreground identity are checked before actions.
A click additionally checks occlusion at the point. A changed window requires
rebinding. Own overlays can be registered by ID; another process's windows
cannot be exempted. Register a Tauri NSWindow on the macOS main thread through
`unsafe owned_overlay_window_id(window.ns_window()? as usize)`, then pass the
result to the worker. Windows uses HWND.

`activate_target()` is reserved for explicit starts/resumes from the settings
UI. It restores/raises the saved window and verifies its actual foreground
identity before returning; it never binds whichever window happens to be active.
Call it on the worker while the main UI thread remains free. On macOS the saved
CG window must map uniquely to a public AX window by PID and geometry; ambiguous,
closed, moved or resized targets fail with actionable errors. macOS 14+ uses
cooperative activation on the main thread and then fresh AX focus checks.
Windows restores the saved HWND, verifies PID/geometry, and requests foreground
activation without synthetic keys or input-thread attachment. Both paths bound
the foreground transition wait to approximately 1.5 seconds (individual AX
calls retain their 120 ms timeout). Hotkeys and normal input/capture loops must
keep using strict validation without automatic activation.

## Capture and input

- **macOS 13+:** a statically linked Swift ScreenCaptureKit stream continuously
  retains the latest frame. Its display crop includes only the bound window and
  excludes the status overlay. Keep the entire target on one display. Image
  sequence/timestamp and stream heartbeat are separate; an unchanged frame is
  not falsely marked as a new image. `capture_if_changed(sequence)` avoids
  copying unchanged pixels; continue processing state deadlines on `None`.
  Pixels are delivered as packed RGBA in
  native capture scale and sRGB. No helper process is launched per click.
- **Windows 10 2004+:** a persistent GDI DIB receives synchronous desktop crops.
  The worker uses per-monitor DPI awareness. Own overlays use
  `WDA_EXCLUDEFROMCAPTURE`. GDI is a functional baseline, not a claimed fastest
  capture backend; measure it before choosing DXGI or Windows Graphics Capture.
- Mouse input explicitly moves, presses and releases. Keys supported are Esc,
  Up, Down and Enter. Modifier keys held by the user stop synthetic input.
  Windows checks the target's process integrity when binding and verifies the
  count accepted by `SendInput`. macOS checks Accessibility permission and
  event construction, but CGEvent posting has no delivery acknowledgement.
  **Successful I/O never implies successful seat selection.**
- `beep()` queues the native system sound for pause/resume acknowledgement.
  It requires no capture/input permission; a sound failure must not undo pause.

`permissions()` performs preflight checks without requesting permission. Missing
macOS Screen Recording and Accessibility permissions produce separate messages.
The runtime must display the original `PlatformError.code/message`, not replace
it with a generic connection error. Its `Display` text includes both as
`[error_code] Korean guidance`, so runtime logs retain the failure category.

## Validation

Run `cargo test -p macro-platform` and build the desktop application on **both**
operating systems. `native/verify-macos.sh` builds arm64/x86_64 Swift libraries
and executes synthetic capture lifecycle, packed-row copy and allocation/free
ABI checks without capturing a screen, clicking or prompting.
The current checks do not establish actual capture, hotkey, input, overlay,
mixed-DPI or booking compatibility. Those require desktop rehearsal with the
packaged application and granted permissions.

Primary API references:
[ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit),
[SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput),
[DPI context](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setthreaddpiawarenesscontext).
