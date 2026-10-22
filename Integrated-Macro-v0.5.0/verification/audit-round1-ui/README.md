# UI / profile race audit evidence

These files record the **pre-fix** reproduction on 2026-10-21. They do not claim that the released application was run.

- `before-ui-reproduction.mjs` executed the then-current `ui/app.js` and `ui/model.js` unchanged through the existing VM DOM harness. Its controlled IPC delayed Save responses and supplied native-shaped state/capture events. The script uses absolute source paths and is historical reading evidence; running it against the fixed sources is expected to fail its old-bug assertions.
- `actual-ui-save-ingress.json` is the actual synthetic profile payload that the production UI sent while F8 was reading. It contains no user settings or key text.
- `before-runtime-save-extracted.rs` copies the production `save`, `apply_capture`, `editable`, geometry helpers, config structures, and storage implementation verbatim from before the fix. It uses the real macro-core Engine. Overlay and logging side effects are stubbed; it is not the full Tauri Worker.
- `result-idle-settings.json` and `result-paused-settings.json` show the temporary settings files after the stale Save removed both the ROI and `seat_end`.
- `before-results.txt` preserves the corresponding successful reproduction output.

Rust commands used project-local `.build-tools/cargo` and `.build-tools/rustup` with the stable toolchain. The probe project and settings lived under `/tmp`; no real app launch, OS key/mouse injection, permission change, or user profile write occurred.

The corrected UI regressions are `ui/tests/profile-sync.test.js` and `ui/tests/run-profile-race-dom.mjs`. They exercise the shipped UI sources with controlled IPC and distinguish these checks from native/OS integration.

Validation after the first implementation: `node --test ui/tests/*.test.js` passed 118 tests. A later guard for arm replies older than a completed capture passed its 21-test affected set. After adding reconciliation for profiles persisted before a failed/cancelled start, the targeted profile/start/shortcut/capture set passed 40 tests. Full native and OS behavior remain outside these UI tests.

## Cross-review follow-ups

The delayed-command DOM test also exposed an unavailable Stop button: `commandBusy` disabled it and the normal command dispatcher refused Stop. The separate `stopPending` path now allows Stop while preserving the original operation's lock. Both reply orders are covered.

The native reviewer reproduced a second merge fault: saved RGB `(255,0,0)`, local `(0,255,0)`, and newly captured `(0,0,255)` were merged into unselected `(0,255,255)`. Axes of points could similarly combine. RGB, points, rectangles, and `capture_reference` now reconcile as whole values. The DOM regression delays the Arm reply until after F8 completes and checks the next actual Save payload; independent options still survive.

Start and Stop replies can share runtime revision values because publication, rather than each command, increments them. The regression supplies an earlier successful Start reply after a stopped snapshot with identical run, runtime revision, and profile revision. UI Stop and F4 invalidate earlier response states for both button and shortcut starts. Profile persistence still reconciles independently.

Final affected-set command after these follow-ups:

```sh
node --test ui/tests/profile-sync.test.js ui/tests/start-integration.test.js ui/tests/start-shortcut.test.js ui/tests/capture-integration.test.js ui/tests/keyboard-check-ui.test.js
```

Result: 45 passed, 0 failed. These remain production JavaScript/VM DOM tests with controlled IPC, not proof of physical keyboard receipt, real Tauri scheduling, native click delivery, or ticket-site rendering.
