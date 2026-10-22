# F9 any-state adversarial review

Scope: independent read-only production review and existing actual-DOM regressions. No app launch, physical input, live booking, settings or prior-release modification. The new distribution build is owned by root; this reviewer will inspect its contents after packaging.

## Initial evidence

`initial.json` and `initial/` preserve the source checkpoint read at review start. This is a checkpoint, not a claim that no other team member had begun work. The requested change explicitly removes F9 phase restrictions, including Idle, Stopped, Faulted, WaitingSale and undelivered Entry. It still requires a usable bound window and the user's existing region/seat settings; F9 does not identify arbitrary page semantics.

## Findings sent to implementation

1. **Cross-window retained-state provenance.** In the initial caller source, `Command::Stop` clears `restart_context` while Engine Stop retains selected/attempted/count history. A successful `Command::Bind` replaces the native target and clears captures but does not replace the Engine. Thus A-window selection → F4 → B-window binding → F9 can use A's retained history in B when color/ROI/geometry match. Previous Stopped-phase rejection hid this route. Recording the current target only at the next supersede is insufficient: it is already B. Require original context provenance or explicit new-context reset after successful changed-window binding. Source-derived counterexample; no actual Worker/native execution was claimed.
2. **Misleading recovery instruction.** If F4 → F9 preserves delivered seat history, the old “F4로 기록을 끝내세요” message is no longer a valid solution to F9 mapping conflicts. The text must match actual state behavior.
3. **Physical preflight versus state independence.** Missing connection, denied capture/input or invalid foreground window remains a concrete input-free failure. The revised implementation commits a post-entry checkpoint before physical validation and reuses the existing `pause_for_context` recovery loop for recognized transient foreground/capture errors. Recovery must resume region/seats, never the abandoned entry queue. Missing connection/permission or incompatible geometry can still require a corrected setup and another F9; there is no automatic browser binding. This is distinct from the removed internal phase rejection.

## Required adversarial boundaries

- Pending initial/date/session/next actions must be cancelled; old ACKs and later scheduled ticks must not revive them.
- Newer F4/F1/F2/F5/F9 epochs must supersede delayed F9 validation/capture/results.
- Idle/new bindings establish a new logical-run/proof lifetime. Stopped/Faulted may preserve their logical run and delivery receipt lineage, but old count authorizations/requests must not survive.
- Do not count already delivered seat input twice, lower target below retained total, or resend a delivered Escape merely because F9 is repeated.
- Latest UI/overlay events and responses must remain monotonic; UI wording must stop telling users F9 requires a prior running mode.

## Follow-up source findings

- The Bind correction compares both target window ID and PID and resets Engine/input/count proof only after a successful changed-window binding. Rebinding the same window preserves its history. This closes the cross-window counterexample without mutating saved settings.
- Independent Engine review added failed-Escape → Faulted → F9 and known legacy-conflict → F9 counterexamples. This reviewer traced their callers and objected that a legacy conflict must remain known as failed even when F4 clears `current` before F9; the failure cannot be reconstructed only from a current-candidate pointer.
- F8 initially checks its epoch immediately after native capture, but then calls publish and log I/O before saving. A later control received in that interval can stale the prepared write. A final epoch check before entering save was requested. Already-entered storage commit is a separate atomic operation boundary and must not be described as infinitely cancellable.

`ui-final.log` / `ui-final.json` record 19 passing existing actual-DOM deferred-IPC regressions on current UI handlers. The phase matrix is independently tested in Rust; the UI tests do not claim to execute it.

## First source checkpoint adjudication

All recorded source findings were addressed and the actual corrections/callers were reread:

- Different-window bind resets old seat/count state after successful binding only; same ID/PID preserves it. The regression uses actual binding/reset helpers plus Engine, without a real native window.
- A failed Escape clears only its failed-delivery latch. Faulted remembers the previous Conflict phase; F9 resumes observation of that known incident. A successfully delivered Escape remains latched, and stale ACK cannot own the replacement action.
- Legacy known failures enter `rejected` when reported, before F4 can clear `current`. F9 never promotes those failures to retained successes. For legacy conflicts that survive a handoff, `current` is preserved and the special legacy route is checked before the newly continuous/optimistic route. The runtime's actual `known_conflict_retry_event` helper is exercised with Engine, alongside queued/failed/delivered Escape controls.
- F8 now checks command ownership again after publish/logging and immediately before `save`. A cancellation received before that boundary produces a cancelled result without saving or reporting a captured point. An already-entered storage transaction is not recalled.
- Existing transient-context recovery resumes the committed Region/Seats checkpoint; it cannot revive the discarded scheduled/entry queue.

`source-review.json` binds the inspected 14 source/test files and logs. The independent core suite reports 10 new any-state cases plus 15 reentrant cases passed; desktop reports 129 passed / 0 failed / 1 timing test ignored. The 19 UI cases passed with unchanged UI hashes. These totals are not a count of independent bug fixes and do not establish actual Worker/OS/site behavior.

This first checkpoint was superseded by the blocking Stop → F9 case below. Its test totals and source hashes remain historical evidence, not release approval. Package inspection remains pending.


## Release blocker found after the first checkpoint

A known seat-conflict dialog can remain open while F4 stops input. The checkpoint runtime unconditionally discarded `RestartContext` and reset `VisualRecovery`, while core Stop cleared the current legacy candidate. F9 then resumed Region/Seats without evidence that the known dialog had closed. The missing recovery history is confirmed by the actual source path; a resulting real-site misclick has not been demonstrated.

The preliminary full build passed but is not a release candidate. Root preserved it under `verification/release-0.5.11/preliminary-stop-conflict/`. The agreed correction is to keep only a known unresolved conflict checkpoint/baseline across Stop, cancel queued Esc through the normal cancellation path first, preserve an already delivered Esc latch, and restore the checkpoint only on explicit F9. Repeated Stop and same/failed Bind must not erase that provenance; new starts and successful changed-window binding keep their own reset rules. The correction and its callers have now been independently reread; details follow.

## Follow-up source adjudication

The actual Stop handler now cancels queued input through `cancel_unposted_actions` before preserving a known-conflict `RestartContext`. Core Stop still stops all execution. Only a detected visual incident keeps its old baseline; an ordinary speculative watch is discarded. Repeated F4 preserves the known checkpoint. Explicit F9 restores it through the existing supersede path and observes the latest image before the retry helper. The six legacy/visual × queued/failed/delivered Escape cases preserve failed-seat attribution, retry only undelivered Escape, and leave a delivered Escape latched. A popup closed manually while stopped advances on restored pixels without an unnecessary Escape.

Review also caught native capture sequence reset on successful same-window Bind: macOS and Windows restart their sequence at zero, whereas a retained visual incident previously required a sequence beyond its old stream. `same_target_stream_restarted` now resets only visual stream ordering. It retains baseline pixels, the attributed attempt, and original baseline sequence in reports; native/count metadata remain untouched. The old-100/101 → new-1 popup/new-2 restored regression verifies the new boundary. It is called only after successful same-ID/PID binding.

Bind errors before native commit do not discard this history. **A driver-level bind failure can clear `NativeAdapter.target`; the next binding is then treated as a new context and old history resets.** This conservative reset does not prove the old popup closed. Do not describe all failed-native-rebind paths as history-preserving. New F1 and successful changed-window Bind retain their reset semantics.

The follow-up desktop suite reports **132 passed / 0 failed / 1 timing test ignored**. `source-final-review.json` records current source hashes and the unchanged UI19 evidence. UI `visualDialogView` rejects Stopped phase, so retained incident data do not present active watching while stopped. This remains helper/Engine/VisualRecovery and source-caller evidence, not live Worker/native execution. Root's final full build and artifact inspection are still pending.
