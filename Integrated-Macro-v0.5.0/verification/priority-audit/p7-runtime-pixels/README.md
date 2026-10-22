# P7 fractional pixel conversion

## Confirmed failure

`before-fix.log` records a failing test using actual vision extraction, Engine seat action creation, and runtime pre-click color validation. A three-pixel component selects pixel (61, 5). At scale 1.75, the old `(61 / scale) * scale` falls below 61 and `color_at` reads pixel 60, rejecting the valid candidate. The same fixture passes at scales 1 and 2. This is not evidence about the user's current scale-2 capture incident.

## Minimal change

Only image-derived `seat:` and `region:` actions use the new shared conversion. Integer pixel indices use `OwnedFrame.pixel_to_local` for color sampling, selected-color checks, macOS native input and blocked-point recovery. The recovery structure retains only the point's image-candidate origin flag; no user profile schema changes.

General entry, legacy region navigation, refresh and completion preserve their existing coordinate division. Fractional point values also retain the prior division. Out-of-image coordinates and invalid scales are rejected.

Windows capture supplies physical pixels at scale 1, and its native input driver already adds half a pixel when normalizing to the desktop. The helper therefore preserves integer indices for Windows native delivery/recovery, while sampling the center of that same pixel. A naive universal center change would double-center: for a virtual desktop width 4,096, x61 produces 984 with the current contract but 992 if centered twice. At the last pixel, double centering yields 65,536 and hits the driver's range rejection; the preserved input yields 65,528. These are source-formula checks, not physical SendInput measurements.

## Validation and limits

Four P7 tests cover the real extraction/action/color failure, Windows normalization including a negative desktop origin and last pixel, first/last pixel round trips, invalid positions/scales, fractional point compatibility, action classification and selected-color sampling. `final-targeted.log` reruns all P1/P4/P7 runtime tests together (14 tests).

`runtime-production-from-audit-baseline.diff` shows production changes from the saved cancellation-audit baseline, including the P1 behavior-preserving extraction. The isolated extraction diff remains in `../p1-runtime/` for comparison. No region/refresh transition, interruption or popup policy was changed by this pixel correction.

No full Worker/AppHandle, physical browser, OS input, permissions change, app launch or release build occurred. The macOS Rust target compiled during tests. Windows runtime target compilation and native input were not executed; its explicit Boolean conversion branch and existing normalization formula were tested on the host. Native foreground/hit testing and display hardware behavior remain unverified by this harness.
