# Shared macro engine

`macro-core` contains no browser connection, HTTP client, OS input, CAPTCHA solver, or GUI. Both operating systems consume the same `Engine` decisions. The desktop adapter supplies positive screen observations and executes returned actions.

```rust
use macro_core::{Engine, Event, Profile, RunMode};
let mut engine = Engine::new(Profile::default()).unwrap();
let actions = engine.step(Event::Start { mode: RunMode::Normal }, 0);
let state = engine.snapshot();
```

Time arguments are milliseconds from one monotonic origin. Never persist `sale_deadline_ms` across process restarts. `clock::estimate_clock` models HTTP Date precision and request latency; it does not identify the actual booking scheduler.

## Adapter contract

- Process `Observe` before its following input action. Observe actions have IDs but require no acknowledgement.
- Acknowledge each input action ID once with `ActionAcknowledged`. An acknowledgement means input delivery, never successful reservation.
- Reject or pause on a changed browser binding, capture scale, occluded target, missing permission, or unsupported element target. Never click a different active browser window as a fallback.
- Before discarding queued physical actions on pause/context loss, send `ActionCancelled { id }` only when the adapter proves they were never delivered. This restores an unclicked seat, date step, completion control, or region navigation for a later retry. An acknowledged action cannot be rolled back this way. Invalidate capture data and keep asynchronous observations scoped to their originating run and browser binding.
- Supply `SeatResult::Selected` or `LocallySelected` only for positive selection evidence. Color disappearance, silence, or an input receipt alone is insufficient.
- Enhanced region selection waits for `RegionMapReady`. Enhanced refresh waits for a strictly greater `RefreshCompleted.generation`. A new screenshot alone is neither event.
- After a conflict with retained selections, `SelectionObserved` must identify which selected IDs actually remain. The engine then fills only the shortfall.
- `CompletionConfirmed` must positively identify completion for the requested seat count. This is the only event that reaches `Completed`.
- Unknown screen states remain visible waits. Exact calendar dates/session values are observed at runtime; future hidden coordinates are not mandatory.

## Legacy and optional behavior

All additional feature flags default off. The default original click sequence records `attempted_ids` after acknowledged clicks and can click the configured completion button. These are explicitly separate from `selected_ids`. Enabling conflict recovery disables this blind progression and requires selection evidence.

An optional `entry_target` provides a concrete visible booking button for normal F1. The engine clicks it once when the configured deadline arrives, or immediately for an unscheduled start. It then waits for a positively identified seat screen; the old date/session/next coordinates are not replayed onto the newly opened page. A bridge may continue typed entry stages when `auto_entry` is enabled. Without that bridge, the adapter must expose manual seat-screen confirmation and reject observer-only starts that have no actionable target. F2 ignores this normal-booking target.

`update_entry_target` supports paused calibration. Updating or clearing an already delivered target does not re-arm it or bypass the manual screen wait. Changing a target whose delivery is still ambiguous is rejected. Only a proven-undelivered `ActionCancelled` permits retry. Manual entry waits have no automatic observation timeout.

Original F2 navigation additionally preserves its timed region/arrow loop: after acknowledged navigation and the configured region delay, it scans again without a manual transition confirmation. This creates a new scan generation, not proof of refreshed site data. It is disabled for enhanced conflict/refresh behavior. `site_auto_refresh` waits for externally observed site refresh evidence instead of requesting an internal Refresh button.

F2 starts with its configured first region. `legacy.regions` holds up to two optional `{color, vision, complete}` overrides; omitted values use the global settings. `snapshot.active_region` is `0` or `1` during this flow. Native adapters must use `effective_color`, `effective_vision`, and the active context when detecting candidates. Normal F1 and the optional single-region priority flow use global settings. Single-region priority does not alternate the original two regions.

`update_calibration` and `update_legacy_regions` change current-screen settings only while idle or paused, preserving selected and rejected seats. They reject an effective color change while a click outcome is unresolved. Clone the engine to stage these updates before persisting the settings, then commit both changes atomically.

When capture dimensions or scale actually change, call `invalidate_coordinate_geometry` on that staged idle/paused engine before calibration. It clears old coordinate targets, queued legacy targets, ranges and candidates, while keeping colors, arrow navigation, selections and the current phase. It rejects ambiguous initial-button delivery and never repeats an already delivered entry click. Adapters must discard their corresponding capture/candidate caches.

Vision accepts borrowed padded RGBA buffers through `extract_candidates_view`. It excludes configured legend/overlay rectangles, extracts connected matching-color regions, and chooses actual component pixels. A color component is still only a candidate; adapters must bind the correct map context. Region ranking requires verified center-group and stage-distance metadata and never assumes image center is stage center.

Native pixel candidate IDs are stable `pixel:minX:minY` within one bound map; refresh generation belongs to the event. A changed map, position, scale, or component geometry requires adapter validation. Pixel IDs cannot replace semantic seat IDs when artwork changes.

## Verification

From the workspace root:

```sh
cargo test -p macro-core
```

Tests cover original click flows, delayed entry, manual CAPTCHA, region transition, partial conflicts, refresh generations, completion evidence, paused calibration, RGB geometry, and bounded clock estimates. Native input/browser behavior needs separate platform tests.

Replay a JSONL evidence trace without executing any native input:

```sh
cargo run -p macro-core --example replay -- profile.json < events.jsonl
```

Omit `profile.json` to use default settings. Each nonempty input line is
`{"now_ms":0,"event":{"type":"start","mode":"normal"}}`; each output line contains
`actions` and `snapshot`. Invalid input exits with an error rather than silently
skipping evidence. Action IDs in the trace must match the preceding output.
