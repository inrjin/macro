# Independent implementation review

## UI and settings

The new `adjacentSeats` form option maps to `features.adjacent_seats`, separately from `target_seats`. Defaults and older profiles keep it OFF. The reviewer executed the actual UI/model through 24 count/option/mode/trigger combinations with the opposite persisted value. Every button and F1/F2/F5 request carried the current option and count. Import and form round trips also passed. `ui-adjacent-options-results.json` records the run. IPC is stubbed; this is not a native shortcut or seat-selection test.

## Engine identity and priority

The Engine retains the original delivered candidate and geometry, rather than replacing it with a subsequently observed fragment. Same-point or overlapping-footprint aliases are excluded only within the active region and while their original ID remains selected, attempted or rejected. Cancelled proposals do not reserve geometry. Retry/refresh pruning removes failed marks when their rejection is cleared and preserves retained attempts; new-run reset clears marks.

The reviewer read `seat_selection.rs` and the Engine integration plus the 14 new regression cases. They cover a pair/triple ahead of isolated candidates, partial groups followed by separate-seat fallback, extending retained selections, missing-middle and different-row controls, option OFF, geometryless observations, close distinct neighbors, alias exclusion, failure retries, cancellation, pause, new run, and legacy region separation. The follow-up actual-pixel test preserves one attempt across 9×9, 7×7 and 5×5 renders. The touching-box test verifies the Engine's half-open boundary behavior with supplied candidates; it does not establish that vision separates touching same-color pixels. Adjacency is still a geometric heuristic; no seat number or semantic aisle identity is read.

## Extraction and actual click path

The new extractor requires an opaque matching 2×2 patch for seats and chooses a click pixel supported by such a patch. It retains the entire component's bounds and pixel count for identity. Region extraction retains the generic policy. The eight new vision cases include close 2×2 seats, a supported body with a thin tail, opacity/ROI/exclusion/stride checks, thin-outline limitations and clipped-object ambiguity.

In production `observe_frame`, only `Phase::Seats` selects the new extractor. Every `seat:` action in `execute_actions` uses the new pre-click support helper, including the NOL handoff branch that reuses its just-checked frame. The helper retains earlier freshness, color, opacity and geometry checks and uses the effective region-specific vision configuration. Support failure sends `ActionCancelled` before native input. Entry, manual legacy navigation, refresh, completion and region clicks do not acquire the seat-only support filter. The two runtime regressions exercise the shared helper and Engine cancellation, not an instantiated Worker.

## Remaining limits

- The 2×2 rule rejects valid one-pixel lines, thin outlines and some very small seats; it is an intentional geometric tradeoff, not universal availability recognition.
- A matching 2×2 fragment or legend can still be an invalid seat. Cropping a larger nonseat to a small ROI remains ambiguous.
- Footprint geometry cannot certify that two components are the same site seat. Overlapping objects, map reconfiguration, or a change outside the original footprint can escape or over-trigger the heuristic. Geometry invalidation discards old footprints rather than translating them.
- Delivery-based selection counts still do not verify site acceptance. Silent rejection or a site clearing retained selections can make the count inaccurate.

No new blocker was found in the reviewed scope. Core/vision/runtime regression execution belongs to their owners and the final integrator; this reviewer did not duplicate those suites. No installed app, OS input, release build, user settings, or production-source mutation was performed by this reviewer.
