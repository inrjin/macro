# Independent calibration restart review

Status: source and independent tests frozen after the Resume caller followup. No remaining blocking defect was found in this review scope. This is not release authorization or live-site validation.

## Findings and corrections

1. **Filter changes caused an indefinite restart hold.** Original `same_search_mapping` / `same_post_entry_mapping` compared the entire effective VisionConfig and RGB/tolerance with the old RestartContext. Save/F8 could create an Idle Engine with new filters while the retained checkpoint stayed old. The original shared helpers linked to actual Engine fail the two independent baseline assertions in `before.log` (0 passed, 2 failed). The final guard allows ROI, exclusions, component size, color and tolerance changes only within the retained physical coordinate context and unchanged count/object policy; it retains original delivered/rejected footprints and count obligations.
2. **Save then F4 discarded ordinary delivered history.** The original Stop checkpoint helper retained only unresolved conflicts. The final helper prioritizes a known conflict, otherwise retained current/previous selection, and clears speculative visual watching. Stop still cancels unposted actions before capturing a checkpoint. Its actual caller now captures ordinary retained history as well.
3. **Repeated failed F9 could relabel old marks with new geometry.** Actual caller review found unconditional checkpoint clearing after F9 validation failure followed by capture-reference fallback on the next request. `geometry-cycle-before.json` preserves that source trace; it is not a live Worker reproduction. The final `preserve_restart_provenance` retains the prior target and geometry, including unknown geometry, within the same logical run with retained history. F9 clears the checkpoint only on successful validation. Shared physical checks apply to start/F9/recovery; repeated failed attempts cannot upgrade missing or different geometry.
4. **Resume could bypass a failed count/object policy guard.** The adversarial reviewer traced old Engine OFF / saved profile ON after a failed restart. The final actual Resume caller uses `restart_block_reason`, not only physical comparison; it keeps the old checkpoint on errors and on a live-Idle no-op. Native capture and UI activation have control-epoch checks before committing Resume. Save clears automatic recovery, and F9 checks policy before installing recovery, so the retained physical-only automatic recovery branch does not provide the same policy-mismatch bypass in the reviewed ordering.

## Independent regression evidence

`src-tauri/src/calibration_restart_review.rs` contains 9 tests using actual Engine plus production private runtime helpers and VisualRecovery. `after-provenance-current.log` records 9 passed / 0 failed / 0 ignored. The implementer's final desktop suite recompiles the final Resume followup and includes all nine tests: `../implementation/desktop-final.log`, 142 passed / 0 failed / 1 manual timing test ignored. Parent final workspace/native checks will be recorded separately and must not be inferred from these focused results.

The tests cover:

- Six extraction-filter edits across F2/F5 and the F9 compatibility helper.
- Saved Idle plus repeated Stop, preserving original seat/count receipt while resetting speculative popup watching.
- Original footprint alias exclusion, A remaining counted outside a narrowed ROI, and adding only B across Seats/Cancellation/Normal handoffs.
- Outstanding count effects and request rotation, unknown/lagging count holds, and stale request/ACK rejection.
- Failed-seat footprint exclusion after recolor/recalibration, without promoting failure into success.
- Distinct window/missing-or-changed geometry/count/object/navigation guard reasons.
- Unposted cancellation not becoming retained history.
- Three failed F9/re-capture cycles retaining original target/geometry and current Engine/receipt; unknown provenance stays unknown; original-geometry recovery still requires fresh site-count evidence before B. Fresh empty/different logical runs do not inherit old physical provenance.

The ninth test reconstructs the reviewed F9/checkpoint ordering with shared helpers; it does not execute Worker or native capture. Its count values are explicit external observations, not claims of OCR success. Geometry candidates intentionally exercise Engine identity safeguards even when a supplied alias is outside the new ROI; this is not an extractor test.

## Actual caller review

Reviewed the final bodies and affected callers of `stopped_conflict_context`, `preserve_restart_provenance`, `restart_block_reason`, `capture_restart_context`, `supersede_execution`, `start_guarded`, `continue_after_captcha`, `recover_target`, Command Stop/Resume/ResumeFromUi, and Save. Fresh F1 and successful changed-target Bind retain their existing reset behavior. The Engine's existing restart remaps marks in the current active region when moving to Seats, so changing mode does not by itself erase alias protection; its new production diff is documentation only.

There is no claim that a matching native window ID and dimensions prove the same performance/seat map. Manual deselection, changed content in the same window, and optimistic completion after a genuinely new site session remain separate limitations. Count/object policy changes and actual geometry changes remain deliberate holds; the new messages identify their cause. Original image footprint matching remains a heuristic, especially for disjoint fragments beyond the original footprint.

## Test setup corrections and preserved checkpoints

The initial eight-test run had an incorrect legacy-navigation fixture: it omitted RefreshCompleted and then supplied the current rather than strictly newer generation. Corrected setup passed all eight; the intermediate failing/compile logs remain preserved and are not production failures. The first ninth-test run unwrapped a count request while Engine was Paused; storing the pre-pause request ID corrected the test without a production change. `after-provenance.log` records that setup error and `after-provenance-current.log` records the final success.

The baseline standalone harness first had an escaped Unicode include-path compilation error, then used the corrected UTF-8 path and matching cached serde_json dependency to run the two intended failures. `before-result.json` binds the command, linked core library, and original helper source. No old evidence was rewritten.

No application was launched, no OS input or permission changes were made, no user profile was written, and no release package was built by this reviewer. Windows execution and live booking-site behavior were not tested. The reviewed final hashes and evidence links are in `source-reviewed.json`.

## Peer challenge closure

The adversarial reviewer `/root/input_adversarial_review` independently read the final ninth test and the Resume followup. They confirmed the original target/geometry (including None) and current Engine/receipt separation, request/ACK invalidation, and restored-geometry positive control. They agreed that a duplicate tenth helper test would not substitute for actual Resume caller review; the final caller policy and post-capture/activation epoch checks were reviewed directly. No additional scoped blocker was reported.
