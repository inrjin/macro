# Shared round-seat object analysis review

The optional `seat_objects` path proposes complete round objects from color seeds. It does not establish semantic seat identity, availability, site acceptance, or selected state. The generic region extractor and existing option-OFF extractor are unchanged. No application package was built or launched.

## Implementation and supported evidence

`crates/macro-core/src/seat_objects.rs` estimates a dominant background, then learns a repeated round family from at least six independently separated components. Gray inactive seats can supply size references; their color is not hardcoded. Color seeds inside one supported multicolor outline become one proposal, while a narrow color bridge can yield two independently supported circular outlines. Original seed color alone is never the object ID.

- The core must contain at least 45% registered fill color. More than 8% background-colored core pixels are ambiguous, and any unobserved/transparent/excluded core pixel is ambiguous.
- Inner and outer perimeter samples must support a circle. This is core plus sampled-perimeter evidence, not exhaustive verification of every outer-edge pixel.
- Candidate geometry describes the full proposed circle. `geometry.pixels` counts registered-color pixels inside it. The center-based ID remains an image proposal anchor, not a ticket seat number.
- Pre-click validation rechecks the object and exact 2×2 click support on the supplied current frame. It uses the same outside radius as extraction.
- Unsupported scenes return reasons rather than old color-fragment candidates. A registered outline color shared with selected fill is explicitly ambiguous in the tested mixed scene.

## Validation

`regressions.log`: **11 passed**, with the private-image check excluded from the normal suite. These cover dark text splitting a fill, narrow bridges, one-pixel gaps including four neighbors, selected dark fill/expanded halo, outline-color ambiguity, unsupported white marks/sparse references, whole-object disappearance, exclusions, and transparency. The integrated pixel→object→real Engine test uses an independent two-object hit/toggle oracle: each object is clicked once, a lagging counter releases no further input, and completion is proposed only at observed total two. That count is injected by the oracle; text reading and actual OS input are outside that test.

`screenshot-optimized-final.json` ties results to final source hashes. One user-provided 3024×1608 PNG with map ROI `(0,250,1370,1150)` produced two available-object candidates near `(62,865)` and `(242,1153)`; the three dark selected seats and clipped edge fragment were not candidates. The two available cores each had 250/250 target pixels and zero background/unobserved pixels. Registering the outline color instead produced zero candidates and five ambiguity records. Original screenshot bytes are not stored here. Temporary PNG decoding did not apply ICC transforms.

The final optimized shared-Rust test measured **10,024 µs for one extraction** of that ROI, including color seeds and object analysis. This is one local static-image measurement, not average performance, UI latency, native capture speed, or Windows performance. Earlier debug measurements of 237–261 ms are not production-speed measurements.

## Review corrections and remaining limits

Two adversarial regressions failed before correction; source and results are retained in `opacity-before-*` and `dense-gap-before-*`:

1. A missing core pixel had been removed from the denominator, allowing an incomplete object. Core area now includes it and records `IncompleteObject`.
2. A wider pre-click background ring intersected four valid close neighbors. Extraction and pre-click now share the original 1.16R ring; detection thresholds were not weakened.

The separate `probe/` and `counterexamples.jsonl` document limitations, not successful recognition tests. A tiny white marker occupying 3/69 core pixels leaves the same available-color object eligible. If a site uses that exact visual state for an already-selected seat, this module alone cannot distinguish it; a subsequent total decrease is a reason to hold, not to count another seat. The supplied photo uses a different dark selected fill and does not exhibit that ambiguity. Identical pixels can have different semantic seat meanings.

White numbers/marks above the hole threshold, nonround shapes, fewer than six useful repeated references, textured backgrounds, major overlap, and unsupported scales can yield no candidates. Learned circles can be decorative objects inside a poorly chosen ROI. Gray references are visual evidence, not proof of inactive seat identity. Dense or unusually connected layouts may cost more than the measured sparse-color screenshot. No live-site transition, Windows capture, native click, or complete cross-platform recognition claim is made.

Independent source/adversarial discussion involved `fix_cancellation_start` and `input_adversarial_review`. The implementation reviewer re-read the opacity and dense-neighbor corrections. Runtime/count review separately found that reusing a static SCK timestamp as an Engine observation ID could stall after an early count and configured click delay; the parent changed this to validated-read serials while preserving native post-input freshness checks. The probe preserves that actual-Engine reproduction; it is not a whole-Worker or OS reproduction.
