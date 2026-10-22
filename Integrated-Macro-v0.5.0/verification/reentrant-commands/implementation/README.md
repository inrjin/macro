# Reentrant execution commands — implementation record

This is source verification, not an application build, native-input test, or release. Prior distributions and verification records were not edited.

## Behavior

- F1/F2/F5, UI Start, and explicit F9 acquire a new control epoch at ingress. Superseded requests are rejected before they alter the current gate, watches, engine, or capture state. Native guarded input observes that epoch; input already posted cannot be undone, and the existing down/up completion contract remains intact.
- A current execution request cancels unsent actions and stops/pauses the previous work before validating new settings. Validation failure leaves the previous work stopped/paused with an error; it does not resume it.
- Repeated identical start settings can reuse an already-saved revision. A stale different profile or a future revision still fails CAS. Late start successes/errors do not minimize/focus windows over a newer command.
- Compatible F2/F5 restarts preserve delivery/selection marks, failure exclusions, unresolved count attempts, logical run ID, and the real native receipt lineage. Action/request IDs and observation generations move forward; old ACKs and count requests do not authorize new work.
- F9 is an explicit post-entry handoff/retry. It never replays entry coordinates. It accepts post-entry search, corresponding pauses, completion, and F2/F5 search states. A delivered initial booking click waiting in Entry/AwaitingMap also accepts F9 when the automatic CAPTCHA routes are off. Idle/Stopped, waiting for sale, and undelivered initial/automatic entry remain blocked.
- F9 switches to Normal execution settings. Retained selections keep their current region-specific ROI/color/completion mapping. Without retained selections it begins the saved Normal region search, or Seats when region priority is off. Existing conflict recovery is retained, including whether Esc was already delivered.
- Explicit Completed retries reopen completion only after a new map observation (count OFF) or a new valid count request (count ON). Automatic ticks and obsolete ACKs cannot repeat completion. A retained count above a lowered target blocks further seat, refresh/navigation, and completion input until the user corrects the target/context.
- `features.manual_entry` defaults false. In Normal mode it preserves the initial booking-button/sale-time policy but skips the registered date/session/next queue and automatic entry steps. The user continues manually, then the enabled NOL/Enter signal or F9 hands off to search. OFF preserves the original entry order.

## Context compatibility

Runtime compares bound window, capture dimensions/scale, effective ROI/exclusions/component limits, available color/tolerance, count verification policy, and object-extraction policy before retaining selection history. Region navigation configuration is compared where an active cancellation region is retained. Target count is not seat identity and may change. A failed save/start followed by F9 cannot combine an old engine with incompatible newly saved color/ROI/count/object settings.

F1 remains a fresh-entry command. A fresh run does not infer existing selections from prior application history. With count verification OFF, delivery remains an estimate and cannot prove site acceptance. Same window/geometry/ROI does not prove that the user has not manually navigated to a different seat map. These are limitations, not tested site identity claims.

## Verification

- `runtime-focused.log`: first desktop compile caught JSON macro recursion depth after adding a snapshot field. The field moved to the existing post-construction insertion path; no recursion-limit increase.
- `runtime-focused-2.log`: initial 7 reentrant/handoff helper tests passed.
- `runtime-all.log`: desktop suite 124 passed, 1 manual timing test ignored. Tests exercise actual ingress, gate, mapping and Engine functions; they do not instantiate a live Worker or deliver OS input.
- `core-engine-all.log`: existing core engine suite 102 passed. This checkpoint preceded the final single-preflight correction in `restart_after_entry`; the independent regression and parent's final workspace run cover that final correction.
- Independent core review records live under `../independent/`; final 15 tests passed with before/after source hashes unchanged. They include original F9 failures, the lowered-target over-selection failure, completed retry, conflict Esc state, cross-mode/region mapping, initial-button handoff, manual-entry controls, and the single-preflight correction when new CAPTCHA options differ from the previous run.
- UI/response races are independently checked under `../adversarial/` and the parent's UI test records.
- `source-final.json` and `implementation.diff` bind this implementation to exact before/final bytes. Parent final workspace/JS results are recorded separately and supersede checkpoints where appropriate.

The actual macOS/Windows hotkey/native scheduling behavior, physical input already in the OS queue, live page transitions, and real-site CAPTCHA completion were not executed. Windows MOD_NOREPEAT remains unchanged. No speculative cross-platform debounce was added; macOS held-key behavior remains a live validation item. Snapshot/control epochs cannot retract a window operation already performed or a profile save already committed before a newer command arrived.
