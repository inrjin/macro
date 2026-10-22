# Repository Guidelines

## Project Structure & Module Organization

This workspace contains versioned release packages rather than one root application.

- `NOL-Ticket-Assistant-macOS-v0.3.0/source/`: browser extension in `extension/`, Swift app/native host in `macos/Sources/`, tests in `tests/`, and local rehearsal HTML/profile fixtures.
- `NOL-Coordinate-Macro-macOS-v0.4.0/source/`: Swift implementation in `Sources/`, icons in `Resources/`, and `build.sh`.
- `NOL-Coordinate-Macro-Windows-v0.4.0/`: AutoHotkey v1 modules, PowerShell clock utility, bundled `runtime/`, and command launchers.
- Older versions, ZIPs, and `통합 매크로/` contain historical distributions. Scope changes to the intended version; edit source files and rebuild packaged apps. `evidence/` and `verification/` hold review notes and validation artifacts where present.

## Build, Test, and Development Commands

Ticket Assistant commands run from `NOL-Ticket-Assistant-macOS-v0.3.0/source/`, not its parent package directory:

- `npm install`: install development dependencies.
- `npm test`: run Node unit tests.
- `npm run test:browser-all`: run all Playwright browser/extension suites; requires Chrome and Playwright-compatible Chromium. `CHROME_PATH` overrides Chrome for `test:browser` only.
- `bash macos/build.sh`: build universal macOS app/host, sign locally, and run self-tests; requires Xcode Command Line Tools and Python 3.
- `python3 tests/mac-host.test.py`: test native messaging after building.
- `open "macos/build/NOL Ticket Assistant.app"`: launch the rebuilt app.

From `NOL-Coordinate-Macro-macOS-v0.4.0/`, run `bash source/build.sh` to build and self-test. On Windows, run `Self-Test.cmd` before `Start-Macro.cmd` from the Windows coordinate package.

## Coding Style & Naming Conventions

Match surrounding code: JavaScript uses ES modules, two-space indentation, single quotes, semicolons, camelCase functions, and uppercase constants. Swift and AutoHotkey generally use four spaces; Swift types use PascalCase. Keep AutoHotkey v1 compatibility and preserve existing Windows file encodings/line endings. No formatter or linter configuration is supplied; avoid unrelated formatting changes.

## Testing Guidelines

Use `tests/*.test.js` for JavaScript regressions, Node assertions for units, and Playwright for browser behavior. Native checks use the Python host script, Swift self-tests (`Sources/Tests.swift` for Coordinate Macro), and `MacroSelfTest.ahk`. No coverage threshold is configured. Exercise changed behavior with local fixtures; report platform/runtime checks that were not executed.

## Commit & Pull Request Guidelines

No Git metadata is present, so established commit conventions cannot be verified. Use concise imperative subjects, such as `Fix seat retry timeout`. Keep commits scoped to one component/version. PRs should identify affected packages, explain behavior changes, list validation commands/results and platform limitations, link relevant issues, and include screenshots for UI changes.
