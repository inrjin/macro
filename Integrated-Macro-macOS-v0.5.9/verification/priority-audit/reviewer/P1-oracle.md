# P1: transition evidence oracle

Keep three times separate: content generation, native image arrival, and worker copy/scan. The current macOS `captured_at_ns` is the second, not the first. A click receipt records native input posting, not completion of browser rendering.

| Scenario | Controlled truth | Required interpretation |
| --- | --- | --- |
| Old worker image copied again after click | Sequence and captured time precede input | Do not make a post-navigation seat attempt from it. |
| Native cache advanced before click, worker had not copied it | Sequence is newer than the worker's image; captured time still precedes input | Sequence-only checks are insufficient. |
| Fresh image arrives after click, destination renders later | Frame metadata is fresh, but the old region is still visible | A fresh-frame check does not prove destination readiness. Measure any proposed old-region seat attempts explicitly. |
| Completed destination is pixel-identical | SCK may emit idle updates with the old image sequence and capture time | Requiring a changed image can wait indefinitely. A timeout permits progress but cannot prove which region is active. |
| Input was cancelled before down | No delivered navigation | Do not create a transition barrier based on an undelivered input. |

For a fixture, store its truth-only region identity and readiness separately from the pixels consumed by the algorithm. Measure navigation post time, destination-ready time, first eligible observation, first proposed seat attempt, and the region identity at that attempt. Report old-region attempts, correct attempts, and waits beyond a declared deadline; do not count all fresh frames as successful transitions.

Two runs can expose identical pixels and metadata while one completed a same-looking navigation and the other did not navigate. Without additional evidence, those runs are indistinguishable. This is a limitation to report, not a test oracle to resolve by assuming success.

Sources reviewed: `src-tauri/src/runtime.rs` transition/cache helpers and `observe_frame`; `crates/macro-platform/src/macos.rs` cached copy paths; `native/MacPlatform.swift` complete/idle callback bookkeeping; Windows synchronous capture paths. No production source was changed by this reviewer.
