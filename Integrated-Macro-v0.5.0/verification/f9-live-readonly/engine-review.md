# F9 live-report read-only Engine review

2026-10-22. Source review by `/root/fix_cancellation_start`, cross-challenged with `/root/seat_shortcut` and `/root/input_adversarial_review`. No source, settings, application, controls, build, native input, or tests were changed/executed. This note is the only new artifact from this review. Runtime facts below are sanitized metadata supplied by the parent/runtime reviewer; this reviewer did not independently reread the private log file.

## Actual observations and interpretation

- Parent found 28 F9-processing records. `captcha_manual_completed` is emitted after an Engine handoff and may also be emitted while recovering/Paused. It does not establish an actual seat click or website acceptance.
- Around 09:28 / 09:31, F9 entered Region, and the captured region analysis reported `matching_pixels: 0`, followed by foreground waits. That evidence supports an empty region-detection gate, not a universal keyboard-reception failure.
- Around 10:12:54 / 10:13:00, F9 entered Seats, the object analysis returned zero candidates/reference objects (`no_repeated_shape`), yet completion was re-delivered. This matches the existing count-OFF retained-target path: an empty current map observation clears the restart observation gate, then the prior attempted count reaches the target before candidate selection is considered.
- Around 10:13, a fresh/logically restarted Seats run also reported zero objects. However, the runtime reviewer traced the later same-run continuation after the 10:13:51 F9 to two seat input deliveries followed by completion delivery. Therefore “F9 never produced input” is contradicted by the log. “F9 successfully secured two seats” is not established: count verification was OFF and there is no server/site acceptance proof.
- Current scheduled run 6 / current saved region-priority/ROI/color/object settings must not be assigned retrospectively to earlier runs. `freshRun` means runtime run-ID/Idle bookkeeping, not proof of a new site session or a snapshot of effective options.

## Source boundaries

The relevant released runtime snapshot is `verification/calibration-retry/implementation/before/src-tauri/src/runtime.rs`:

- lines 629–633: `freshRun` is `run_id == 0 || Engine phase == Idle`.
- lines 4809–4885: F9 uses the saved profile, enters Normal, removes its scheduled deadline, resets CAPTCHA detector/handoff state, restarts post-entry, and validates permissions/window/capture. The completion log at 4881–4884 includes `phase` and `recovering` and is not an input-delivery result.
- lines 4819–4820: the released retained-history comparison can reject changed ROI/color/interpretation settings. The calibration-retry fix is unreleased and does not modify the running 0.5.11 application.

Current Engine executable behavior is unchanged by the calibration fix (only its documentation changed):

- `crates/macro-core/src/engine.rs:1047`: F9 uses `restart_after_entry`; it does not replay scheduled/date/session/next entry.
- `engine.rs:1083`: unresolved conflict stays Conflict; retained selection goes to Seats; otherwise `after_entry` chooses the configured route.
- `engine.rs:1157`: no retained history plus region-priority ON goes to Region. The Engine does not determine from pixels whether the browser is already inside a seat map. Starting Region from an already-entered seat map is a possible failure condition, but no supplied frame proves that was the actual screen in the historical Region failures.
- `engine.rs:560`: a delivered region click automatically enters Seats in the F9 continuous path. There is no mandatory extra F6 after a successful region delivery in this path.
- `engine.rs:705`: a current-generation candidate observation, including an empty list when count verification is OFF, clears the restart observation gate.
- `engine.rs:1411`: selection first checks the retained count; if it already equals the goal, it sends completion before looking for another candidate. Existing attempted IDs/footprints persist on F9.
- `engine.rs:1648`: with count verification OFF the count is selected/attempted ID union, not live website truth.
- `crates/macro-core/src/seat_object_shapes.rs:419`: no learned repeated shape produces no accepted object candidates. It is not evidence that the website has no seats; it means this recognizer supplied no supported candidates from that observation.

## Cross-review conclusion

All reviewers agree on three separations: command processing is not seat acquisition; a later actual input delivery is not site acceptance; current settings cannot be applied retroactively to historical runs. The unreleased calibration correction addresses repeated restart rejection after valid filter changes and retained checkpoint provenance. It does not by itself solve object-recognition failure, classify arbitrary current site pages, or confirm that old optimistic selections still exist.

No guaranteed F9 recovery claim is supported by this evidence. No change should be applied to the scheduled live run as part of this read-only review. A future correction would need to target the observed post-command detection/state behavior and be validated separately from shortcut reception; no implementation is authorized or performed here.
