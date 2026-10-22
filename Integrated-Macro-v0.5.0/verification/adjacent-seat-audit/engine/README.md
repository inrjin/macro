# Multiple-seat identity and count audit

No production source was edited. Five component-level scenarios completed: two ID-alias reproductions, two unchanged-ID controls, and one duplicate/stale-ACK control. The standalone executable links the existing compiled production macro-core library. It does not launch the app, inject input, capture a browser, or query a ticket service.

## Confirmed source behavior

- `model.rs` exposes `target_seats` (1–4), not an adjacent-seat group or spacing rule. `engine.rs::select_next` picks another candidate; no relative `x + spacing` move exists.
- `vision.rs` assigns `pixel:min_x:min_y` from the current matching-color connected component. This is an image component label, not a stable site seat identifier.
- `select_next` rejects previously selected/attempted/rejected **IDs**. On ACK, the current ID enters `attempted`; `selection_count` counts the union of ID sets. It does not compare delivered target coordinates or seat footprints.
- Repeated ACKs for the same action ID are already inert. The reproduction uses genuinely different action IDs caused by new component IDs.

## Reproduced counterexample

One fixed site hit rectangle is `[10,19) × [20,29)`. The fixture presents a matching-color component of size 9×9, then 7×7, then 5×5. Production vision returns the same target `(14,24)` every time, but IDs change from `pixel:10:20` to `pixel:11:21` to `pixel:12:22`. Production Engine treats them as distinct attempts.

| Target count | Delivered clicks at same point | App estimate | Independent toggle oracle | Complete emitted |
| --- | --- | --- | --- | --- |
| 2 | 2 | 2 | 0 selected | Yes |
| 3 | 3 | 3 | 1 selected | Yes |

The unchanged-ID controls neither click nor count the same component again. Cancelled/stale/duplicate action ACKs also do not add a selection.

## Limits and review

The fixture's rendering sequence is supplied independently by the test; it is not generated from the toggle oracle's selected boolean. In particular, after a second click toggles the oracle off, the third rendered component is still a smaller synthetic component. This proves an admissible image-sequence/identity defect, **not** that a real site's select/deselect styling follows those exact states. Another reviewer confirmed that limitation and found no additional harness blocker.

Parent-sanitized real logs have different first/second coordinates, e.g. `(106,921)` then `(122,920)`. This exact-point reproduction therefore cannot alone explain that recorded sequence. The runtime/vision reviewer separately checks changed targets, split components, and disabled-seat color remnants. Original frames are unavailable, so the real second component's shape is unknown.

## Minimal corrective boundary, not implemented

Core can prevent the proven exact-target alias by remembering delivered Point targets as well as IDs, rejecting an alias at the same point without adding success-confirmation waits. Only ACKed inputs may acquire this protection. Cancelled inputs must not. An identified conflict must remove/transfer the appropriate protection for the existing retry policy; retained attempts must remain protected across refresh and Pause/Resume. New runs and changed map contexts need explicit reset rules.

Exact-target protection does not solve a checkmark splitting one seat into new components with shifted targets. Stable identity needs original component geometry or another proven correspondence. Replacing IDs with center coordinates still permits shifted fragments. A fixed exclusion radius can incorrectly suppress genuinely adjacent small seats. Neither workaround should be called a complete solution to the real trace.

## Required regressions for a future fix

1. Changed component ID at the exact delivered target is not clicked/counted again.
2. Split/shifted fragments of one seat are distinguished from a genuinely separate neighboring seat.
3. Two or three independent valid candidates still reach the requested count.
4. Unsent ActionCancelled and stale ACK do not reserve a location.
5. Failed-candidate retry honors the existing cancellation search interval; retained attempts remain excluded across refresh.
6. Pause/Resume preserves delivered identity; new run/map namespace resets do not suppress the same coordinate in another zone.

`metadata.json` contains exact compile/run commands, linked-library SHA256 and source hashes. `results.txt` is captured output. These tests characterize the defect; no app or release package was built.
