# Cancellation runtime audit

## Confirmed configuration defect and correction

Inactive options from another UI mode were passed unchanged into the cancellation engine. In original two-region/arrow-key modes, a saved automatic-refresh flag could turn a timed region transition into an F6 wait even though no internal refresh button was authorized. A saved general-booking region-priority flag could redirect those routes into general region scanning.

`apply_execution_options` now normalizes only the cloned execution profile:

- General-booking region priority is inactive in cancellation runs.
- Automatic button refresh requires cancellation mode, no legacy region navigation, the saved ON flag, an allowed internal-button spelling, and a registered Point target.
- Other modes/legacy navigation execute without automatic button refresh. Saved preferences remain unchanged.
- Static legacy first-scan eligibility reads `engine.profile()` so inactive saved flags cannot suppress that scan.
- Explicit `desktop.cancellation_mode = legacyRegions` requires both valid Point buttons at Start/F2. Saving and F8 calibration remain available. The error uses the exact UI labels.
- Requested internal refresh without a button continues scanning and reports “새로고침 버튼 미지정 · 탐색만 진행합니다” in readiness. Start logs distinguish saved settings from effective execution options.

Three focused regressions exercise real Engine transitions after normalization, repeated empty/static OFF scans, and explicit two-region start validation. The existing refresh fence, partial-success, visual-dialog, cancellation, F2, and static-scan desktop tests remain in the suite. `desktop-tests.log` records 76 passing tests before the final valid-Point strengthening in the same validation test; parent workspace verification covers the frozen source. No release/app build, live OS input, or personal settings access was performed.

## Preserved unresolved original-mode assumption

`pre-fix-probe.rs` uses extracted production frame/reuse helpers and the real macro-core Engine. Its two passing **counterexamples** establish both sides of the legacy timing tradeoff:

1. After original timed navigation, an unchanged pre-input frame can be reused as the next region's map and yield a seat action.
2. Requiring a new pixel sequence plus post-input timestamp for that path can leave a valid static map waiting indefinitely when the arrow key or already-open region produces no pixel change.

This audit does **not** add that strict fence to blind timed legacy navigation. Existing original modes still assume their configured delay is sufficient; enhanced legacy paths retain explicit F6 confirmation. Native post/idle observation could distinguish some unchanged frames, but ScreenCaptureKit does not guarantee a later idle callback. A native fresh snapshot would require separate implementation, latency measurements, and live verification. The two probes are source/engine evidence, not proof of an actual user's browser behavior or a fixed defect.

Existing cancellation retry intervals remain unchanged: retrying a failed candidate in a later search pass is distinct from an actual site refresh or a new refresh generation. Empty scans do not end the run.
