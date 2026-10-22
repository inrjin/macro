# Calibration retry correction (unreleased source)

The reported run repeatedly rejected F2/F5 after normal calibration changes. Runtime logs reviewed by the parent show that the old and new captures were both 3024×1784 at scale 2, while the ROI and tolerance changed. Save/F8 placed the live Engine in Idle, but an earlier RestartContext still held delivered-seat records and the old filter profile. The old compatibility check compared those filters forever; recalibrating did not release the hold.

## Implementation

- Separate physical coordinate identity from extraction filters. Same bound window and width/height/scale can retain original seat footprints, failed-seat marks, count obligations and receipt timestamps while using a changed ROI, exclusions, minimum/maximum component size, registered RGB or tolerance for NEW observations. Existing Engine restart freshness, action IDs, count request IDs and alias exclusion remain in use.
- Count verification and object-recognition policy changes remain guarded. Reasons identify the actual UI control and old/current on/off values. Window, unknown geometry, changed geometry and cancellation navigation have distinct reasons and compact reason-code logs.
- Navigation comparison retains the prior scope: a restart into Cancellation with an existing active region compares its navigation definition. F9 preserves its existing current-region handoff; this change does not add a blanket navigation lock.
- Stop preserves an earlier retained checkpoint after Save made the live Engine Idle. A known conflict checkpoint wins over a later stopped snapshot containing ordinary selected seats. An ordinary retained seat does not keep a speculative popup watch. Queued input cancellation still precedes checkpoint capture.
- A failed F9 must not relabel old seat coordinates using a newly saved capture reference. Original target/geometry provenance, including unknown geometry, survives repeated attempts within the same logical run until capture validation succeeds. F9, automatic context recovery and manual Resume validate it. Resume also keeps count/object policy guards, preserves history on an Idle no-op, and checks control epoch after capture/activation before committing a late response.
- Fresh F1 and a successful changed-target Bind retain their existing fresh-context behavior. No automatic new-session inference was added for Completed or for a changed website map with identical window dimensions.

## Review and verification

Implementation: `/root/seat_shortcut`. Independent actual helper/Engine regressions: `/root/fix_cancellation_start`. Adversarial actual caller review: `/root/input_adversarial_review`. The independent reviewer reproduced the original calibration and Stop lifetime failures before the correction. The geometry-provenance failure was found by tracing actual Worker callers; its regression exercises the production shared helpers and Engine, not a live Worker.

Desktop regressions passed: 142 passed, 0 failed, 1 manual timing test ignored (`desktop-final.log`). Independent new regressions passed 9/9 (`../independent/after-provenance-current.log`). `source-final.json` binds the reviewed source, tests and current README; `final.diff` contains the implementation changes. The final Engine doc comment was clarified after compilation without changing executable code; parent full checks will compile that exact final source. Prior verification artifacts and the 0.5.11 application/ZIP are unchanged. This task does not build or distribute an app.

## Limits

The regression fixtures exercise production compatibility/checkpoint helpers plus Engine transitions and synthetic candidate geometry. They do not execute a Worker with native capture, physical OS input, or a live booking site. A matching window/scale is coordinate evidence, not proof that the website still displays the same semantic seat map. Delivered-seat estimates with count verification OFF are not proof of site acceptance. This correction deliberately preserves history rather than silently treating a saved filter change as a new booking session. Actual map/session changes and externally deselected seats remain separate questions.
