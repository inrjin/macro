# Independent Windows platform review — released 0.5.11

Reviewer: `/root/fix_cancellation_start`. Scope: read-only code and primary documentation review, with SHA-256 comparison to `verification/release-0.5.11/source-before-build.json`. No application, input injection, keyboard monitor, permission change, or Windows runtime was executed by this reviewer. The current unreleased calibration changes are outside this release.

## Result

The Windows adapter contains implementations for window binding, foreground validation/restoration, synchronous screen capture, input posting, and passive Enter/first-character monitoring. It is not a stub adapter. This review found no additional demonstrated source-level release blocker. It does **not** establish that this source compiles, links, installs, or operates correctly on Windows.

The available local build attempt is blocked before Windows compilation: `windows-check-attempt.log` reports Rust `E0463`, missing the `x86_64-pc-windows-msvc` target's `core` crate; `windows-check-attempt.json` records exit 101 and no Windows binary/runtime validation. The parent also reported the unavailable cross SDK/dependencies and blocked toolchain download. A build kit and an executable are different artifacts; no Windows executable can be claimed from this attempt.

## Actual implementation and limits

| Area | Source evidence | Finding |
| --- | --- | --- |
| DPI and coordinates | `crates/macro-platform/src/windows.rs:120`, `:134`, `:302`, `:375` | Per-thread per-monitor-v2 DPI context; physical window coordinates; virtual-desktop absolute mouse conversion. Window position, size, and PID must exactly match the bound target. A window move can require rebinding. Mixed-DPI/multi-monitor behavior remains untested. |
| Capture | `windows.rs:827`, `:883`, `:891` | Desktop `BitBlt` plus `GdiFlush`, BGRA-to-RGBA copy, forced alpha 255, scale 1. Every capture increments the sequence; unchanged pixels still return `Changed`. This is synchronous full-frame GDI capture, not a measured DXGI/SCK-equivalent performance implementation. |
| Foreground and recovery | `windows.rs:245`, `:302` | Restores the same HWND and asks Windows for foreground activation, then verifies the real foreground. It can return activation/geometry failures; no automatic new browser popup tracking is present here. Point validation uses visible top-level rectangular bounds, not browser hit-target semantics. |
| Input and cancellation | `windows.rs:348`, `:375`, `:392`; common adapter in `lib.rs` | The cancellation predicate is checked after validation and next to `SendInput`. Held modifier keys reject the action. A partially accepted batch attempts the matching release and reports failure. Full OS queue acceptance is not browser/site selection confirmation. |
| Privilege | `windows.rs:168`, `:225`, `:883` | Desktop/DC access is checked; bind rejects a browser with greater token integrity. This is not a macOS permission model. |
| Passive keyboard input | `windows.rs:415`, `:619` | Separate bounded low-level-hook threads for Enter and character activity; injected/repeated/held-at-arm/modifier inputs are filtered, and all events pass to `CallNextHookEx`. Character content is not exported. Actual Windows hook delivery is untested. Paste/autofill are outside the first-character contract; firmware Fn combinations may be indistinguishable from normal character VKs. |
| Keyboard diagnostics | `windows.rs:182`, `:878` | macOS-specific health fields are `None`, and native callback/rejection diagnostic methods return empty vectors on Windows. Runtime delivery/routing logs still exist, but Mac-level callback diagnostics are not implemented on Windows. |
| Platform build | `crates/macro-platform/build.rs:5` | The native Swift build is skipped for non-macOS targets. Windows links raw `user32`, `gdi32`, `kernel32`, and `advapi32` FFI instead. Windows-only unit tests are not exercised by a macOS workspace test run. |

The common adapter adds target checks around capture and before input. These checks do not turn a fresh GDI capture or successful `SendInput` return into proof that a site accepted a seat.

## Version and compatibility distinctions

The adapter's per-monitor-v2 DPI call assumes Windows 10 1703 or newer. Overlay exclusion uses `WDA_EXCLUDEFROMCAPTURE`; Microsoft documents full exclusion from Windows 10 2004, while older versions behave as `WDA_MONITOR`. Consequently, the code's 1703 DPI minimum alone is not evidence that the overlay yields identical underlying pixels on every earlier Windows 10 release. [Microsoft DPI contexts](https://learn.microsoft.com/en-us/windows/win32/hidpi/dpi-awareness-context), [display-affinity behavior](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity).

Windows can silently remove a low-level keyboard hook after a timeout. The implementation checks its thread/pump failure but explicitly cannot prove a timed-out hook remains installed. Microsoft's input API also documents integrity-level restrictions and returns inserted-event counts, not website acknowledgements. [LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc), [SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput).

The optional NOL selected-count reader uses bundled raster glyph templates: 295 ordinary templates include AppleGothic, AppleSDGothicNeo, Arial/Helvetica variants and a user-provided heading; 78 negative-only templates use Apple fonts. No Windows-rendered site corpus was validated here. Supported Windows font/rasterization recognition is therefore unverified, not a known universal failure. The CAPTCHA background-color detector does not use these text templates. Both readers' compilation is distinct from recognition accuracy on a real Windows browser.

## Build-kit review requirements

Tauri's Windows development prerequisites are Microsoft C++ Build Tools with the desktop C++ workload, Rust MSVC, and WebView2. This project also uses Node/npm for its frontend and tests. NSIS packaging avoids the separate MSI/VBScript path. Final PE dependencies, signing status, and installer contents must be checked after a real successful build; no blanket VC++ redistributable claim is made before PE import inspection. [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

The original `scripts/build-windows.ps1` uses `npm install` and unlocked Cargo commands, but it **does** check `$LASTEXITCODE` after all four external build/test commands. It is retained unchanged for release source fidelity. A separate kit should preserve explicit failure checks while adding locked dependencies, source/fixture hashes and actual expected EXE/installer checks before producing a success receipt; it must not auto-run the app or alter its settings. The checked-in GitHub workflow's existence does not demonstrate that a Windows run passed; it currently uploads lock files, not a distribution binary.

At review time no dedicated Windows runner was available in the tool inventory. The presence of GitHub workflow read/rerun tools is not evidence of an authorized repository or Windows runner. No remote workflow was triggered.

## Evidence

- `platform-review.json`: checked file hashes, release/staging equality, local attempt references, and unverified scopes.
- `windows-check-attempt.log` and `.json`: parent's actual blocked cross-target attempt.
- Build-kit source review is a separate follow-up; this document does not approve an uninspected future script.
- Peer review corrected an initial statement about missing external exit-code checks: rereading the original script confirmed the checks exist. The remaining original-script gaps above are stated narrowly.
