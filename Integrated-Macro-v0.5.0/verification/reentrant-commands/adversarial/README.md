# Reentrant command adversarial review

Scope: source and actual JavaScript event handlers with deferred Tauri responses. No app launch, native key/click, live browser, release or version edits. The implementation belongs to root/UI and seat_shortcut/Rust; this reviewer owns two new DOM test files and this evidence directory.

## Reproduced baseline

`before/` preserves seven relevant production files, with their hashes in `before.json`. Missing UI module dependencies were copied unchanged solely to execute the preserved module through the existing minimal DOM harness.

Command: `REENTRANT_UI_ROOT="$PWD/verification/reentrant-commands/adversarial/before/ui" node --test ui/tests/reentrant-review.test.js`.

`before-dom.log`: nine initial scenarios failed. Distinct first failures were running Start disabled, pending Start disabled, subsequent shortcut rejected as busy, and older runId event accepted despite a newer run being displayed. Later assertions for outdated errors/finally and Stop ordering were not reached behind that initial busy rejection; nine failures do not represent nine independently reproduced defects.

The tests additionally cover native control epochs, cleanup events before a new successful start, equal-revision stopped→old-running reversal, F2 mode switches, the real overlay listener, and late initialization errors. `third-after-dom.log` reports the earlier 19/19 checkpoint. After root declared UI source frozen, `final-dom.log` reports 19 passed / 0 failed, exit 0. `ui-final.json` binds that final run to the actual app/model/overlay sources and independent DOM tests.

## Objections sent to implementation

- `main.rs::start_run` reveals any delayed error by showing/focusing the settings window. UI rejection of stale errors alone cannot stop this native side effect. Request epoch must survive through error delivery.
- `runtime.rs::handle` originally clears pending start/run epoch before checking stale command ownership. Atomic epoch assignment and channel enqueue are separate, so different sender threads can queue older requests later. An obsolete command must not mutate the newest run.
- Start A can save revision 0→1 before its reply reaches UI; newer Start B still carries expected revision 0. Strict CAS then rejects B even if its profile is identical to A. Preserve genuine F8/Save conflict protection while handling idempotent repeated execution requests explicitly.
- New UI execution followed by an intermediate stopped event must not be invalidated solely by a local response counter. Conversely, F4/F9 with a higher native control epoch must reject old success/error replies and old state events.
- Restarting the engine does not deselect browser seats. Image/count identity and retained-selection behavior must be checked by the separate engine reviewer; UI test success cannot establish those semantics.

## Additional independent counterexamples

The independent engine reviewer identified the validation-error shortcut branch as missing native epoch checks. The actual DOM test `invalid-shortcut-native-newer` reproduces an invalid F1 request followed by native F9 with a higher control epoch: the old error was displayed over the new run. `stale-invalid-shortcut-before.log` records failure; root now checks request ID, local response epoch and native epoch before applying rejection results. The corrected branch passes.

`overlay-bootstrap-before.log` independently reproduces an old bootstrap error overwriting a live overlay event. The current overlay keeps live status after such initialization failures. The passing checkpoint covers both corrections, including downstream busy/finally assertions previously blocked at the first rejection.

## Platform keyboard-repeat review

Local primary dependency source global-hotkey 0.8.0 uses Windows `MOD_NOREPEAT` for registration and generates Released by a 50ms `GetAsyncKeyState` polling thread. Adding a generic held-key latch could therefore discard a genuine rapid second press before the synthetic release is observed. macOS passes Carbon HotKeyPressed/Released through directly. A macOS-only edge guard was discussed but not adopted without evidence about real Carbon repeat delivery. The existing platform registration remains unchanged, with no arbitrary debounce or generic held-key latch. Actual held-key repeat and rapid release/repress behavior were not exercised on either OS. This distinction was sent to the implementation owner and retained as an unverified platform limitation.

## Corrected callers inspected

Current `main.rs::start_run` carries the command's accepted control epoch through the asynchronous reply and suppresses obsolete success/error window side effects. Runtime stale control requests are rejected before clearing pending/new-run ownership; ApplyShortcut results also undergo an epoch check before reporting errors. `save_start_profile` permits revision-mismatched starts only when the whole submitted profile exactly equals the current authoritative value, leaving actual stale changed profiles subject to CAS. `implementation/runtime-all.log` reports 124 passed / 0 failed / 1 ignored; its new `reentrant_runtime_tests.rs` uses actual ingress/gate/profile helpers and Engine, not an instantiated native Worker. `main.rs::reentrant_handoff_tests` checks obsolete success/error suppression without creating real windows.

## Final F9 and retained-state cross-review

The corrected Engine permits F9 during Seats/Cancellation searches and after Normal entry has actually been delivered, including the old `Entry/AwaitingMap` manual handoff. It does not bypass an undelivered/scheduled booking click. Stopped/never-started operation still uses F1/F2/F5; the UI now says so.

An explicit F9/F2/F5 retry after Completed reopens completion only after a new map observation (existing optimistic policy) or a new request's count evidence (count option ON). Old ACKs, old-generation candidates and Tick alone cannot resubmit it. Retained selected/delivered/failed footprints remain; a lower target cannot add seats beyond the retained total. `independent/after-cross-mode-entry.log` is the 14-test checkpoint; `independent/final-core.log` reports 15 passed / 0 failed on the frozen source, including the final profile-gate regression. These cover the retry paths and manual-entry ON/OFF. This reviewer read the tests and actual Engine callers; those tests deliberately supply Engine events and are not evidence of native count-reader or ticket-site success.

The other independent reviewer found failed settings/start followed by F9 could combine an old Engine with newly saved Worker settings. The corrected F9 applies the current effective Normal profile via `restart_after_entry`; retained selections must pass the current color, effective ROI, tolerance, object/count policy and active-region compatibility checks. It validates the current window and capture geometry before committing the handoff. F2 → F9 → F2 retains the effective region mapping. Runtime's pure helper regression checks these configuration boundaries. The final `restart_after_entry` also applies the old-state handoff preflight exactly once: newly enabled manual/NOL/Enter options cannot turn the accepted handoff into a silent no-op. Its regression was added after source inspection; no failing before-log was captured for that last finding. Actual Worker/Tauri/native integration was not executed here.

## Outcome and limits

No remaining blocking contradiction was found in the final reviewed source and the reproduced UI scenarios. The final source manifest records the reviewed files; parent's complete workspace verification is separate from this focused review.

- Latest-command checks prevent old queued/blocked work from authorizing new input. Input already submitted to the operating system cannot be recalled by F4 or another execution command.
- Same browser and capture geometry do not prove the page still represents the same seats. Existing image ambiguity and optimistic counting limitations remain; no new semantic success detector was added.
- Deferred IPC tests execute actual UI handlers through the repository's minimal DOM harness. They do not exercise real Tauri delivery timing, app focus, macOS/Windows input, physical shortcut repeat, browser rendering or a live ticket site.
- This task changes source only. No new app build, ZIP, version, installed app, permission or user settings were changed by this reviewer; existing releases are outside this review's mutation scope.
