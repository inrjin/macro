# Seat candidate geometry verification

The seat-only extractor now requires an opaque, matching 2×2 patch. It chooses an actual constituent pixel of a qualifying patch nearest the original component centroid. Geometry records the full accepted component bounds and pixel count, including unsupported thin extensions. The generic extractor keeps its existing region behavior and additionally reports geometry.

`seat_point_supported` applies the same RGB, opacity, ROI and exclusion requirements to the four possible patches containing the proposed pixel. Runtime integration is owned and verified separately by the parent task.

## Focused checks

- `cargo test -p macro-core --test seat_candidate_geometry --offline --locked`: 8 passed. See `seat-geometry-tests.log`.
- `cargo test -p macro-core --test vision_clock --offline --locked`: 15 passed. See `generic-vision-regression.log`.
- Independent source review found no additional blocker. See `../reviewer/implementation-review.md`.

Cases cover the synthetic 1×3 fragment matching the logged identifier/point, separate nearby 2×2 seats, normal 3×3 seats, unsupported centroid/tail pixels, pre-click support loss despite a still-matching target pixel, alpha thresholds, ROI/exclusions, row padding, invalid input and preserved generic extraction.

## Limits

These checks use synthetic RGBA buffers and production pure vision functions. No original incident screenshot, native capture, browser interaction or site selection confirmation was available. Matching the logged point does not prove that the actual incident component was 1×3.

The 2×2 rule rejects legitimate designs rendered only as a one-pixel outline or three-pixel shape. It can still accept a disabled item, legend or other non-seat shape containing a matching 2×2 patch; clipping a larger shape with the ROI can also create an eligible component. Geometry and color are not proof of availability. The extra seat pass and support bitmap were not benchmarked on the target application in this verification.

The prior `source-before.json`, `result.json` and five-test audit describe the earlier source before this implementation and are preserved as historical evidence.
