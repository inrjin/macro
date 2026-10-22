# Seat selection implementation review cycle

## Final scope

The implementation keeps the demonstrated adjacency-row correction in
`crates/macro-core/src/seat_selection.rs`. The parent separately fixes the
runtime candidate cache. The experimental pixel-mask identity change was
**rejected and reverted** after an adversarial comparison demonstrated a new
duplicate-click regression. This is not a release-readiness or live-site claim.

Implementation owner: `fix_cancellation_start`. Independent reviewer:
`input_adversarial_review`. Adversarial/rendered-object reviewer: `seat_shortcut`.
All reviewers read actual source and callers; reproduction and counterexamples
changed the implementation decision.

## Findings, objections, and resolutions

1. **Bounding rectangles can suppress a different color component.** The actual
   seat extractor recognizes a U-shaped component and a separate 3×3 component
   inside its empty interior. After the first delivered click, `same_seat`
   suppresses the second because one rectangle contains the other's click point.
   `baseline-mask.log` preserves the failing extractor→Engine test. This remains
   an explicitly unresolved limitation of the final bounds-based implementation.

2. **The proposed mask remedy introduces a different regression.** A prototype
   retained immutable original component masks in shared `Arc` row runs, used
   bounding rectangles only as a prefilter, and required 75% overlap of the
   smaller actual pixel set. It fixed the preceding concave-component case,
   maintained shrink/split marks, and excluded mask data from JSON and verbose
   debug output. However, a selected ring repainting as a new central marker has
   zero original-color overlap: the prototype clicks the same physical object
   twice and completes with no actual selection. The prior bounds method blocks
   that second click. See `ring-oracle-before.log` versus
   `ring-oracle-mask-experiment.log`; the oracle controls rendering and selection
   independently of Engine's delivered-input count.

   The reviewers rejected classifying this as merely an old limitation. It is a
   regression introduced by the proposed remedy. An alternative that remembers
   pre-existing inner components was also challenged: a single physical seat can
   already contain disconnected color components, so separate initial labels do
   not prove separate seats. Adding that state would expand uncertainty and
   lifecycle complexity. The mask prototype and tests are preserved under
   `mask-experiment/`, but are absent from production.

3. **A distant small component can split a valid adjacent row.** The original
   ranking checked only the last assembled row. A distant 2×2 component whose
   center falls between two compatible 10×10 components separated by a 2px gap
   caused the pair to lose priority to an isolated front seat. The independent
   actual-extractor→Engine failure is in `independent-before.log`.

   The accepted correction searches compatible rows in reverse creation order,
   preserving the former last-row preference. It stops once the current center
   is more than `0.35 * current.height` below a row's first center; older rows
   cannot pass the existing first-center condition. Both first and last vertical
   checks remain, so gradual y drift cannot connect different rows. The
   independent regression also checks that an upper staircase does not outrank
   a real lower triple, retained-seat extension, fallback, and option OFF.

4. **Review found an experimental test gap.** The first multirow mask oracle's
   256 cases all expected false. The reviewer independently checked that
   distribution. Before rejecting the prototype, the implementer added
   positive cases with identical absolute pixels expressed in different ROI
   coordinates, explicit 11/16 versus 12/16 multirow accumulation, and assertions
   that both outcomes occur. `mask-unit.log` documents that prototype-only test;
   it does not establish anything about the final non-mask implementation.

## Lifecycle and compatibility

The final core does not change Engine state transitions or mark lifecycle.
Delivered/retained marks, failed-candidate retry, cancellation without delivery,
pause/resume, refresh, zone scoping, and new-run reset retain their prior
contracts. Geometry-less imports and serialized candidate geometry retain their
original shape. `model.rs`, `vision.rs`, `tests/seat_candidate_geometry.rs`, and
`tests/seat_selection.rs` were restored byte-for-byte from `before/`;
`engine.rs` was never modified in this cycle. Only the row-search correction
remains in the implementation-owned production scope.

## Cost and validation limits

The rejected mask prototype added sorting and retained row runs only to
accepted seat components, never generic region extraction. Even so, the same
debug fixtures measured approximately 31% more time for 1,500 seats and about
102–104% more for a large component or thin comb. A comb also produced 32,514
row runs for 33,024 pixels. These are local debug comparisons, not optimized-app
or OS latency measurements; they are another reason not to claim this prototype
ready. See `vision-timing-before.log` and `vision-timing-after.log`.

`implementation-final-source.json` records final implementation-owned hashes.
After the rollback, the independent reviewer re-read the final diff and runtime
callers and passed all seven scenarios in `independent-final.log`; the only
remaining core production delta is the row search. The exact ring comparison
records baseline `clicks=1, actual_selected=1, completion_proposed=false` versus
prototype `clicks=2, actual_selected=0, completion_proposed=true`.
The adversarial reviewer re-read the restored code and runtime cache, then
passed five final rendered-object regressions in `vision-final.log`. The final
ring regression requires one physical click, one actual selection, and no
premature completion for a requested count of two. Both final reviews found no
additional blocker in the retained row/cache changes; neither claims to resolve
the concave-bounds limitation.
The parent's final source manifest and full workspace checks supersede interim
test logs. Logs named `initial-mask`, `mask-lifecycle-row`, and `mask-unit` refer
to the rejected prototype. The final implementation still does not infer
semantic seat identity from disconnected paint, touching objects, movement, or
arbitrary selection repainting. No release build, application launch, live-site
operation, OS input, or user-profile modification was performed by this agent.
