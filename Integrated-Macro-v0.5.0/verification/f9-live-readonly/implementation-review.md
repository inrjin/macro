# F9 live-log diagnosis — read only

No production/configuration edits, tests, builds, application launches, capture calls or OS input were performed. The running booking task was not controlled. This note reads the existing 3,083-line runtime log and source. Log indices below are zero-based; times are local on 2026-10-22.

## Findings

1. **F9 reception is not booking success.** `captcha_manual_completed` appears 28 times across the log, of which 27 are version 0.5.11 and one (index 175) is 0.5.10. The event is written after runtime handoff/capture validation, before future region/seat recognition or site acceptance. One event even has `recovering:true` and `phase:paused`. It does not prove CAPTCHA completion was detected, a correct region was clicked, or tickets were selected.
2. **Region attempts demonstrably reached recognition, then found no matching region pixels.** At 09:28:13 (2386→2387) and 09:31:14 (2419→2420), F9 entered Region. Those scans—not the later settings file—record region ROI `(174,506,1761,1247)`, RGB `(124,104,239)`, tolerance 24, and `matching_pixels=0`, `accepted_count=0`. The first then encountered foreground loss, recovered, and still had zero matching pixels. There is no screenshot identifying which site page was visible.
3. **Object-mode seat attempts often had matching color but no usable repeated shape family.** For example 10:13:37 (2914→2915) and 10:13:51 (2920→2921) entered Seats, with 30,567 matching color pixels but `reference_objects=0`, `template_families=0`, `no_repeated_shape=23`, and zero candidates. This is neither missing ROI nor proof that there were no actual seats. The available evidence cannot distinguish an unsupported seat drawing, unsuitable ROI/background, or a non-seat page.
4. **Two F9 retries reissued completion from old estimated selections.** At 10:12:26, F2 delivered two seat clicks (2864,2867) and completion (2871), storing a 2/2 estimate. F9 at 10:12:54 and 10:13:00 (2873,2877) observed zero object candidates (2874,2878) and sent only completion again (2875,2879). No new seat click occurred in either interval. With count verification OFF this is transmission/estimate evidence, not site acceptance. Engine accepts an empty new map as a fresh observation and checks retained target count before candidate emptiness.
5. **A later F9-owned run did transmit seats; that still does not prove booking success.** After 10:13:51 F9, the same process/run continued scanning through foreground losses. At 10:14:06.447, scan 2954 found 31 candidates; seat actions 2955 and 2961 were delivered, followed by completion 2966. No intervening new start/Save/F8 is logged before those inputs. Pixels changed sharply immediately before candidates appeared; whether the user manually navigated or the site transitioned is unobserved. This refutes a universal “F9 never issued input” explanation without contradicting the user's report that it never achieved the intended result.

## Actual source contract

- `crates/macro-core/src/engine.rs:1083` (`restart_post_entry`): F9 discards entry queues and uses Conflict if unresolved, Seats if retained selection exists, otherwise `after_entry`.
- `crates/macro-core/src/engine.rs:1157` (`after_entry`): Region priority chooses Region, otherwise Seats. This is an option/history branch, not page recognition.
- `src-tauri/src/runtime.rs:5374` (`observe_candidates_on_frame`): Region uses the registered `desktop.region_roi`, requires a registered `stage_center`, and extracts `region_color` (falling back to seat color). At line 5451 the stage coordinate ranks candidates by horizontal alignment/distance; it does not detect the stage on screen. Seats uses its effective seat ROI and the object extractor when enabled.
- `crates/macro-core/src/seat_object_shapes.rs:216` / `:419`: repeated usable shapes are learned; no learned family produces `NoRepeatedShape` and zero allowed seat candidates. The option has no unsafe generic fallback.
- The current settings file says region priority ON, objects ON, count verification OFF, with both region and seat ROIs/stage registered. Those present values are not retroactive evidence for older F9 attempts. A new no-history F9 would choose Region under that saved policy; existing retained records still choose Seats.

## Relation to the unreleased calibration correction

The calibration correction removes the false hold caused by changing ROI/color/tolerance while preserving the same physical window and seat history. It also preserves original coordinate provenance and identifies remaining guard reasons. **It does not modify stage/page classification, object-family detection, zero-candidate progress, or the count-OFF retained-completion policy.** Therefore deploying it alone cannot honestly be described as solving all observed F9 failures.

## Next work candidates, not implemented here

- Expose F9's actual route and cause (registered region-priority route vs retained-seat route), plus “accepted handoff” rather than a success claim. Record candidate/reference counts and the reason a completion is based on retained estimates.
- Diagnose the failing captured scene before changing recognition thresholds or enabling a fallback. Existing color/reference statistics do not identify the visible page or prove that a seat was available.
- Review completion reissue when only old count-OFF estimates exist and the current frame has no independent seat-page/selection evidence. A current-map observation alone is freshness, not a semantic page confirmation. This is separate from the already corrected calibration guard and requires its own contract/reproduction.

Cross-review: engine reviewer confirmed route/fresh-map/retained-count source behavior. Adversarial reviewer confirmed that historical scan configuration must be used instead of current settings and that log delivery is not site acceptance. Both agree the calibration correction does not address these recognition/page-evidence issues.

Source hashes read:

- runtime.rs: 6cd1a72e60d7658eb091cfdf73ca9789f78037139f012c900c2b76c64b02f2ae
- engine.rs: 8444a110ed702236b0e094f13a3af95300fb70f747bdf9b6e7c032433467777d
- seat_object_shapes.rs: 33bad8d441317c4a4c6886c3e9b50efab96d9ea340f551b250d36ce6acb88023
