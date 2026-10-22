# F9 any-state handoff — implementation record

This record covers the source change requested after the reentrant-command audit. Earlier verification directories and released packages are historical and were not rewritten. Version 0.5.11 and packaging are owned by the parent release task; this implementation did not build or launch an app.

## Contract and transition matrix

F9 is an explicit user assertion that entry/CAPTCHA is complete. It does not identify the current website page or solve a CAPTCHA. No engine phase or previous mode rejects this assertion. The current booking-window, permissions, capture, stored geometry and retained-selection mapping guards still apply.

| Previous condition | Explicit F9 result |
|---|---|
| Idle, scheduled sale wait, initial booking not delivered, registered date/session/next pending, Entry/Captcha | Discard pending input and registered entry queue; Normal post-entry Region when region priority is on, otherwise Seats. |
| Region, WaitingRegion, Seats, WaitingRefresh; any previous mode | New observation generation; post-entry Region/Seats policy. No entry coordinate is replayed. |
| Delivered/selected seats remain | Preserve matching selection/attempt/failure history and active region settings; use Seats to avoid resetting selection with another region click. |
| Paused | Restore the search checkpoint for explicit handoff; the pause itself is not a phase rejection. |
| Stopped, including F4 | Explicit new handoff is allowed; same-window delivery history survives, and a later F4 still cancels it by epoch. |
| Faulted | Explicit handoff is allowed; a known unresolved Conflict checkpoint is restored rather than hidden by Faulted. |
| Conflict | Keep failed-seat identity and successful-Esc latch. Unsent or failed Esc can be retried; successful Esc is not resent. Closure still requires the existing observation/direct-confirmation path. |
| Completed | Explicit retry reopens completion, preserving delivered seats. A fresh map/count observation is required before a new completion input; old ACKs and old generations do nothing. |

## Source changes

- `engine.rs`: phase-independent explicit handoff, discarded entry/sale queue, monotonically increasing action IDs and observation generation. Remembered delivered marks remain protected after Stop clears `current`. A directly confirmed legacy failure is rejected at failure time, including Stop→F9, rather than promoted to a retained successful attempt. Failed Escape delivery retains the Conflict checkpoint; legacy Conflict re-observation is routed before the new continuous-search policy.
- `runtime.rs`: F9 ingress is current-epoch + not-interrupted, independent of run-start timestamps. Idle creates a new logical run; stopped/faulted engines with historical receipts keep that run lineage while their count request/action authorization is renewed. The Observe-only post-entry state is committed before physical validation, so recoverable context loss resumes search rather than old entry. A hard validation error pauses with an explicit cause and no input.
- Successful binding to a changed target ID or PID resets the old engine, queued actions, count proof/cache/receipt and restart context, and advances the run ID. Same-window binding preserves selection history; failed binding does not commit that reset.
- The actual conflict retry caller uses a shared helper tested with the production Engine. Visual attempts retain their existing incident requirement; directly confirmed legacy conflicts retain their candidate identity and delivered-Esc latch.
- F8 capture commit rechecks the ingress epoch immediately before profile storage, after potentially blocking publish/log work. A newer F9/F4 prevents this stale commit. A storage commit already in progress is still a synchronous commit boundary, not reversible OS work.
- Event and helper documentation now matches any-state F9. Prior phase-rejection tests were updated on cloned engines without weakening their original automatic-entry order assertions.

## Reproduction and verification

- Independent original-policy baseline: `../independent/before.log`, 5 failures (Idle/undelivered/Stopped phase rejection).
- Independent failed-Esc checkpoint baseline: `../independent/escape-before.log`, actual Conflict expectation versus Seats failure.
- Independent legacy-known-failure baseline: `../independent/legacy-conflict-before.log`, failed seat incorrectly retained after F9. This first failure log exercises the direct branch; the final test additionally covers Stop.
- Final independent Engine suites: `../independent/final-core.log`, **25 passed**, 0 failed (10 any-state tests + 15 prior reentrant tests). The matrix includes phase/mode/region option combinations, cancelled old entry and sale ticks, old ACKs, count requests, F4 ordering, initial confirmed delivery marks, and unsent/failed/successful Esc.
- Final desktop suite: `desktop-final.log`, **129 passed**, 0 failed, **1 deliberately ignored timing test**. Five new tests in `src-tauri/src/f9_any_state_tests.rs` call the production ingress/context/conflict helpers and Engine. They do not instantiate the live Worker or exercise native input/capture.
- `source-before.json` and `before/` preserve the implementation baseline. `implementation.diff` is the exact before/after implementation diff. `source-final.json` binds nine relevant final files, including both independent suites and the unchanged main caller. Parent final workspace/native/package checks are recorded separately.

## Remaining limits

F9 cannot prove that the browser currently shows a usable seat map. A wrong page, missing connection/permission, changed geometry or incompatible retained selection settings can still prevent input with a specific reason. Transient capture/target recovery uses the existing bounded guards and observation policy. Already delivered OS input cannot be recalled. Selection history is based on the configured evidence policy; optional NOL count verification remains independent. Legacy manually reported dialogs without an armed visual baseline still need the existing direct closed-dialog confirmation; this change does not manufacture visual closure evidence. No physical keyboard, real site, Windows run or application-launch validation was performed by this implementation task.
