# Adjacent-seat incident: vision and pre-click color evidence

Five standalone tests pass against the current production `macro-core` and `macro-platform` crates. Source files are not edited. Only this directory contains the probe, its isolated Cargo target/lockfile, and evidence. `results.log` records the run; `result.json` identifies the source hashes and test boundary.

The user log reports RGB (185,224,180), tolerance 12, scale 2 and a second candidate `pixel:122:919` at (122,920). Tests use those numeric settings and positions with a smaller synthetic frame; they do not reconstruct or claim to contain the user's missing screenshot. No OS input, actual browser, NativeAdapter or full Worker ran. Pre-click checks invoke production `OwnedFrame` pixel-center sampling and production RGB matching, matching runtime's integer-image-candidate branch. Runtime capture freshness and foreground validation were reviewed, not executed here.

## Reproductions and controls

1. Recoloring a seat's center while its original-color outer border remains preserves its ID. Not every selected style causes a duplicate.
2. Recoloring the border while retaining an inset of the original color changes the ID from `pixel:97:911` to `pixel:100:914`. The same physical fixture seat still passes current color validation.
3. A selected mark that divides one matching component leaves the original ID and introduces a new `pixel:108:911` fragment, which also passes current color validation.
4. A synthetic unavailable gray card with a 1×3 residual original-color stroke produces exactly `pixel:122:919`, point (122,920), and passes current pre-click color checks. Two pixels fall below the existing minimum of three. Removing the residual color causes the pre-click check to reject it. This explains a possible mechanism, not the actual screenshot's composition.
5. Two valid 2×2 fixture seats separated by one pixel remain separate candidates at center spacing three physical pixels: a broad fixed-distance duplicate filter could suppress a real neighbor. Conversely, a hypothetical valid 19×22 hollow seat has 78 matching pixels but no solid 2×2 block, so a solid-support filter would reject that seat design.

## Source findings

Vision currently identifies connected matching-color components by their minimum x/y, not a persistent site seat identity. It selects a real matching pixel nearest the component centroid; there is no adjacency-spacing or offset-click generation. Engine filtering is by candidate ID. Runtime stores candidates by ID, accepts the latest same-geometry capture, then checks the target pixel's color. It does not verify HTML availability or compensate for changed IDs belonging to the same physical seat.

Thus ID changes can evade previous-attempt exclusion, and residual matching color on an unavailable card can pass both extraction and pre-click validation. Increasing click delay cannot by itself make those two checks recognize semantic seat identity.

## Smallest proposed direction, not implemented

Preserve component bounds/area in `CandidateExtraction` metadata without changing public `Candidate` input compatibility. Record the original component footprint only after an actual seat input is delivered. While that attempt remains retained on the same map, suppress rediscovered fragments inside its original footprint. Preserve nearby non-overlapping real seats; avoid an arbitrary global radius. On proven conflict, stop/new run, or changed map context, manage only the corresponding attempt footprint according to existing lifecycle semantics.

For tiny fragments, a seat-only solid 2×2 criterion would deterministically reject the reproduced 1×3 residue before the first click and avoids choosing a bad first candidate as a size reference. It also excludes legitimate thin outlines or some very small seats. A relative-size rule has different ambiguity: its initial reference can itself be a fragment and different seat designs can have different areas. Neither option should be represented as universal seat recognition. The missing actual frame prevents deciding which geometry the user's site requires.
