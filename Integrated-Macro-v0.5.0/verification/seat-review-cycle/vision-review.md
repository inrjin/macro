# Final independent vision and runtime review

## Decision and verified final source

The precise color-mask proposal was **not adopted**. It fixed the concave bounding-box omission but introduced a new same-object double click. Reviewers challenged the initial classification as merely a known limitation; direct before/after evidence showed it was a regression. The production mask changes were reverted. The independently demonstrated row-grouping and bounded runtime-cache fixes remain.

`crates/macro-core/src/model.rs` and `vision.rs` are byte-identical to the preserved `before/` versions. Final reviewed hashes are in `vision-review-source.json`. The reviewed production `same_seat` retains bounding-box/exact-point behavior; no serialized mask, run allocation or mask sorting remains.

## Final production regression evidence

`crates/macro-core/tests/seat_shape_regressions.rs` contains five normal regression tests; all passed in `vision-final.log`:

- One-pixel background gaps and diagonal-only contact preserve separate nearby objects.
- Selected inset/split remnants do not count one physical object twice, with adjacent preference ON/OFF and requested counts 2/3.
- A split selected object does not hide its neighbor across a one-pixel gap.
- Cancellation retries release only the failed object after the configured interval; selected remnants stay excluded.
- A ring repainted to a new inner marker remains one clicked object: actual selected count 1, requested count 2, no completion action.

The harness renders synthetic labeled objects, passes their RGBA through real seat extraction and Engine actions, and applies click coordinates to independent object state. Clicking a physical object twice toggles its selection off. Engine receives neither oracle object IDs nor the selected set. ACK means modeled input delivery. Conflict tests supply an immediate accurate `DeliveredSeatResult` and dialog-gone event; they do not test native input or visual-dialog detection/latency.

The implementer and independent engine reviewer read the actual harness and its final revision. No additional defect was found within the final retained changes. No browser, screen capture, OS click, app build or real ticket transaction was executed by this reviewer.

## Rejected proposal and preserved counterexamples

The isolated before and mask-prototype packages run the same preserved harness in `ring-comparison.rs`. Their reporting test prints external oracle results:

| Implementation | Physical clicks | Actual selected / requested | Completion proposed |
| --- | ---: | ---: | --- |
| Preserved baseline | 1 | 1 / 2 | No |
| Rejected mask prototype | 2 | 0 / 2 | Yes |

See `ring-oracle-before.log` and `ring-oracle-mask-experiment.log`. This directly supports rejecting the mask proposal, rather than relying on code speculation. `vision-mask-prototype/core` is a complete isolated source copy with `source-hashes.json`; the parent's `mask-experiment/` also preserves the reviewed source/test set.

The C-shaped object plus a separate 2×2 object inside its empty bounding box still exposes a production limitation. `vision-baseline-failure.log` shows only one actual object selected when two were expected. The mask prototype corrected that scene, but cannot distinguish it from some one-object repaint cases using color-mask identity alone. Treating initially separate components as proof of distinct seats would also misclassify a single seat whose original paint is disconnected.

The initial-disconnected-paint scene is a pre-existing limitation; the ring-to-new-marker scene is a regression of the rejected proposal. Experimental known-limit assertions remain only in verification artifacts. They are not counted as final production selection successes. `vision-after.log` is the historical experimental run, not the final source result.

## Final actual-code rereview

- `seat_selection.rs:20`: original exact-point/bounding-box comparison is restored. The empty-space ambiguity is explicitly unresolved.
- `seat_selection.rs:64`: row grouping now revisits compatible earlier rows when a small interleaved component would otherwise split them. The early cutoff is consistent with sorted row creation and the minimum-height distance constraint; first/last row compatibility still prevents gradual bridging.
- `runtime.rs:822`: `update_candidate_cache` retains only current observations and the original in-flight candidate. It cannot retain every historical color-component ID. It restores the original candidate after adding the latest observation, preserving the actual clicked point when a repaint moves the centroid without changing ID.
- The revised runtime regression uses real 5×5→8×5 image extraction to prove that same-ID/different-point condition. Its execution is recorded by the parent in `runtime-cache-after.log`; this reviewer read the final test and production helper.
- Runtime seat extraction remains the seat-only 2×2-support API; region extraction remains generic. Candidate observations move directly into Engine, while diagnostic JSON is separate. After the mask revert there is no new mask cloning/serialization behavior to release.

## Experimental cost, excluded from final production

The rejected mask prototype reused the component index buffer but added sorting, row-run allocations and Arc ownership. The manually invoked ignored timing test used one warm-up and 20 iterations per fixture in unoptimized debug builds. Mean extraction time was:

| Synthetic fixture | Baseline | Mask prototype |
| --- | ---: | ---: |
| 1,500 separate 9×9 seats, 1,024×640 image | 55.68 ms | 72.84 ms |
| One accepted 65,536-pixel component | 20.57 ms | 41.45 ms |
| Connected thin comb, 33,024 pixels | 9.60 ms | 19.62 ms |

See `vision-timing-before.log` and `vision-timing-after.log`. These single paired local runs may include concurrent development noise. They exclude capture, UI, Engine overlap comparisons and OS input, and are not release-app latency measurements. Additional work was material in these debug fixtures; it was not treated as free.

Run storage was also shape-dependent: 256 runs for the dense component versus 32,514 for the comb. At 12 bytes of run payload this is 3,072 versus 390,168 bytes, before vector capacity, Arc headers and allocator overhead. Current-frame candidates could share masks but still incur initial storage. These costs belong to the rejected experiment; final `vision.rs` contains no mask sorting or storage changes.
