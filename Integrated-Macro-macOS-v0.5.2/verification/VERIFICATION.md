# Native validation — 2026-10-21

Environment: macOS arm64, Apple Swift 6.2.1, Xcode Command Line Tools SDK.

| Check | Result |
| --- | --- |
| Optimized Swift static library, arm64, macOS 13 deployment target | Passed |
| Optimized Swift static library, x86_64, macOS 13 deployment target | Passed |
| Host C ABI link with the same Swift libraries/frameworks as `build.rs` | Passed |
| Capture allocation/free, returned error string/free, main-thread capture rejection | Passed |
| Production window validator with 18 synthetic foreground, window identity, geometry and point occlusion cases | Passed |
| Synthetic capture lifecycle: first image, status-only start, static idle, interrupted capture, fresh-frame recovery | Passed |
| Synthetic padded BGRA buffer copied to packed rows through the C ABI | Passed |
| Non-prompting Screen Recording/Accessibility preflight | Returned `0`: neither permission granted to the test process |
| Windows INPUT and BITMAPINFOHEADER C layout assertions via clang x86/x64 targets | Passed on 2026-10-20; unchanged |
| `cargo test -p macro-platform --offline --locked` | Passed: 4 unit tests; production Swift helper compiled |
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
windows do not hide a visible sample point.

The synthetic buffer harness emitted macOS sandbox/IOSurface diagnostics but
completed all assertions. It did not start a capture stream. The test-only C
entry points are excluded from production builds.

Compilation and ABI checks do not establish that a packaged app can control a
browser. A permitted desktop rehearsal on each OS is still required.
