# Native validation — 2026-10-21

## Passive Enter listener — native checks passed, desktop rehearsal pending

After explicit approval, `verify-macos.sh` compiled the new source for arm64
and x86_64 and passed the host C ABI harness, including the Enter key metadata
filters and off-only allocation/drain. Existing window, activation and capture
fixtures also passed. See `verification/native-review-latest.log` and
`verification/native-review-latest.json` for output and source hashes.
The test process reported permission bits `0`; no real listener was installed.

macOS observes Return/keypad Enter through an AppKit global event monitor.
It filters repeat/modifier events and events attributed to this process, keeps
at most 16 metadata records, and never reads characters. A missing event window
number remains `0`; the runtime must combine the recorded foreground PID with
fresh exact-window validation. The monitor does not provide a universal
hardware-only guarantee for another application's synthesized events.
Main-thread installation has a one-second deadline and a cancellation epoch;
late work cannot arm a timed-out watch. Removal invalidates queued signals first.

Windows uses a dedicated `WH_KEYBOARD_LL` message thread and always forwards
keys to `CallNextHookEx`. Its bounded nonblocking queue excludes injected keys,
repeats and modifier combinations. Installation waits at most two seconds;
shutdown confirmation waits at most 500 ms and never joins an unfinished
thread. A failed shutdown invalidates the receiver/token and returns an error.
Windows may silently remove a hook whose callback exceeds the OS timeout;
thread health alone cannot prove continued hook registration.

Both adapters clear events when the token changes and discard events older
than 500 ms or dated in the future. Notification and captured-frame timestamps
use the same per-platform monotonic clock. Capture timestamps denote SCK image
arrival / GDI capture completion, not a browser rendering guarantee.

The actual `windows.rs` and `lib.rs` also passed host metadata-only
`cargo check --tests` using a temporary target-selection harness. This includes
typechecking the repeat/release, held-key and C-layout regression source; it
does not link Windows libraries or execute those tests. Its source hashes and
limits are recorded in `verification/windows-native-review-latest.json`.
Workspace Rust test/build results are recorded separately by the aggregate
check. None of these checks establish real browser Enter delivery.
Desktop rehearsal must still exercise stop/pause, wrong-window inputs,
CAPTCHA rejection/retry and actual Return/numeric Enter on both systems.

Public listener contracts:
[AppKit global monitor](https://developer.apple.com/documentation/appkit/nsevent/addglobalmonitorforevents%28matching%3Ahandler%3A%29?language=objc),
[Windows low-level keyboard hook](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc),
[Windows injection metadata](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-kbdllhookstruct).

## 0.5.4 activation change

The new explicit `activate_target()` path preserves the saved target instead of
binding the settings window. macOS matches the saved CG ID/PID/geometry to one
public AX window, requests its restoration/focus/raise, cooperatively activates
its application on the main thread, and then polls fresh focused AX + CG evidence
on the worker. Window closure, PID reuse and geometry changes abort immediately.
Denied or delayed foreground transfer expires instead of starting input.
Normal hotkeys, validation and click guards do not activate windows.

Sixteen new synthetic cases exercise exact-target selection, a preceding tiny
Chrome helper, off-screen saved identity, closed/reused/moved targets, duplicate
CG or AX geometry, unavailable/wrong-owner AX windows, immediate or delayed
activation, bounded refusal, and non-retryable permission/identity failures.
These call the production selection and wait helpers with injected metadata,
clock and results; they do not invoke real window activation. The 43 prior
window cases, lazy probe/cache checks, lifecycle and C ABI checks remain.
Optimized Swift compilation passed for arm64 and x86_64, and the host C ABI
harness passed all 16 activation and 43 window cases plus the existing checks.
The test process reported permission bits `0`. Results, output and source hashes
are recorded separately in `verification/native-0.5.4.json`; the table below
describes the retained 0.5.3 baseline rather than a live desktop activation test.
The actual Windows Rust module also passed host metadata-only type checking;
`verification/windows-activation-0.5.4.json` records its source hashes. This did
not link a Windows executable or validate behavior on Windows.

Public activation contracts:
[Apple coordinated application activation](https://developer.apple.com/documentation/appkit/nsrunningapplication/activate(from:options:)),
[Windows foreground activation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow).

## 0.5.3 baseline

Environment: macOS arm64, Apple Swift 6.2.1, Xcode Command Line Tools SDK.

| Check | Result |
| --- | --- |
| Optimized Swift static library, arm64, macOS 13 deployment target | Passed |
| Optimized Swift static library, x86_64, macOS 13 deployment target | Passed |
| Host C ABI link with the same Swift libraries/frameworks as `build.rs` | Passed |
| Capture allocation/free, returned error string/free, main-thread capture rejection | Passed |
| Production window validator with 43 synthetic foreground, window identity, geometry and point occlusion cases | Passed |
| AX probe laziness (1 assertion) and observation-only focus cache expiration/invalidation (8 assertions) | Passed |
| Synthetic capture lifecycle: first image, status-only start, static idle, interrupted capture, fresh-frame recovery | Passed |
| Synthetic padded BGRA buffer copied to packed rows through the C ABI | Passed |
| Non-prompting Screen Recording/Accessibility preflight | Returned `0`: neither permission granted to the test process |
| Windows INPUT and BITMAPINFOHEADER C layout assertions via clang x86/x64 targets | Passed on 2026-10-20; unchanged |
| `cargo test -p macro-platform --offline --locked` | See final workspace check record; root added two parser regression tests in 0.5.3 |
| Windows Rust compile or actual desktop execution | Not run |
| Actual screen capture, click, key, beep, focus/occlusion, overlay or mixed-DPI behavior | Not run |

Reproduce the native Mac checks with
`bash crates/macro-platform/native/verify-macos.sh`. They do not request
permissions, capture a screen, play a sound, or send input. After the last source
change, both architecture libraries were rebuilt and the host ABI harness was
relinked and run successfully:

```text
Native synthetic window validation, capture lifecycle, ABI allocation/free and thread guard OK; permission bits=0
```

The window tests call the same `validateWindow` implementation used by both
production C ABI entry points. Read-only calibration succeeds when the bound
point is visible but the settings app still owns keyboard focus, or a different
browser window is in front without covering that point. Input still requires
the bound browser and exact window to have focus. Tests also reject the wrong
popup under the pointer, unrelated occluders, unregistered settings windows,
foreign window IDs passed as overlays, points outside the target, closed or
reused window IDs, and changed geometry. Registered own overlays and transparent
windows do not hide a visible sample point when ordinary window metadata proves
visibility. An AX hit on our own overlay is not accepted as browser evidence.

The reported Dock surface (window 13, PID 1397, layer 20, rectangle
`0,0,1512,982`) and Chrome windows (helper 125397 and target 123013) are replayed
with both reported in-window points and the final out-of-window point. Only an
exact `com.apple.dock` bundle match plus an active-display-sized Dock-layer
surface can trigger a system-wide AX hit-test. Positive target PID and unique
AX-window-to-CG-bounds identity are required before continuing through the
remaining window list. Genuine Dock/menu hits, unrelated windows, unavailable
AX, wrong browser windows, ambiguous equal bounds, and missing Accessibility
permission remain blocked. The supplied terminal diagnostic measured screen
recording permission, not Accessibility; all AX outcomes in these tests are
synthetic.

Foreground binding resolves public `AXFocusedWindow` geometry to a unique CG
window instead of choosing the first normal-layer browser helper. Explicit
input validation remains fresh. Only frame observations may cache ambiguous
focus results for 75 ms, keyed by ordered browser IDs/geometry/alpha, target,
and foreground PID. The suite checks expiry, order/geometry/foreground changes,
clear-before-fresh-validation, backwards time, and non-caching of failures.
Normal unambiguous validation performs no AX calls. AX messaging calls have a
120 ms per-call timeout and never request permissions.

The exact combined native output, exit status, and source/verifier SHA-256 hashes
for this run are retained in `verification/native-0.5.3.json`.

The synthetic buffer harness emitted macOS sandbox/IOSurface diagnostics but
completed all assertions. It did not start a capture stream. The test-only C
entry points are excluded from production builds.

Compilation and ABI checks do not establish that a packaged app can control a
browser. A permitted desktop rehearsal on each OS is still required.
