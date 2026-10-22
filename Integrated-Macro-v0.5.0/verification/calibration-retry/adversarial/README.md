# Calibration retry adversarial review

## Initial findings

The hard-hold message is issued by `Worker::start_guarded` for an F2/F5/UI restart with retained history, not the direct F9 handler. Its single message conflates target ID, geometry availability/equality, effective ROI/exclusions/component limits, seat color, tolerance, count/shape policies, and cancellation navigation. The message alone cannot identify which comparison failed.

`supersede_execution` snapshots the old Engine before the next start payload is saved. `save` can replace the current Engine with an Idle Engine containing the new calibration while leaving `restart_context` unchanged. A failed start then leaves that old context in place; a later start from Idle does not replace it. Consequently valid saved/F8 calibration can remain trapped behind the old comparison. Root independently checked the user's repeated-error log and current calibration; this reviewer has not executed the live Worker.

A separate mode boundary is F2 with an active zone override → F5: old effective zone ROI/color are compared with the new global settings. Different settings are valid in both modes, but the initial guard treats that as incompatible reuse. This is distinct from the user's current non-overridden regions.

Actual pure UI `toForm`/`fromForm` on a normal synthetic per-zone profile preserves the compared fields. A contradictory imported profile containing arrow-key navigation plus desktop automatic refresh is normalized to the displayed automatic mode and `navigation:none`; this is an input-compatibility corner, not evidence for the user's reported configuration. No files were created or modified by that pure-function check.

## Approved correction boundaries

Keep same-window and capture-geometry evidence and count/shape/navigation policy guards. Permit valid ROI, exclusions, component-limit, color and tolerance corrections without discarding original delivered/rejected marks. Rotate candidate/frame/count authorization and require fresh observation; preserve delivery receipts and an outstanding count attempt. A smaller ROI excluding a retained seat must not subtract it from the retained count.

Stop after a failed start/save must preserve old retained history even when the current Engine is Idle. Preserve visual baseline only for a matching known unresolved conflict, never merely because delivered seats exist. Repeated F4 must remain stopped; explicit F9/start owns resumption. New F1 and successful changed-window Bind keep their reset semantics.

Original footprint matching is an image heuristic, not server seat identity. Changing a color threshold can reveal disjoint fragments outside the original footprint; permitting calibration does not establish universal duplicate prevention. Deleting all history would increase re-click risk and is not the proposed correction.

## Caller review and new challenge

The first correction uses distinct `RestartBlockReason` variants and reports the actual UI names “NOL 선택 좌석 수 확인” / “좌석 외곽으로 구분” with previous/current enabled states. Window, missing geometry, changed geometry, and legacy navigation have separate reasons. The navigation comparison remains scoped to entering Cancellation with a previously active legacy region; F9 keeps the current-region handoff and does not impose a new blanket navigation restriction.

The independent eight-test source exercises production Engine, comparison helpers, and VisualRecovery. It deliberately supplies synthetic candidates rather than claiming to run extraction/Worker/native I/O. Its fresh aliases, retained A outside the newly narrowed ROI, pending count authorization, rejected-seat marks and non-delivered cancellation cover meaningful boundaries. A navigation fixture initially omitted the actual legacy RefreshCompleted progression; this is a fixture setup failure and must not be reported as a production regression or a final passing result.

A further caller challenge was sent to implementation/root: after old retained geometry differs from newly calibrated geometry, F9 validation clears `restart_context` even on failure and leaves a Paused Engine with old marks. A subsequent F9 can capture a new checkpoint using `desktop.capture_reference` when `frame` is absent, assigning the new geometry to old marks. The proposed calibration allowance must not accidentally turn this repeated-command path into authorization for a different pixel coordinate system. This is source-derived pending reproduction/correction, not a claim of live-site input.

The final implementation preserves original physical provenance for retained history in the same logical run, retains a failed F9 checkpoint, and reuses physical validation for F9, recovery, and explicit Resume. The repeated-F9 relabeling challenge is closed by these actual callers and independent regression 9. Unknown original geometry stays unknown; merely saving a new capture reference cannot upgrade it. Only target/geometry provenance is inherited: the current Engine and latest receipt remain current.

That review exposed a second policy-boundary challenge: old count OFF → a failed F5 save with count ON → F9 restores the old Engine then rejects the policy mismatch, leaving it Paused → Resume checks only physical geometry and can resume the old OFF Engine while Worker.profile remains ON. The same path applies to the object policy. The final Resume caller uses the complete restart guard, closing this policy bypass; a failed check retains the checkpoint. Resume checks control epoch after capture, and ResumeFromUi checks after activation before entering Resume. A no-op Resume on a saved Idle Engine cannot erase the old checkpoint. This correction was reviewed in the actual caller and existing policy-helper controls; no extra test that merely copied the caller was counted as Worker execution. Existing input-epoch guards and control queue draining mean the earlier source race was not proof of delivered physical input.

## Separate completed-run limitation

With count verification OFF, F2/F5/F9 intentionally preserve delivered/selected history. If the target was already met, a fresh candidate observation may authorize only another completion click. That observation does not prove selections are still present after manual deselection, expiry, or navigating to a different performance in the same native window. A successful F1/Normal start creates a new Engine, and a successful changed-window Bind resets context; there is no dedicated current-seat-screen “forget history” control. F1 executes its configured entry/CAPTCHA path, so it must not be presented as an effortless F5 reset workaround. The ROI guard correction does not claim to solve semantic scene identity or stale completion history.

## Final source review

No additional blocking defect was found in the scoped calibration correction at the source hashes in `source-reviewed.json`. Actual reviewed paths include `save`, `supersede_execution`, `capture_restart_context`, F2/F5 `start_guarded`, F9 handoff, F4 checkpoint selection, Resume/ResumeFromUi, automatic target recovery, and changed/same Bind. Same-run original footprints and receipt/count obligations survive calibration; stale candidate/count permits are discarded. Stop still cancels queued unposted input first. Existing known-conflict provenance and successful-Esc latch remain preserved, and ordinary speculative watches reset.

The hard-policy messages use the real UI labels and previous/current on/off values. New F1 and successful changed-window Bind still clear the old logical context. Source diffs leave the Engine executable policy, image recognition, UI configuration mapping, platform code and released packages unchanged; the Engine edit is a corrected documentation comment.

Read verification evidence: implementation desktop suite 142 passed / 0 failed / 1 manual timing ignored; independent targeted suite 9 passed / 0 failed. The original calibration/Stop defects have before-failure evidence; the physical-provenance and Resume challenges are identified separately as caller-derived counterexamples, with shared-helper/Engine regressions where applicable. Earlier navigation and paused-request test-setup errors remain visible in their logs and are not production failures. Parent is running the final whole-source JS/Rust/native gate without a distribution build; do not substitute these focused counts for that final result.

This reviewer performed no production edit, app launch, native input, user-settings write, or previous-release modification. The reviewed fixes are unreleased source. Actual Worker plus native capture/input, Windows runtime, and an authenticated booking flow were not exercised by these tests. The separate completed-run and semantic scene/seat identity limitations above remain.

