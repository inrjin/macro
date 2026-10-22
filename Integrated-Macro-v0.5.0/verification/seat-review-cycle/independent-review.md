# Independent seat-code review

Reviewer: `/root/input_adversarial_review`. Scope: production core logic and affected runtime cache callers, independent synthetic-image/Engine regressions. No application, browser, operating-system input, user configuration, or release build is exercised here.

## Findings and disposition

### R1: unrelated small candidate splits a valid adjacent pair — reproduced and corrected

- Location: `crates/macro-core/src/seat_selection.rs`, `preferred_candidate`, row construction using `rows.last_mut()`.
- Trigger: 10×10 candidates at `(10,20)` and `(22,23)` satisfy the current same-row tolerance and 2-pixel horizontal gap. A distant 2×2 candidate at `(100,25)` falls between their vertical centers. Checking only the latest row assigns the original pair to separate rows.
- Consequence: with adjacency enabled, an earlier isolated seat wins even though the pair still exists. Without the distant object, the pair wins.
- Evidence: actual production seat extraction from synthetic RGBA followed by actual Engine in `crates/macro-core/tests/seat_review_regressions.rs::distant_small_component_cannot_break_an_existing_same_row_pair`; `independent-before.log` records 5 passing controls and this failure (`pixel:0:0` instead of `pixel:10:20`).
- Correction: search previous compatible rows while preserving the first/last vertical-extreme checks. The backward search stops once the first center is more than `0.35 * current height` above the current center; older rows cannot satisfy the required first-center comparison. A new staircase-versus-real-triple control covers excessive row bridging. Independent tests passed after this correction; the final retained-source run is recorded below.

### R2: original rectangle includes another object's empty-space position — reproduced, unresolved

- Location: `seat_selection.rs::same_seat`, prior containment check before overlap ratio; native extraction kept only rectangle/count metadata.
- Trigger: separate matching components in the opening/interior of a C/U/ring-like component.
- Consequence: delivering the surrounding candidate can suppress a different component although their matching pixels never overlap.
- Evidence responsibility: vision reviewer owns the actual extractor/Engine/object-oracle reproduction; this reviewer inspected the original branch and the proposed replacement.
- Experimental correction reviewed: retain immutable matching-pixel row spans only for seat extraction, share through `Arc`, and compare actual overlap when both masks exist. This fixes the disjoint-mask example but introduces R4 below. The implementation is therefore **not adopted**. Source and hashes are preserved under `mask-experiment/`; the original bounding-box limitation remains in production.

### R3: candidate cache retains historical IDs and replaces in-flight point — root reproduction reviewed

- Location: `src-tauri/src/runtime.rs::observe_frame` and new `update_candidate_cache`.
- Trigger: long cancellation searches produce changing component IDs; alternatively, an in-flight confirmed-mode candidate retains its ID but repaints to a different centroid.
- Consequence before correction: historical masks would accumulate; selected-color sampling could follow the repaint centroid instead of the actual click point.
- Correction inspected: retain current observed candidates plus at most the original in-flight candidate. Preserve the original when the same ID is observed again, and release it on the next observation after `current_candidate_id` clears.
- Evidence: root's `candidate_cache_*` regressions, `runtime-cache-before.log` and `runtime-cache-after.log`. Source inspection confirms the cache has one lookup consumer: current-candidate selected-color sampling before the next extraction/cache update. These are production-helper tests, not full Worker/OS integration.

### Rejected hypothesis: identical pixel IDs necessarily block another legacy region

Pure extractor-to-Engine injection using bare `pixel:x:y` IDs can retain a rejection at the same coordinate across regions. This is not evidence of that defect in the application: `runtime.rs::observe_frame` prefixes observed IDs with `zone{index}:` before passing them to Engine. Root supplied this counter-evidence; the reviewer verified the caller. No production change is requested for this hypothesis.

### R4: mask-only overlap removes existing repaint protection — new regression; experiment rejected

- Location: experimental `seat_selection.rs::same_seat`, early return from the two-mask overlap branch.
- Trigger: a selected thick ring loses its original matching color and displays a matching 2×2 marker inside the formerly empty center. The two matching masks have zero overlap.
- Baseline: the original rectangle contains the new marker's click point, so duplicate protection blocks a second click.
- Experimental consequence: zero mask overlap treats the marker as a new candidate. The external site oracle observes a second click toggling the same object off while delivered-click counting proposes completion. This is **a newly introduced regression**, not merely an old unsupported case.
- Review correction: this reviewer initially grouped the case with residual semantic limits. Root challenged that classification against the baseline; the reviewer accepted the counter-evidence. A passing test that asserts the bad outcome documents the regression and cannot establish readiness.
- Decision: preserve the experiment and its evidence, remove the production mask change, and retain only R1/R3 corrections. Protecting pre-existing component labels is not an adequate replacement: one initially patterned seat may already consist of a ring and separate inner component. That alternative adds state without proving separate seat identity.
- Performance evidence: synthetic debug timings in `vision-timing-before.log` and `vision-timing-after.log` show additional extraction cost for 1,500 small candidates, a 65,536-pixel component, and a thin connected comb (approximately 31%, 102%, and 104% in these runs). These are not release/app latency measurements, but they further invalidate a claim of no added cost.

## Compatibility and state checks

The independent test file also covers retaining the original delivered footprint across pause/refresh, ignoring old-generation observations, extending a retained middle seat from its neighbor rather than the far end, old JSON without geometry, exact-target alias rejection without geometry, omitted adjacency defaulting off, and keeping candidate JSON limited to geometry metadata. The initial six-case run is recorded in `independent-before.log`. `independent-after.log` records seven passing cases while the mask experiment was still present; it is not the final retained-source result.

Experimental mask tests received a further adversarial correction: the original 256-case cross-ROI set oracle had 256 negative expectations. The reviewer independently computed that distribution and requested positive multirow cases. The implementer added equivalent absolute pixels encoded through different ROI origins and a two-row 11/16 versus 12/16 overlap threshold control. The corrected test passed, but this coverage does not negate R4 and is preserved as experimental evidence only.

The current mark lifecycle stores evidence after delivery (or supported positive legacy results), retains selected/attempted marks, prunes failed marks when retry/refresh clears rejections, scopes marks to the active region, and clears them for a new run/post-CAPTCHA restart/coordinate invalidation. There is no arbitrary distance exclusion. Adjacency row/gap heuristics affect ranking, with separated-seat fallback.

## Limits that must remain explicit

- Original color masks are not complete semantic seat boundaries. Initial text/white strokes can split one seat into multiple components; different component labels must not guarantee different seats.
- A changed selection marker can appear entirely outside the original matching pixels. Zero-overlap changes are not universally solved by retained masks.
- Same-color edge-touching seats may already form one component. The same binary mask does not supply a missing boundary.
- The 2×2 support check rejects thin/one-pixel shapes and cannot prove availability. It is separate from identity and adjacency.
- Delivered-click counts remain estimates. No synthetic case establishes site acceptance, retained selection, real-world speed, or successful booking.
- The rejected mask experiment adds sorting/allocation and span traversal. `Arc` avoids cloning the full mask but does not eliminate its measured synthetic extraction cost.

## Final verification

The final production delta reviewed against `before/` consists of the compatible-row search in `seat_selection.rs` and current/in-flight candidate-cache maintenance in `runtime.rs`. `model.rs`, `vision.rs`, and `engine.rs` are byte-identical to the preserved baseline. The mask experiment is excluded.

`cargo test -p macro-core --test seat_review_regressions -- --nocapture` was rerun after the production mask rollback: **7 passed**, recorded in `independent-final.log`. This includes the originally failing unrelated-component row case and the non-bridging staircase control. The test name now says "original footprint" rather than suggesting production retains a pixel mask.

The final `seat_shape_regressions.rs` was read again: five supported-behavior regressions remain, including a ring repaint that must receive only one click, retain one oracle selection, and avoid completion. Experimental assertions that reproduced bad outcomes are preserved separately. Its site oracle supplies immediate, correctly attributed conflict outcomes and dialog closure; those scenarios do not test the actual visual detector's latency/attribution or real Esc behavior. The final runtime cache test uses a real 5×5→8×5 repaint extracted into the same ID with a different point; the retained-point invariant remains after experimental Arc assertions were removed.

`independent-source-final.json` records SHA-256 values for the reviewed source, test files, and the three independent test logs. Key final hashes:

- `crates/macro-core/src/seat_selection.rs`: `7248517b7f3065ccb78d9a640d15ce62f19730ecbc5013e7fe5cf85d641aec1a`
- `src-tauri/src/runtime.rs`: `eacdcd9a99f3cc1051a85e42db8227071d670b2ca3b1a75ec54e6d9d32203481`
- `crates/macro-core/tests/seat_review_regressions.rs`: `14782d4415fd96e4ab0c06961cdf8b4b5b1059714f68eb1f6123710162d2402f`

No additional blocking defect was found in these **two retained corrections**. R2 and semantic seat-identity/selection-count limitations remain unresolved; this review does not approve a claim that seat recognition or booking now works universally. Root performs the final whole-workspace regression run. No live site, OS input, or distribution build was validated by this reviewer. Test totals and agreement are not the acceptance criterion: the retained corrections close their demonstrated failures, and the regressing experiment was rejected.
