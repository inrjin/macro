# P1 runtime replay review

The extracted `apply_scan_capture`, `navigation_requires_new_pixels`, and `candidate_available_in_frame` helpers retain the prior production decisions. This refactoring makes those decisions callable by the tests; it does not introduce a new transition-readiness policy.

The replay's observed outcomes agree with `P1-oracle.md`:

- Legacy two-region and arrow navigation can admit pre-input cached pixels after the configured wait.
- A newer sequence alone can represent a native frame that already arrived before input.
- Strict sequence/time fencing can leave a legitimately unchanged destination waiting.
- Fresh callback metadata and pixel-identical maps do not identify whether the destination has finished rendering.

The initial replay omitted the second native capture query performed by production's `candidate_still_available`. That review finding is addressed: both production and `Replay.scan_with_preclick` now call the extracted `apply_candidate_capture` handler. The extraction retains the previous geometry checks and cached-frame replacement behavior. The old-cache counterexample explicitly supplies an unchanged second query. Separate cases supply a newer matching seat, a disappeared seat, alpha-249 pixels, changed geometry, and a capture error. Removed or insufficiently opaque candidates produce `ActionCancelled`; geometry and capture failures propagate as errors. This closes the identified component-harness gap without claiming the full Worker's error-handling path was exercised.

The owner reports six passing P1 tests in `production-harness.log`. This review independently read the updated production helper, its two call sites, the separate pre-click test inputs, and the isolated extraction diff; it did not rerun the owner's suite. The helper extraction is behavior preserving, not a fix for semantic navigation readiness.

No app, browser, native input, or user settings were exercised by this review. Hypothetical strict-fence runs are comparison experiments, not the production policy.
