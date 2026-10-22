# Learned silhouette implementation review

This changes only the optional object extractor and its process-local geometry evidence. Generic color-component extraction still has no object proof. Platform input, CAPTCHA, scheduling, and selected-count reading are outside this implementation.

## Recognition contract

Opaque foreground is separated from the measured dominant background. Whole foreground components may contain several colors; registered-color components do not define seat identity. At least six complete observations of a repeated silhouette support a family. Families are learned from pixels, without names such as rectangle/circle/diamond in production code. Several families coexist; matching and ambiguity apply to local proposals. Clean all-available maps may supply references too.

Learned masks preserve concavities. Hole-bearing references are excluded, rather than filled, because repeated white selection marks must not become an apparently valid new shape. Every reference has its own surrounding-background validation, including previously seen signatures. A component larger than the reference limits remains a search seed while its stored pixel-coordinate vector is bounded.

A whole component can be fitted to a repeated template with up to two pixels of rasterized size variation. Only its one-pixel normalized edge band is adjusted. Missing interior remains expected occupied evidence and is tested as missing/background content. This handles gray versus colored antialiasing without weakening internal mark checks.

A narrow connection can be split when two complete repeated silhouettes each retain their own visible background boundary. A large repeated compound cannot override those smaller independent hypotheses if they disjointly explain its occupied mask. Wide connections are not positive split evidence.

Conversely, separated fragments are locally held when their arrangement supports a larger repeated whole outline interrupted by background-colored content. All proposals inside that competing whole are held, including a third middle fragment. They are not joined into a new clickable object. Real small objects with identical pixels to a white-striped larger object remain ambiguous in that local context.

## Click and identity evidence

`CandidateGeometry.object_shape` shares a private validated bitset through `Arc`. It is omitted from JSON; an imported/serialized bbox cannot authorize an object-mode click. Shape methods reject nonfinite, negative, fractional-grid or mismatched bounds. Generic color geometry has no shape proof and retains the original behavior outside object mode.

Each candidate also shares a process-local `SeatObjectContext`: ROI RGBA fingerprint including alpha, dimensions/stride, all vision configuration, color and tolerance. When any of these pure-analysis inputs change, pre-click validation re-extracts exactly once on that same acquired frame and requires a currently allowed candidate with identical bounds and silhouette. It does not compare candidate ID strings, which cancellation mode may prefix with a region. An unchanged context still runs the local checks. Missing context is rejected, including after JSON serialization.

The newest frame must still support the exact stored silhouette, opaque core, surrounding background, registered fill, and a solid 2×2 click patch. The overall shape does not prove availability or server acceptance. A registered outline color shared with another solid state remains ambiguous within the learned family. Count verification remains an independent optional safeguard.

## Bounds, cost and limitations

Reference silhouettes are 5–128 pixels per side, at most 32 families. Per-component sliding searches have a 100,000-placement limit; all-frame search has a 500,000-placement limit. Budget exhaustion is reported. These limits bound work units, not a promised wall-clock latency. Exact masks reuse topology arrays, identical candidate proofs share `Arc`, and failed exterior checks abort early. Larger sliding searches lazily build a foreground summed-area table for that component: the count in a candidate rectangle is an exact upper bound for its silhouette's visible foreground, so insufficient rectangles are rejected without scanning every template pixel. Separate exact objects do not allocate this table. This corrects the measured sparse-border case without sampling selected colors or changing positive classification.

No universal semantic resolution is claimed. A narrow-neck single object and two bridged seats can have identical pixels. Likewise two small objects and white-separated fragments can be visually identical. Fully merged shapes, absent repeated references, large white lettering/marks, nonuniform background, and unsupported rasterizations can be held. Small marker changes with the same registered color can still be visually indistinguishable from available fill. No new learned-shape policy turns input delivery into selected-count proof.

Independent tests render mathematical shapes and compute actual site hit/toggle truth. They cover positive extraction, pre-click validation, Engine count ON/OFF, and close neighbors; they do not execute native input or a live ticket site. The provided screenshot validates the original round layout only. Synthetic non-round positives are not a claim of every operator layout.

## Review corrections recorded

- Removed a coarse core-color sampling shortcut because it hid outline-only ambiguity and biased checker-pattern fills.
- Removed one global mask-overlap cutoff in edge fitting: antialiased diamond perimeters changed area disproportionately. Local one-pixel edge/interior evidence now governs fitting.
- Rejected enclosed-hole references to prevent repeated selection markers teaching a new family.
- Added local larger-whole competition to prevent repeated open white stripes teaching two independent fragment families, while retaining all-available support.
- Added disjoint smaller-template evidence before rejecting compound learned families.
- Extended local fragment competition to every contained piece, preventing a third middle fragment from remaining clickable.
- Bound pre-click authorization to the entire ROI interpretation: new neighboring fragments or reference/state ambiguity cannot reuse an old locally unchanged mask. Fresh interpretation occurs only once, with no repeat-capture loop. The interval after acquisition and actual OS delivery remains a TOCTOU limit; a finite hash is also not a mathematical equality proof.
- Bounded oversized component coordinate storage and reused exact templates without skipping each reference's validity checks.

## Final focused evidence

- Independent adversarial corpus: 20 passing tests, two manual performance tests ignored during the normal run (`../adversarial/final-corpus.log`). Cases include analytic shape/hit truth, antialiasing, all supported shapes, repeated white-hole/open-stripe selection styles, mixed families, and a fully connected 3×3 positive grid exercising the integral prefilter.
- Independent Engine/site-oracle integration: 11 passing tests (`../independent/after-context-integration.log`), including both count policies, all-available maps, slanted close neighbors, partial success/retry, three fragments, stale proof, and region-prefixed candidate IDs.
- Process-local proof contracts: two passing tests (`../evidence-contract-final.log`), covering checked dimensions/coordinates, shared Arc lifetime, serialization loss, fresh-context re-evaluation, and ROI changes.
- Existing round-object tests: 11 passing normal tests; the private screenshot test is run separately by the parent on the final revision. Earlier actual-screenshot checks restored two available objects and excluded the selected objects; they are not non-round production-site validation.
- The parent owns final workspace/JavaScript checks and final private-screenshot analysis. Source hashes are in `source-final.json`; earlier failed and intermediate logs remain preserved.

Independent optimized static-image timings (`../adversarial/final-performance.log`, single test thread): 1,000 available objects took 20.811 ms median extraction over three samples; one unchanged-frame pre-click check took 0.492–0.544 ms. With only three available objects among 1,000 references, rounded/slanted scenes measured about 7.5–7.7 ms including pre-click checks and the independent oracle. The sparse 400×400 border seed with six 100×100 reference objects dropped from a measured 1,457.9 ms before the exact integral bound to 3.263 ms after it (one sparse sample in each reported run), retaining zero candidates. This is neither app latency nor an OS/website benchmark, and no worst-case millisecond guarantee is claimed.

The compatibility `template_diameter` diagnostic is only the mean width/height of a sole learned family; it is not a geometric diameter for non-round shapes and is not shown as such in the UI.
