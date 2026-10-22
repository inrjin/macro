# Implemented engine behavior

The original `metadata.json` and reproduction output remain pre-fix evidence. `implementation-check.json` records the new targeted check and changed source hashes.

- Each delivered seat keeps its original candidate geometry and actual Point target, scoped to the active legacy region. A later ID at the same point, within the original bounding rectangle, or with substantial overlap cannot be counted as another seat. Original footprints are not replaced by smaller selected-seat fragments. Geometry-less imports receive exact-point protection only.
- Unsent cancellation does not create a footprint. Existing pause/resume retains marks; failed marks leave on the existing search retry or explicit refresh, while retained attempts remain. New runs, post-CAPTCHA restarts, and explicit coordinate-geometry invalidation clear marks.
- With `adjacent_seats` enabled, similarly sized same-row boxes separated by at most 0.75 of the smaller width form short-gap groups. Full required groups precede partial groups and isolated fallback. Retained seats anchor subsequent choices next to that group. Same-row centers use a height-relative bound; no pitch is inferred from sparse candidate distances. OFF preserves candidate order.
- No new success-confirmation waits, site DOM/list inspection, relative spacing clicks, or guaranteed seat-number adjacency were introduced. Only recognized candidate targets are clicked.

Targeted core tests: **14 passed**. They cover exact/shifted-fragment identity, a real-extractor shrinking-render sequence, separate touching 2×2 candidate boxes, cancelled/stale ACK, retry/refresh retention, pause, new runs, region separation, full 2/3 groups, partial-group fallback, retained-neighbor extension, missing-middle/different-row rejection, OFF and missing-geometry compatibility.

The touching-box case is an Engine boundary test with independently supplied candidates. Actual same-color pixels touching edge-to-edge can form one connected component in vision; this test does not claim such raw pixels produce two seats. Image geometry also cannot prove server seat numbers or distinguish every unusual overlapping/curved layout. OS delivery and live ticket selection were not executed.

Independent review found no additional blocker in this source/test scope. Parent owns the full workspace run and the separate rendered-site/seat-extractor/point-validation end-to-end checks.
