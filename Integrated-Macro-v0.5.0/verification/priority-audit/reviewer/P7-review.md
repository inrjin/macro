# P7 candidate-pixel conversion review

The pre-fix replay uses real extraction and the Engine to produce a candidate at pixel `(61, 5)`. At scale 1.75, dividing by scale and multiplying back before flooring reads `(60, 5)`, rejecting the actual matching seat. The scale-1 and scale-2 controls do not reproduce that numerical error. This is a demonstrated conversion defect, not evidence explaining the user's particular display configuration.

The reviewer independently read the new `image_candidate_local` helper and its production call sites. Sampling and selected-color checks use the captured pixel center. The macOS candidate click and blocked-point recovery use that same center. The action classifier limits this behavior to extracted `seat:` and `region:` actions; ordinary entry, manual legacy region navigation, refresh, completion, and saved fractional points retain their previous coordinate conversion.

Windows already adds a half-pixel during absolute-input normalization. The reviewer raised a double-centering counterexample: at virtual width 4,096, passing index 61 generates normalized coordinate 984, while passing center 61.5 generates 992. The owner's follow-up also checks the last pixel: double centering generates 65,536, which the existing driver's range check rejects, while the preserved index generates 65,528. The implemented native-Windows branch preserves the existing integer-index contract; sampling still uses the center. `p7-windows-center-arithmetic.json` records the independent source-arithmetic example. It explicitly uses an inverse-floor model and is not a measured Windows cursor position.

The owner's `after-fix.log` contains the targeted conversion results. This reviewer did not rerun that suite. The source preserves the shared conversion for delivery and blocked-point checking and leaves the native cancellation guards intact. No new blocker was found in this scope.

These are component and source-level checks. They do not execute macOS mouse posting, Windows `SendInput`, compositor scaling, or website hit testing. Physical click accuracy on either operating system remains unmeasured by this audit.
