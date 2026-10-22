# Optional NOL count gate: implementation and cross-review

## Contract

`nol_selection_count` and `seat_objects` default to false and deserialize from older profiles without migration. `seat_objects` does not change engine policy directly. The count path uses `SelectionCountRequest` and `SelectionCountObserved` / `SelectionCountUnavailable`; it never converts displayed seat numbers into image candidate IDs.

The engine first accepts the site's total selected count, including preselected seats. It then permits at most one outstanding seat delivery. Delivery alone cannot release another seat or completion. Exactly the expected count increment resolves that delivery. Counts above target, decreases, jumps, unreadable text, and unchanged counts keep observation running without a corrective click or timeout-based success/failure. Initial count equal to target may complete without any seat input. The completion action requires a matching count observation in that event step; elapsed delay alone cannot authorize it.

Requests rotate on input acknowledgement, cancellation, pause, refresh, and new-run boundaries. Action-correlated known conflicts update the expected count and preserve unrelated attempts. Partial selected count blocks automatic refresh and legacy region changes. The optional path preserves the original entry/CAPTCHA sequence and the OFF behavior.

## Objections and resolutions

- Count decrease does not establish which click toggled which seat. The proposed same-point corrective click was rejected; no such automatic restoration exists.
- Reader failure after an earlier accepted count could be hidden by a successful vision scan. `SelectionCountUnavailable` now clears verification, preserves the last known number, and exposes pending status. Old request IDs and duplicate observation IDs cannot clear or authorize state.
- Requiring a newer native SCK timestamp for every post-delay read can deadlock a static counter. Runtime uses a successful-poll serial as engine observation ID, separately retaining run/action/native-input timestamp checks. The serial does not claim that pixels changed. Strict request-creation timestamp advancement was rejected for the same static-screen reason.
- Completed runs must not continue displaying a pending count poll after the completion ACK. The snapshot pending flag is scoped to count-relevant active/paused phases.

## Independent boundaries inspected

`seat_objects.rs` learns a repeated round family, checks perimeter/background and available fill, and returns full proposed geometry. Its pre-click check rejects a center-color patch when the complete object disappeared. Existing geometric marks consume the full bounds and preserve the original click point. Synthetic bridge/split, close-neighbor, selected-halo and independent object-hit/count tests are source-consistent; they do not establish semantic identity for arbitrary artwork.

Runtime observes the count in Seats and Completing, validates native window/capture state, matches the request and latest native action, and preserves the native timestamp requirement after input. The explicit Mac blank/suspended/stopped/error states invalidate the cached frame. Unknown readings reach the new engine event. Paused profile editing cannot toggle these features mid-run. The count reader's matching algorithm and template tests are independently reviewed by the separate reader reviewer.

## Limits

The engine tests supply count evidence directly; they do not execute the text reader, browser, OS capture, or native input. Object proposals and total count cannot prove a seat number or attribute unrelated manual/server changes. An unresolved delivery or count mismatch may remain observation-only indefinitely; this is a safety hold, not a demonstrated progress guarantee. Static native capture polling does not prove that an unreported OS stream stall is impossible. No release build or live-site operation was performed. Final source hashes and focused test results are recorded beside this file; parent workspace checks cover integration.

## Final display scope correction

The empty-candidate message now says no confirmed round seat proposals when `seat_objects` is enabled. The feature comment explicitly says repeated round outlines. The OFF message and every state transition remain unchanged. Searches found no test assertions requiring the old message. No new mirror test was added; the parent final workspace suite covers the final source. Updated hashes are in `source-final.json`.

### Historical workspace result

The earlier workspace run passed 364 tests with 0 failures and 2 ignored manual checks. Its log is preserved as `../workspace-before-manual-routing.log`. It covered the round-only message/comment correction, but preceded the later manual-routing flag and context-recovery caller corrections; it is not the final integration result. The independent reviewer also inspected the round-only message/comment and found no state/selection-policy change.

## Manual action routing follow-up

Parent identified a count-ON/non-continuous manual action mismatch after the earlier workspace run. `manual-routing-repro/` preserves four exact-helper/actual-Engine reproductions. The final model adds default-false `Snapshot.selection_count_verification`; engine.snapshot exposes its feature value without changing automatic policy. Corrected runtime/UI callers are independently reviewed and `manual-routing-repro/corrected/` passes every original counterexample and the OFF/entry-step controls. The final integration result below includes this API and its corrected callers; updated hashes are in `source-final.json`.


## Final context recovery and integration result

The follow-up context helper adds automatic recovery for count-verification runs only in Seats, Conflict and Completing. The original continuous policy is preserved; non-continuous Entry/CAPTCHA/Region/WaitingRegion/WaitingRefresh and feature OFF are unchanged. Both actual callers (`pause_for_context_at` and the recoverable native-error branch) were independently inspected for eligibility before cancellation, proof/cache invalidation, known non-delivery handling, and fresh-count requirements after Resume. `context-recovery-review.md` and `context-recovery-source.json` record the review. The regression executes the production helper with the actual Engine's cancel/context/resume transitions; it does not execute a whole Worker, Tauri application or OS recovery path.

Final Rust workspace validation: **367 passed, 0 failed, 2 ignored** (`../workspace-final.log`). Final JavaScript validation: **150 passed, 0 failed, 0 skipped** (`../javascript-final.log`). The final source check confirmed that all **122 files** in `../source-before-final-test.json` and all **54 files** in `../javascript-tested-source.json` still match their recorded SHA256 values. The older 65-file `rust-source-before-final-test.json` precedes the manual/context corrections and is historical only. `source-final.json` preserves this component's final hashes and links both final logs.

These results include the last manual-routing, UI-caller and context-recovery corrections. They remain source/component/rendered-fixture checks: actual packaged application use, physical OS input, a live booking site, and Windows execution are unverified. No release build was performed.
