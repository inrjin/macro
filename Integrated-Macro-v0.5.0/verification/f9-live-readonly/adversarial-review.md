# F9 read-only adversarial review

## Conclusion and evidence boundaries

The user's report that F9 has never achieved the intended booking result is not contradicted by internal handler or native-input records. Parent's sanitized `observations.json` contains 28 `captcha_manual_completed` records: 27 from 0.5.11 and one from 0.5.10. The native shortcut is therefore not globally absent. Those records establish acceptance of a post-CAPTCHA handoff, not CAPTCHA recognition, successful seat recognition, successful site selection, or successful booking.

At 10:14:06–10:14:07, the same run 3 following the 10:13:51 F9 has 31 accepted object candidates, two delivered seat clicks and a delivered completion input. Consequently “F9 never sends any input” is also unsupported. `delivered:true` only establishes the native adapter's input result; count verification OFF does not prove the website accepted two seats. No screenshot or selected-seat/server evidence in this review establishes that success.

## Actual released callers

Released runtime is reviewed from `verification/calibration-retry/implementation/before/src-tauri/src/runtime.rs`, SHA-256 `2d4213c49584b930e2bdc5ed78fb2aaa2700479c7db7bb97cdf0d06377561939`. The working runtime has the later unbuilt calibration correction and must not be substituted for the installed 0.5.11 behavior.

- `src-tauri/src/main.rs:183` maps unmodified F9 to ContinueAfterCaptcha; lines 201 and 218 process Pressed events and register the shortcut. Local global-hotkey 0.8.0 uses Carbon RegisterEventHotKey and maps F9 to virtual key 0x65. This is separate from the NSEvent first-character monitor. Key registration or Fn issues may explain an individual missing attempt, but are not established as the cause of the recorded 27 accepted 0.5.11 handoffs.
- Released runtime lines 3660–3702 validate control ownership and supersede prior work before the F9 branch. It does not perform the UI Start button's browser activation. The bound browser still has to satisfy native foreground/window/geometry checks.
- Released runtime `continue_after_captcha` at line 4810 compares retained settings, creates an Observe-only post-entry Engine state, then checks screen/input permission, target, frame and geometry. Its final `captcha_manual_completed` record includes `recovering`; a transient validation failure may log this event while waiting for target recovery. Hard failure before that final record instead emits an error. Absence of this one event is not proof that the OS never delivered F9.
- `MacPlatform.swift:414` validates front PID, bound ID and native rectangle; another Chrome window or an occluding window can block actual input. It does not automatically infer that a new popup is the intended booking window.
- Released runtime `execute_actions` at line 4485 separately checks candidate availability, conflict state, optional count evidence and native cancellation. Its `input` record near line 4628 is the evidence for native delivery, not the earlier F9 record.

## Distinct observed failure conditions

Parent's contemporaneous metadata identifies these separate cases, which should not be merged into “bad F9 hotkey”:

1. 09:28 and 09:31 handoffs entered Region; the corresponding scans had no matching region pixels. Contemporaneous settings were RGB 124/104/239, ROI 174/506/1761/1247 and tolerance 24, per implementation review. Current saved settings must not be applied retrospectively.
2. 10:13 fresh handoffs entered Seats with no accepted object candidates/reference objects, followed by foreground waiting/recovery. Later frames did produce candidates and inputs. The screen content at the earlier zero-candidate frames is unknown.
3. 10:12 handoffs retained an already-satisfied count estimate and reissued completion. Engine source tests explain this: a new map observation releases freshness, and an already-met target is checked before choosing another candidate. This is neither proof of newly selected seats nor evidence that the UI was still in the same semantic booking state.
4. 09:42 has an explicit retained color/ROI/policy mismatch error. The unbuilt calibration correction addresses that guard and its restart-history lifetime. It does not solve missing region colors, all unsupported object layouts, automatic screen classification, or externally changed selections.

F9 uses retained history and the configured region-priority route, not a semantic detector deciding whether the current page is a region map or an individual-seat map. With no retained history and region priority ON it starts Region. If the user is already inside a seat map this could search the wrong color/ROI; without the historical image that remains a conditional explanation, not a proven scene classification for these events.

## UI and operational limitations

`ui/model.js:446` makes target recovery a waiting state with the reason shown; the main page receives runtime errors in `ui/app.js:1013`. `ui/overlay.js` displays normalized state and candidate information, but its visibility is controlled by released runtime `sync_overlay_visibility` at line 3169. With the overlay option OFF, ordinary F9 feedback is temporary (five seconds), error feedback is ten seconds, and a later candidate-empty/focus hold can lack a persistent visible indicator. Current overlay OFF cannot be assumed for every earlier attempt. Source checks do not establish what was actually visible to the user at those times.

Calling F9 during WaitingSale would supersede the current reservation and enter the post-entry path. No diagnostic F9 press, app focus change or settings change was requested or performed during the current reservation. The latest observed run 6 remains WaitingSale and has no F9 record.

## Cross review and scope

The Engine reviewer and implementation reviewer challenged the initial overgeneralization from zero-candidate events: later actual input exists. All reviewers distinguish handler acceptance, native delivery, and site acceptance; do not infer past settings from the current profile; and distinguish the unbuilt calibration correction from the installed release. Earlier phase/caller/shape synthetic tests do not provide end-to-end proof for the user's real booking page. F9 should not currently be presented to this user as a verified successful recovery method.

No source, user profile, app, older report, version, build or release was modified. No test, native capture, OS input, browser control, or live-site operation was run. Only this new review document was written. This does not claim a complete root cause for every historical F9 attempt.

## Read-source hashes

- `src-tauri/src/main.rs`: `931e1a176b937231dc8729bec5e081f7ef43b1e2a441b238da7325338aa3a17a`
- `verification/calibration-retry/implementation/before/src-tauri/src/runtime.rs`: `2d4213c49584b930e2bdc5ed78fb2aaa2700479c7db7bb97cdf0d06377561939`
- `crates/macro-platform/native/MacPlatform.swift`: `c865ccd7059f4e8c604e353c9909e8e563d91684f9e5510c69ffd8e5d4023ce7`
- `ui/app.js`: `baea51b1dbd2c172c65bde970d2e3eff42640e53d75ee62ab44d8bd6b7f60486`
- `ui/model.js`: `2351e89ce198302b34f13358c980734b26747dd43594a16b2f442480ee8d101e`
- `ui/overlay.js`: `00cc95eee61939c572556466139193b78b7f91df2f46c619b16da805fe239b41`
- `crates/macro-core/src/engine.rs`: `8444a110ed702236b0e094f13a3af95300fb70f747bdf9b6e7c032433467777d`
- `verification/f9-live-readonly/observations.json`: `b4a62bc429eb13ca7988f241a262f030c2125b07b48cb7af928bd880803ae3d1`
