# Independent selection-count reader review

Status: final source regression review complete within the documented image/helper scope. The initial failures below are retained as historical evidence. Final parent execution passed 367 Rust tests with 0 failures and 2 intentionally ignored manual checks, plus150 JavaScript tests with 0 failures; it includes all 5 independent reader tests and 108 rendered fixtures. `final-summary.json` binds the tested source and evidence. This does not establish complete live-site support. No app, live site, OS input, or distribution build was executed.

## Reproduction and independence

`generate-fixtures.swift` renders complete strings with CoreText into RGBA8, using fonts, sizes, positions and colors specified in `manifest.json`. It never reads production templates. The renderer records resolved font names; all requested fonts resolved without substitution in this environment. It writes only its own fixture directory. PNG spot checks verified heading placement, the seat-list panel and a genuinely clipped digit.

Run from the project directory:

```sh
xcrun swift verification/seat-object-system/reader-review/generate-fixtures.swift verification/seat-object-system/reader-review
# Configure the repository's local Rust environment first.
cargo test -p macro-core --test selection_count_reader_review -- --nocapture --test-threads=1
```

The tests invoke the actual `read_selection_count`. Required positives must produce the correct number; optional font/ink variants may reject, but may never produce a different number. Negative/ambiguous/malformed cases must reject. Recognition and rejection are counted separately to prevent an all-Unreadable implementation from passing.

The initial suite had 72 cases. Its four position cases used digit3, which itself failed at the default location; those failures could not establish a position defect. The revised 77-case suite adds digit2 at the same positions and a digit2 contradiction. `position-control-summary.json` binds the revised evidence and captured source/assets hashes. Earlier logs are retained and must not be confused with a final passing run.

## First-source findings

| Condition | Evidence | Consequence |
|---|---|---|
| Arial Bold24 digit3 | Heading score .819 passes; numeral best3 .72897, runner8 .66009 | Numeral fails .79 threshold and .07 margin; no seat input may be authorized by a count guard. |
| Heading size19 | Actual ink bounds (32,30,69×18) score .78620; anchor search best proposal (31,30,71×18) scores .64189 | Proposal geometry misses an otherwise acceptable match. Lowering the global threshold is not a justified fix. |
| Explicit empty message | NeoRegular22 gray text best score at true bounds .61428; anchors fail too | Initial zero is unavailable. Missing numeral must not be silently converted to zero. |
| Top-left digit2 control | Heading is found at (3,4,87×22); required padding4 exceeds left margin3 | Rejects an edge placement. Bottom-left, bottom-right and middle digit2 controls succeed. |
| Header2 plus explicit empty | Returns Confirmed2 | Contradictory state passes; first digit3 version masked this defect because digit3 already failed. |
| Header3 plus three row labels/timer | Header found; same digit3 score failure | Layout is not established as the cause. |

The revised run has 19 required positives: 10 correct, 9 unreadable. Of 37 optional variants, 9 are correct and 28 unreadable. Of 21 rejection cases, 20 reject and one contradiction wrongly confirms2. No observed wrong numeric substitution in this finite set is a universal accuracy guarantee.

## Diagnostic source copy

`probe/src/selection_count_snapshot.rs` is a source snapshot (module-doc markers converted to ordinary comments for include); original SHA values are in `probe/source-snapshot.json`. Its copied template asset is frozen. The diagnostic wrapper reuses private matcher/scorer functions and records proposal geometry and scores. `diagnosticOracleRegions` uses the renderer's text rectangle only to distinguish proposal failure from scoring failure. Those oracle rectangles are **not** passed into production recognition and do not prove a coordinate-independent solution.

## Timing and remaining validation

Recorded per-call times are debug execution on roughly 640×300 synthetic frames, excluding capture, runtime scheduling, browser rendering and native input. They are not app end-to-end latency. The parent separately reported a provided 3024×1608 screenshot reading3 in roughly1017ms debug. Its heading was included in template generation, so that heading is a reproduction control; its digit3 was not included. Neither one screenshot nor synthetic typography proves Windows browser-font coverage.

At this initial stage, the next corrections required rereview against fixed negatives and new held-out glyph/scale inputs. Final source hashes and optimized reader latency are recorded separately below and in the parent artifacts. The count guard cannot recover every ambiguous object or site rejection simply by continuing to observe; actual progress and retained-seat behavior remain distinct from preventing premature completion.

## Template expansion and held-out review

The parent expanded assets to raster sizes18/24/32/40 and added Regular/Medium empty text. `expanded-templates-results.log` still fails digit3 (margin .069326, just below .07) and gray empty text. This is not a reason to lower confidence thresholds. The copied second matcher and detailed scores are in `probe-expanded/` and `probe-expanded-results.jsonl`.

The 96-case revision adds17 genuinely new size/origin cases (21/27/35pt counts0..4 and fractional origins), a dark empty positive control, and an unrelated-empty negative. New sizes were chosen after the first asset expansion. Fifteen of17 held-out cases read correctly on that source, two reject. Overall results and exact source hashes are in `holdout-summary.json`.

The unrelated-empty negative is a new **false zero**: a heading, wrapped3 and three selected-seat rows at the left, plus unrelated empty text at the right, returns0. This is a synthetic counterexample to associating any empty text in the entire frame with the one detected heading; it does not claim this exact layout occurred on the live site. The implementation must justify how evidence belongs to the selected-seat panel. A stricter panel contract may legitimately reject unsupported bare heading/empty layouts; if adopted, preserve these old inputs and add actual supported-panel positives rather than silently treating rejection as recognition.

## Adaptive-ink diagnostic

The parent next normalized antialiasing by local minimum luminance. The source and assets copied in `probe-adaptive/` are separately hashed. Unlike earlier probes, its mask preprocessing is copied from the actual new source, and the diagnostic proposal search includes its density filter. The input rectangles remain diagnostic oracles only.

The parent's `../reader-adaptive.log` reports four remaining required failures:

- Digit0 has true best0=.82581 versus8=.81098; ambiguity margin fails. Its observed width12 differs from the Arial24 zero template width11, while that font's8 template is width12. Arbitrary margin reduction would hide ambiguity.
- Fractional heading ink bounds(33,31,87×22) score .81957, but searched proposal(32,31,87×23) only .74524. Proposal quantization still misses a readable glyph.
- Gray empty text true bounds(110,157,205×20) score at most .71906; proposal scores .68955. This remains a real support gap, and it also prevents the contradictory-number check from seeing that message.

Normalization currently assumes white background. Uniform off-white250 gives a local threshold253 and is therefore classified as foreground. This is a source-derived unsupported-background concern, not yet a measured live-site defect. A broader normalization change must retain dimmed-overlay, clipping and distractor rejection.

## Engine and object cross-review

The count engine's current independent reading confirms that only a fresh matching request observation authorizes an input in that event step. An Unavailable event invalidates authorization while retaining the baseline and outstanding effect. Unknown/decrease/jump cannot become success by timeout, and positive preexisting totals suppress refresh/navigation. The implementer owns the15 core state regressions; these are not a live Worker/OS proof.

Repeated round-object geometry does not identify selection state. A same-fill tiny white check under its hole threshold remains a possible image candidate; an initial preselected seat may therefore be clicked and then produce a count decrease/hold. The vision implementer accepted this counterexample and is retaining it as a limitation control. A small check cannot be categorically suppressed without also excluding available seats with tiny labels.

A site-displayed selected total is distinct from server reservation success. Optimistic site rendering or simultaneous user changes can decouple a +1 from the current app click. Continuing to observe an unresolved count is safe withholding, not proof that the requested target will eventually be reached. These limitations must remain explicit in completion claims.

## Stronger empty-state support contract

After the false-zero counterexample, the parent explicitly adopted a stronger contract: an empty message may establish zero only with a supported same-panel heading/empty/completion-button relationship and visible container evidence. The two old bare empty positives are retained as negatives, with the old contract preserved in `manifest-96.json`. This changes the declared support contract; it does not demonstrate that the original bare layouts were fixed.

The105-case revision adds four supported synthetic empty panels: left/right placement, blue/gray completion button, and a panel touching the viewport left edge. Five new negatives remove the button, change its label, retain seat rows, contradict empty with digit2, or put the heading and empty in distinct panels sharing a full-width footer. The renderer draws independent scene rectangles and text; it does not call or copy the production layout validator. PNGs for a positive and shared-footer negative were visually inspected.

The visible panel boundary is a deliberate support assumption. No unobscured real empty-state image has yet established that this assumed boundary exists in the user's active layout. Passing synthetic panel recognition alone must not be reported as successful end-to-end initial-zero recognition on the live site.

## Corrected panel results and temporal evidence

`panel-corrected-results.log` records four tests passing against105 fixtures. `panel-corrected-source.json` binds the unchanged source/layout/template/test hashes for that run; source hashes were checked again after execution. The required panel4 positives and same-panel contradiction negatives passed. These inputs share available system fonts with the production bank; they are independent whole-scene renderings, not a held-out cross-platform browser corpus.

The108-fixture manifest adds two unrelated-timer variants (count2 while9:42 becomes9:41) and a second heading outside the original numeric band. All108 listed RGBA files were checked present and frozen after generation. A parent whole-core test overlapped the last regeneration and briefly encountered missing files; that filesystem failure is not a reader result. Parent was informed to rerun only after freeze.

`evidence-counterexample/result.jsonl` demonstrates another defect with the actual reader: baseline count2 yields a numeric evidence rectangle(28,18,161×52). Adding explicit empty text outside it leaves every evidence byte unchanged, but full recognition becomes Unreadable. Therefore crop-only input validation does not cover the reader's global unique-heading/empty-message conditions. The approved runtime correction compares the full acquired frame and, when it differs, runs the full reader once on that already-acquired frame. Count, heading and evidence context must all match before input. A timer change should invoke one reread, not force a recursively stable full frame. The final runtime tests now cover this correction using the actual reader and production guard; they are not OS execution evidence.

Changes after that last acquisition and before OS delivery remain possible. Repeated recapture cannot establish atomicity with an externally changing website. A fast matched count is still a site-displayed value, not proof that the service has reserved the seats.

## Optimization boundary review

The initial early-rejection optimization used multiplication by a floating threshold. At intersection55/union100 and threshold.55, final division equals the threshold but `.55*100` rounds above55, incorrectly rejecting the exact boundary. Parent changed the upper-bound comparison to the same division form as final scoring and added55/77/79 equality regressions. `score-boundary/` preserves the independent private-function check and clearly distinguishes the reconstructed prior comparison from the actually copied corrected source. The proof is specific: perfect agreement on all remaining expected ink and empty remaining background gives the maximum possible score; it does not validate the semantic recognition of a new font.

## Explicit support boundaries

The round-object detector is an optional, default-off path. Generic color-based extraction remains available independently, as does the selected-count option. Round-object tests do not resolve merging or splitting of rectangular, polygonal, narrow, outlined or other non-round seats. Initial separated color components can still belong to one semantic seat; a small same-fill check can still represent a selected seat. A higher count protects against some erroneous follow-up actions but does not infer every correct clickpoint or guarantee progress.

The empty-layout validator establishes a clear vertical lane with at least one actual side boundary, recognized labels and a bounded completion button. It does not reconstruct a complete top-and-bottom container hierarchy. Unobscured real zero-state recognition remains unverified. Non-white background support, unsupported typography, simultaneous manual changes and site rendering delays remain explicit limits.

## Global opacity and final runtime rereview

The reader previously substituted white for alpha<250, even though uniqueness and contradictory-label absence depend on the whole visible frame. A missing pixel outside the numeric crop could therefore pass the global recognition policy. The final reader rejects any visible pixel with alpha<250; stride padding is excluded from that policy. The added independent test starts with actually readable count2, mutates a far pixel outside its evidence rectangle to0/249, and requires Unknown;250 remains supported, and zero-filled storage padding remains irrelevant. The parent final workspace run passed this test. Strict opacity can reject native captures with transparent rounded corners; actual native coverage was not assumed.

The latest focused runtime log `../runtime-count-guard.log` passes 15 tests with one manual timing case ignored. The actual rendered empty/second-heading controls first assert unchanged numeric crop and then reject through the production guard; the unrelated timer is accepted after one actual full read. A missing pixel outside the band rejects too. Later tests cover explicit count-policy manual routing and the phase-scoped context-recovery caller correction. Other cache/Engine glue tests use decoded-reader seams and are not presented as OCR or whole-Worker evidence. Exact final sources are recorded in `final-summary.json`.

The cache stores only a pure decoded value keyed by full RGBA fingerprint and width/height/stride. Current native receipt/request checks, new read serial and per-action proof remain outside the cache. Start/Stop clear it; Pause/ContextLost invalidate action proof. Origin and scale do not alter pixel recognition, while each new proof records current geometry. No additional source blocker was found after these corrections within the reviewed scope. Final integration and unsupported/live-site limitations remain separate.

## Final evidence connection

`../workspace-final.log` passed 367 Rust tests with 0 failures and 2 intentional manual-image/timing ignores. Lines410–419 record all 5 independent reader tests passing. The earlier 364-result run is preserved in `../workspace-before-manual-routing.log` and predates the final caller corrections. The frozen 108 rendered inputs cover 41 required correct reads,30 required rejections and 37 optional variants that must either read the correct value or reject. The final log does not print the optional correct/Unknown split, so no recognition percentage is inferred. The new missing-global-pixel and storage-padding regression passes separately within those 5 test functions.

`final-summary.json` records the final log hash, fixture-manifest hash and exact production/test hashes. All 14 reviewed source/test hashes were independently compared with the corresponding final Rust or JavaScript tested-source manifest and current file; all match. Every frozen fixture hash was rechecked after the run. The parent's full Rust source pre/post check found no changes. `../javascript-final.log` passed 150 JavaScript tests with 0 failures/skips; its 54-file tested-source manifest also matches the current reviewed UI files. The final caller matrix and narrow context-recovery correction are recorded in `caller-review.md`.

Final independent conclusion: the specific demonstrated reader contradictions, float-boundary rejection, opaque-global-scope gap and stale local-evidence authorization were corrected and checked. No remaining source blocker was found within that declared scope. This is not a claim that arbitrary fonts, borderless empty layouts, all seat shapes, native Windows behavior, real booking success, or acquisition-to-input races are solved.
