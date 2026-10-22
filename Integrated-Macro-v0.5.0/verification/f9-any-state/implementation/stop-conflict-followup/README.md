# Stop → F9 unresolved-conflict follow-up

This follow-up supersedes the earlier readiness checkpoint for this specific boundary. The earlier source/hash/log files remain unchanged. The parent preserved the successful but undistributed preliminary build under `verification/release-0.5.11/preliminary-stop-conflict/`; it is not the final release candidate.

## Defect and minimal correction

The actual Stop caller previously cleared all queued actions and reset VisualRecovery before Engine Stop. Stop also clears the core current candidate. Therefore a known but unresolved conflict could lose its visual baseline/checkpoint, and F9 could return to Region/Seats without evidence that the dialog had closed. Simply preserving a checkpoint after `actions.clear()` was insufficient: an unsent queued Esc still had a requested-delivery latch.

Core Stop semantics are unchanged. The only new core API is `has_unresolved_conflict()`, exposing the existing restart checkpoint predicate. Runtime Stop now calls the actual shared `cancel_unposted_actions` first, then retains only a known-conflict RestartContext. Repeated Stop keeps that existing context. It preserves VisualRecovery only for a previously detected incident; an ordinary armed seat watch is discarded. Stopped performs no input or observation. F9 restores the existing context through its original supersede path and validates/captures before resuming. The normal legacy/direct-confirmation path remains direct confirmation; no visual baseline is invented for it.

Unsent or failed Esc is rearmed after fresh observation through the existing caller. Successfully acknowledged Esc stays latched until the known dialog closes. If the popup was manually closed while stopped, the actual baseline comparison produces Restored before the retry caller, so another Esc is unnecessary. Failed-seat identity stays rejected and another actual candidate can proceed after closure.

## Same-target binding and capture sequence

Bind no longer deletes the checkpoint before native binding can succeed. A successful changed window ID/PID resets engine/history/context/visual evidence as before. A successful same-ID/PID bind preserves the known incident but starts a new capture stream. Native drivers restart their frame sequence at this boundary; retaining the old sequence gate would reject new low-numbered frames indefinitely on a static stream.

`VisualRecovery::same_target_stream_restarted` resets only its new-stream observation ordering. Comparison uses a temporary baseline clone with the stream origin; stored baseline pixels, original baseline sequence, attributed attempt and incident are retained. Report baseline sequence still identifies the original stream while current sequence identifies the new stream; they need not be numerically increasing across streams. Native/count frame metadata is not changed. Nonincident watches reset. The method is invoked only after successful same-target binding.

A native validation failure retains the adapter target and checkpoint. A later driver-bind failure can leave `NativeAdapter.target=None`; the next successful connection is conservatively treated as a new context and clears old history. We do not claim identity preservation through that untested native failure/reconnection sequence. No last-known-target inference or new native state was added.

## Verification and source binding

- `desktop-first.log`: 7 focused F9 helper/Engine tests passed before the same-bind stream follow-up.
- `desktop-final.log`: final full desktop suite **132 passed, 0 failed, 1 ignored timing test**. Eight F9 tests include the actual production helpers and Engine, plus real VisualRecovery sample/compare logic.
- New Stop matrix: visual/direct-confirmation × queued/failed/delivered Esc (6 combinations), repeated F4, stale ACK/Tick inactivity, rejection of the failed seat, and next-candidate continuation. Additional controls discard speculative watches, handle a manually restored popup before Esc, and verify original baseline sequence100 against new stream sequence1/2.
- The tests use shared production helpers in the same order as the actual caller, not a live Worker/adapter. The successful/failed Bind call ordering and fresh-F1/changed-target reset branches were additionally read by separate independent and adversarial reviewers. Native driver failures, physical keys, actual OS capture and live-site behavior were not exercised here.
- `source-before.json`, `before/`, `implementation.diff`, and `source-final.json` bind the four changed source/test files. The visual-recovery before text was reconstructed by removing only this patch and its bytes were verified against the parent's preliminary-build SHA-256. Other before files were copied before editing.
- Parent final workspace/native/build checks are separate. Earlier 129-test and core25 logs belong to the previous checkpoint and are not represented as runs of this follow-up source.
