> Historical experimental review. Superseded by `vision-review.md`: the mask change was rejected after its new ring-repaint regression was compared against the baseline. The prototype is not production code.

# Independent pixel-shape and runtime integration review

Production implementation was owned by the core/runtime agents. This reviewer changed only `crates/macro-core/tests/seat_shape_regressions.rs` and verification artifacts. Reviewed source hashes are recorded in `vision-review-source.json`.

## Reproduction and regression evidence

The new harness renders labeled synthetic objects, passes RGBA through the real seat extractor and Engine, then applies emitted click coordinates to independent object-selection state. A second click on one physical object toggles its real selection off. The Engine never receives the oracle's object IDs or selected set. Input ACKs only model delivery. Conflict tests explicitly provide an immediate correct `DeliveredSeatResult` and dialog-gone event; they do not test visual dialog detection, attribution latency or native Esc handling.

`vision-baseline/Cargo.toml` links the preserved `before/crates/macro-core` implementation to the same new regression source. `vision-baseline-failure.log` shows the pre-fix failure: a C-shaped object and a separate 2×2 object inside its empty bounding box yield only one actual selection instead of two.

`vision-after.log` records **five normal regression tests passed**:

- Concave bounding-box empty space does not suppress the distinct object, with adjacent preference ON/OFF and direct/cancellation modes.
- A one-pixel background gap and diagonal-only contact preserve separate neighbors.
- Inset and split original-color remnants cannot count one object twice for requested counts 2/3.
- Split selected remnants do not hide an adjacent object across a one-pixel gap.
- Cancellation retry releases the failed object after its interval, preserves the first selection, and issues one modeled Esc.

Two additional tests are **known-limit observations, not correct-selection successes**. Both reach a completion action with actual selected count 0 versus requested count 2:

1. A single object's original paint is already two disconnected components. This limitation exists before and after the change; component identity is not physical seat identity.
2. A selected ring repaints to a new same-color marker in its originally empty center. Original/new color masks have no overlap. The old bounding-box policy happened to suppress this click; the new precise-mask policy permits it. This is an explicit tradeoff, not merely an unchanged pre-existing limitation. Filling the ring interior would suppress a genuine separate inner object in the converse scene.

The implementer and independent engine reviewer inspected the actual harness and agreed its scope is appropriate. Neither these tests nor that agreement establishes live-site correctness. No browser, native capture, OS click or real ticket transaction was executed.

## Actual-code review

- `vision.rs:234`: mask construction is seat-only, after size/support acceptance. Click-point selection occurs before sorting, preserving the existing nearest-supported-pixel tie order. Generic region extraction keeps `mask=None`.
- `model.rs:271`: accepted component indices are sorted and encoded as immutable horizontal runs, including absolute ROI offsets. No bounding-box bitmap is allocated. Clones share the mask through `Arc`.
- `model.rs:286`: two ordered run streams count actual pixel intersection, with an integer ceiling of 75% of the smaller component and early success. Row transitions and disjoint intervals do not contribute overlap. This describes color correspondence, not semantic object boundaries.
- `seat_selection.rs:20`: valid bounding rectangles are a coarse rejection step; two native masks use actual overlap. Exact same-point protection and legacy bounds-only compatibility remain. No fixed-radius neighbor exclusion is introduced.
- `seat_selection.rs:71`: the corrected row search can revisit an earlier compatible row despite interleaved small components. Its early cutoff is valid because row creation follows sorted center Y and the allowed minimum-height distance cannot exceed the incoming node's height bound.
- `runtime.rs:822`, `runtime.rs:4758`: candidate cache contains the current observation plus the original in-flight candidate. Old IDs do not accumulate. `Candidate::clone` preserves the mask's `Arc`, and the original point/mask wins over a repaint with the same ID. The revised runtime test uses actual 5×5→8×5 image extraction and checks original `Arc::ptr_eq` preservation; execution belongs to the parent's runtime check.
- `runtime.rs:3986`: candidate observations move directly into `Engine::step`; cache and delivered-mark clones do not round-trip through JSON. The diagnostic `candidatePreview` serializes geometry separately, where `#[serde(skip)]` omits the mask without changing the live object.

No additional blocking defect was found within these reviewed changes. The two semantic limitations above remain observable and must not be included in a claim that all duplicate selection is solved.

## Timing and memory cost

`manual_same_fixture_extraction_timing` is ignored in ordinary test runs. It was run manually against the preserved baseline and new source using the same debug toolchain, fixtures and 20 iterations, after one warm-up. Logs are `vision-timing-before.log` and `vision-timing-after.log`.

| Synthetic fixture | Before, mean per extraction | After, mean per extraction | Change |
| --- | ---: | ---: | ---: |
| 1,500 separate 9×9 seats in 1,024×640 | 55.68 ms | 72.84 ms | +31% |
| One accepted 65,536-pixel component | 20.57 ms | 41.45 ms | +102% |
| Thin connected comb, 33,024 matching pixels | 9.60 ms | 19.62 ms | +104% |

These are unoptimized local extraction measurements from single paired runs, with possible concurrent development activity. They exclude capture, UI, native input and Engine overlap comparisons. They are neither release-app latency measurements nor a timing guarantee. The added sorting/allocation cost is material in these debug fixtures and should not be described as free.

Sorting adds O(P log P) work per accepted component. A horizontal run has three `u32` values; using 12 bytes of payload per run, the dense 65,536-pixel object stores 256 runs (3,072 bytes), while the comb stores 32,514 runs (390,168 bytes). The latter is much larger than a packed bitmap of its 256×256 bounding box. Actual allocated capacity, vector/Arc headers and allocator overhead are additional and were not heap-profiled. The 1,500-seat fixture has 13,500 runs (162,000 payload bytes) across many allocations. `Arc` sharing and bounded runtime caching prevent copying these bytes for each candidate clone or retaining every historical observation, but do not eliminate the current-frame storage cost.
