# Runtime audit, round 1

## Reproduced pre-input frame defect

`probe.rs` preserves the pre-fix `frame_passes_transition` helper extracted from production runtime source and links the actual `macro_core::Engine`. `results.txt` records two passing **counterexample** tests: an unread frame with sequence N+1, captured before the region or refresh click, passes the old sequence-only fence and leads to a seat action. Passing these tests confirms the old defect; it is not a current-code success test.

No OS events were sent. Native capture order, browser rendering, and ticket selection were not exercised. The permitted native ordering was established separately by source review. The initial probe linked `target/debug/deps/libmacro_core-363b1bb7f35392ab.rlib`; that build hash is historical and may change after rebuilding.

## Corrections and current regressions

- Region/refresh transitions require both a newer frame sequence and `captured_at_ns > InputReceipt.started_at_ns`. Native receipt time is taken immediately before the first click/key post, not at Rust function return. A valid static frame captured while the input call is finishing remains eligible.
- Guarded native input checks the captured action epoch and interrupt flag. Proven unsent cancellation emits `ActionCancelled`, preserving retry eligibility; a delivered down/up pair remains acknowledged.
- External full-profile mutations require the current `profileRevision`. F8 replies carry the authoritative whole profile and revision. UI start submits settings and start in one request.
- Dispatch enqueues synchronously before scheduling the blocking reply waiter. UI-start cancellation survives save/activation/capture delays. A cancelled or superseded start reply cannot minimize settings afterward.
- **20-second reply-timeout cancellation is limited to `StartFromUi`.** It invalidates only that epoch and queues scoped cleanup, without writing the shared interrupt flag. Cleanup requires ownership of the original UI run, preserving a newer UI start and any pending or started F1/F2/F5 request sharing the cleanup epoch. This last boundary was verified in the queue/ownership model; real UI timing was not reproduced. `ApplyShortcut` retains its existing request-id/epoch gate; this audit does not claim that the gate cancels an already-consumed shortcut after a long operation or reply timeout.
- Incomplete transparent visual frames are recoverable capture errors. Recovery waits for a valid opaque sample, preserving an existing dialog incident until a valid restoration is observed.

`targeted-summary.json` records the final desktop test command and outcome; `desktop-tests.log` records 73 passing tests. Tests use real routing helpers, channel enqueueing, pixel validation, and the core engine; they are not a full Worker/AppKit integration test. No application package, live input, login session, or personal profile was modified.

## Remaining limits

Post time plus capture callback time excludes a known pre-input capture; it does not prove the website has rendered its next stage. Callback timing is not a browser acknowledgement. Windows real execution and macOS installed-app interaction were not performed in this audit. UI `ResumeFromUi`/manual confirmation retain their existing response lifecycle; the new timeout and delayed-window-handoff checks specifically cover UI Start.
