# Windows 0.5.11 adversarial review

Scope: release-source lineage, test-input completeness, build-script/caller consistency and artifact claims. The reviewer changed only new evidence in this review directory. No app, settings, prior release, original source, Windows installation or remote workflow was operated.

## Result

The staged 138 source files exactly match the recorded macOS 0.5.11 release. The later calibration retry implementation is absent. The 417 fixture/manifest files include all 410 manifest-referenced RGBA scenes used by the shared/desktop tests. This is evidence of a faithful source and test kit, not evidence of a Windows executable or working ticketing flow.

No executable readiness approval is possible: the available target check fails with E0463 (missing Windows Rust target core), and no Windows compiler/linker or runtime execution has succeeded. The README accurately labels the deliverable as a build preparation kit. Existing 0.5.11 behavior and known limitations are intentionally preserved.

## Objections and correction review

1. Source138 alone omits filesystem fixtures required by Rust tests. The parent added the exact seven referenced manifests and 410 RGBA files. Independent hashes and reference checks are recorded in `adversarial-source-review.json`.
2. Git's CRLF conversion can break byte-based release hashes on Windows. An isolated Git worktree with `core.autocrlf=true` reproduced the mismatch; `* -text` preserved the original hash. The kit now contains `.gitattributes`; the README instructs preserving hidden repository files.
3. The original workflow invoked PowerShell without establishing the native-tools environment demanded by the script. Both callers now use the CMD wrapper; it locates installed VS through `vswhere`, initializes `VsDevCmd` for x64 within `setlocal`, checks failures, then invokes PowerShell. The PowerShell preflight still requires the actual compiler/linker/SDK commands and MSVC architecture.
4. Tauri regenerates platform schemas. The initial input tree is exact, and only four named generated schemas may differ in a unique build copy. This exception does not cover application/config/lock/UI inputs.
5. The parent identified partial failed artifacts being visible to `if: always()` upload. The revised script verifies all candidate PE files and metadata first, then renames the directory to `artifacts` and sets success. Failed stages retain diagnostics; candidate files are excluded from the workflow upload paths.

The script uses `npm ci`, locked Cargo tests/builds and the installed local Tauri CLI. Per-command nonzero or absent native exit codes fail the attempt. Unique target/output directories prevent a previous build's binary from satisfying a new attempt. The original source is rechecked, and the application is never auto-run. File and product versions and Authenticode status are recorded. These observations are source review only: no PowerShell parser/runtime or CMD/Windows execution is available here.

## Runtime and packaging distinctions

Build Tools, Rust and Node are requirements of the build machine, not a reason to ask ordinary app users to install Visual Studio. Tauri's current local build dependency enables static VC runtime linking by default; the final PE import table must decide additional runtime redistribution requirements. No PE import table exists for this attempt.

The original configuration uses Tauri's default WebView2 bootstrapper mode. A built NSIS installer may download the missing WebView2 Runtime; a bare application EXE does not by itself establish that prerequisite. No Authenticode signing certificate is configured. These facts are now in the kit README. [Tauri Windows distribution](https://v2.tauri.app/distribute/windows-installer/), [Microsoft WebView2 distribution](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution).

Shared/macOS tests, source parity and a future PE link are separate from Windows GDI capture, mixed-DPI coordinates, foreground/privilege restrictions, keyboard hooks, WebView2 startup, actual installation and site acceptance. The platform reviewer separately documented those boundaries. Their initial claim that the old script lacked per-command exit checks was challenged and corrected: it already had those checks; its missing protections were dependency locking and source/artifact lineage verification.

## Frozen evidence

`adversarial-buildkit-review.json` records the exact reviewed CMD, PowerShell, workflow, README, attributes and manifest hashes. `adversarial-source-review.json` is the source/fixture byte inspection; `adversarial-line-ending-probe.json` is the local Git reproduction. Any package/archive validation is a separate later step and does not convert this kit into an executable release.
