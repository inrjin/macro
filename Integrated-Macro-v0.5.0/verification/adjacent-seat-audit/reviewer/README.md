# Seat-count and repeated-candidate review

## Before implementation: released 0.5.9 UI contract

The released 0.5.9 UI labels the choice **목표 좌석**, with `1석` through `4석`. Before this incident's implementation, `ui/model.js::fromForm` mapped it only to `Profile.target_seats`. Neither the UI nor the core `Profile` / `LegacySettings` schema had an adjacency, row, seat-spacing, or consecutive-seat option. The pre-change `README.md` explicitly said adjacency was not implemented.

The historical distribution's Korean usage file (`../통합 매크로/사용법.txt`, decomposed filename on disk) describes its 1/2/3/4 choice as a seat count and says: “연석이 아니고 자리의갯수입니다.” This is evidence from the supplied documentation, not a decompilation or execution of the historical executable.

`ui-count-probe.mjs` executed the pre-change HTML, app listeners and model using the repository's minimal DOM fixture. The IPC response was stubbed. All 12 combinations of count 2/3, normal/cancellation/direct-seat mode and button/shortcut start sent the requested count. Changing 2 to 3 changed only `target_seats` in those profiles. Results in `ui-count-results.json` are preserved as before-implementation evidence; rerunning this source-loading probe now reads the subsequently modified source and is not a historical replay.

Run with:

```sh
node --experimental-vm-modules verification/adjacent-seat-audit/reviewer/ui-count-probe.mjs
```

## New optional priority: UI verification

The user subsequently requested same-row adjacent candidates first, with separate seats as fallback. The new **연석 우선** option is independent of **목표 좌석** and explicitly describes geometry-based estimation rather than reading seat numbers.

`ui-adjacent-options-probe.mjs` executes the modified actual HTML/app/model. It checks 24 combinations of count 2/3, option ON/OFF, three modes and button/shortcut start. Each starts from the opposite persisted option to verify that the current form is transmitted. All requests carry the intended `features.adjacent_seats` and `target_seats`. Default-OFF, older-profile-OFF, imported option and form round trips also pass. Results are in `ui-adjacent-options-results.json`. This is an IPC-payload test, not an Engine, browser or native-input test.

## Existing coverage gap

The extractor names a component using its minimum x/y, while the Engine excludes previously attempted/selected/rejected candidates by that string ID. A changed matching-color shape can therefore represent the same physical seat with a new ID. The existing P6 site oracle recolors a selected seat entirely gray; it cannot expose a same-color remainder, inset boundary, or split component. Static component-size/ROI tests and hand-authored stable Engine IDs likewise do not cover this temporal identity problem.

The Engine owner's new `engine/component_identity.rs` was independently reviewed. Real extraction and Engine calls demonstrate repeated counting at an identical click point after the component ID changes. Stable-ID and duplicate/cancelled-ACK controls separate this from duplicate acknowledgement handling. Its independent rectangle hit-test toggles site state without supplying truth to the Engine. The image inset is a scripted rendering sequence, however, rather than a renderer derived from the site's selected Boolean; this does not establish the real site's selection styling. The two-seat fixture ends with zero selected, and the three-seat fixture with one. A distinct real-world click location cannot be explained by this same-point fixture alone.

The vision owner's five tests in `vision/src/lib.rs` were also independently reviewed. They invoke the production extractor and `OwnedFrame`/RGB helpers; the runtime's private helper and full Worker are not executed. Their integer-coordinate sampling is equivalent to the reviewed macOS helper branch. The 1×3 residual-color fixture generates `pixel:122:919` at `(122,920)` and passes color validation, while removing that color makes the old target fail. Matching numeric log settings and position does not reconstruct the missing real screenshot. The unchanged-boundary control, split-component case, close small-neighbor control and hollow-seat counterexample support the reported scope. No additional blocker was found in these probes; their runs were performed by the owners, not repeated by this reviewer.

## Constraints on a possible correction

- Changing IDs to click coordinates alone does not cover a checkmark splitting one seat into components whose centers move.
- A fixed exclusion radius can suppress a legitimate tiny neighbor. Include tightly spaced true-seat controls if introducing spatial deduplication.
- An original-component footprint can be narrower than a fixed-radius exclusion, but a bounding rectangle can still cover multiple touching or overlapping objects. Include those controls and register exclusions only after actual delivery, not on a cancelled proposal.
- Clearing all clicked-seat exclusions on refresh can re-click retained selections; keeping all failures forever prevents the requested retry after refresh. Those lifecycles need separate rules.
- Clicking a fixed distance to the right is not evidence of an available adjacent seat. A future adjacency feature needs inactive gaps, aisles, curved rows, scaling and partial-selection cases, and is separate from fixing repeated counting.
- Matching opaque pixels in an inactive seat can pass a color-only check. Shape thresholds may reduce some false positives but cannot certify site availability.

No production source, installed application, user configuration, OS input, or release artifact was modified by this review.
