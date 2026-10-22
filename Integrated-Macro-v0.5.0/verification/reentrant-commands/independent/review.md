# Independent reentrant-command review

Final focused result: **15 passed, 0 failed**, recorded in `final-core.log`. The 12 reviewed source/test files in `source-reviewed.json` were unchanged between the final focused test's before/after hashes. This is source and component verification; it does not establish real browser/OS input delivery or authorize a release.

## Reviewed implementation and callers

- `crates/macro-core/src/engine.rs`: `restart_search`, `restart_after_entry`, `restart_post_entry`, retained footprints/count state, fresh-observation gate, completion and excessive retained-count handling.
- `crates/macro-core/src/model.rs`: optional `manual_entry` defaults through the existing defaulted feature schema.
- `src-tauri/src/runtime.rs`: ingress epochs, stale control rejection before mutations, `supersede_execution`, profile save/start, mapping checks, F9 handoff, original native input cancellation closure, count receipt lineage and visual-conflict recovery.
- `src-tauri/src/main.rs`: enqueue before blocking dispatch, outdated execution replies and minimized-window handoff guards.
- `ui/app.js`, `model.js`, `overlay.js`, `index.html`: replacement starts, invalid replacement stop, authoritative profile reconciliation, response ownership, control/run/revision ordering, default-OFF manual entry.
- Peer tests in `src-tauri/src/reentrant_runtime_tests.rs` and the independent DOM harness were read. They test real helpers, not a constructed full Worker or an actual Tauri window.

## Findings and corrections

1. **F9 was limited to one CAPTCHA phase.** The original actual Engine reproduction in `before.log` failed two retry checks. The final API supports explicit post-entry reruns and search-mode handoff to Normal without replaying initial/date/session/next inputs. A delivered initial booking click waiting in Entry is accepted; an undelivered initial click and scheduled waiting are rejected. The user's reported F9 message alone did not reveal their exact engine phase.
2. **Selection history could be erased by a naive restart.** The implementation preserves original delivered/retained/failed footprints and outstanding count evidence in compatible search contexts, while discarding pending input, old candidate generations and old authorization. Action IDs remain monotonic. Failure marks become retryable under the existing refresh/search policy; retained seats remain excluded.
3. **A lowered target allowed an extra click.** `lower-target-before.log` reproduces two retained attempts, a new target of one, and a newly issued third seat. The final engine holds both new input and completion above the target. The independent regression also raises the target again and confirms useful searching resumes without re-clicking the retained seats.
4. **Saved/effective settings could mix after failed replacement then F9.** Source review traced save creating an idle engine under new settings, followed by restoration of only the old Engine. F9 now validates retained ROI/color/tolerance/count/object interpretation and explicitly applies the current effective Normal profile. This correction was checked through actual helper tests and caller order, not through a full Worker execution.
5. **F2 could be rejected after F9 preserved a legacy zone.** F9 changes mode to Normal while retaining `active_region`. The mapping helper previously used the old mode to decide whether to read zone settings. It now uses the active region; the peer helper regression and independent Engine test cover F2 → F9 → F2, zone ROI/color/completion and alias exclusion.
6. **Applying new options could revoke an already accepted F9 handoff.** `restart_after_entry` formerly validated the old state, changed the profile, then revalidated via `step`. Enabling manual/Enter/NOL options could silently leave Entry unchanged. The final method performs one preflight and preserves the event's manual flag, monotonic time and fresh count-request behavior. `profile-gate-after.log` is an after-only test; no before-failure execution is claimed.
7. **Late invalid-shortcut errors could overwrite a newer F9 state.** The UI error branch originally checked only a local response epoch. A peer actual DOM reproduction confirmed the race. The final branch also checks the native control epoch and latest request ID before applying the error or snapshot. Peer evidence is under `../adversarial/`.

## Final behavior matrix

The 15 tests include parameter matrices: F9/F2/F5 explicit completion retries with count ON/OFF; search-mode F9 with count ON/OFF and pause; conflict retry before/after Esc delivery; manual-entry combinations with scheduled initial click; saved hidden entry buttons; new CAPTCHA-option application.

Explicit completion retry now re-arms completion only after a new map observation (count OFF) or a fresh accepted count request (count ON). Old ACKs, old generations and Tick alone cannot trigger it. Earlier `after-core-checkpoint.log` and `after-core-matrix.log` used a stricter permanent completion latch; they are historical checkpoints, not evidence for the final explicit-retry policy.

## Limits

- Same bound window, dimensions/scales and calibration do not prove that a site still displays the same semantic performance or seat map. Automatic popup/new-window discovery remains outside this change.
- The tests inject delivery acknowledgements and count/failure observations separately. They do not prove server selection, OCR accuracy, popup attribution or actual key/mouse delivery.
- F1 starts a new admission run; compatible F2/F5/F9 preserve the current search history. Explicit F4 ends that history. Invalid new execution stops the predecessor instead of continuing its clicks.
- A native input already sent before a replacement command cannot be recalled. Existing epoch/interrupt checks cancel input not yet posted, and delivered input remains counted under the established policy.
- No app launch, live site operation, permission change, Windows execution or distribution build was performed by this reviewer. Existing release artifacts and user profiles were not modified.

No additional blocking defect was found in the final reviewed source. Broader workspace validation and release authorization are separate parent responsibilities.
