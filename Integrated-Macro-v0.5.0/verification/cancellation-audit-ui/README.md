# Cancellation UI audit

This audit exercised the production JavaScript and its existing VM DOM harness. It did not launch the app, send physical input, connect to a ticket site, or write user settings.

## Before-fix evidence

`before-reproduction.mjs` and `before-results.txt` retain these observed failures:

1. Selecting two-region clicks after arrow navigation, without registered region points, still sent `arrow_keys`. The returned profile changed the visible selection back to arrow navigation.
2. While F8 awaited the second region's color, an unsaved first-region color disappeared when the returned profile expanded `legacy.regions` from zero to two slots.
3. A legacy two-region profile without desktop capture metadata lost its navigation targets when switched to single-region mode and back. Its valid coordinates also displayed as unregistered.

The same DOM reproduction confirms that switching from internal-button mode to either legacy mode preserves the saved refresh and region-priority flags. Their execution scoping is a runtime responsibility; those settings must remain available when returning to their intended modes.

The reproduction imports current source files. It is historical evidence; its assertions for the old bugs should fail after the fixes.

## Corrected behavior and checks

- Fresh settings select current-map scanning. Explicit two-region mode requires both region targets for Start/F2, while Save and F8 setup remain available.
- Older navigation targets migrate to capture metadata before a mode change. Existing target and ROI values display without requiring that metadata.
- Only `legacy.regions` receives fixed-slot reconciliation. Other variable-length arrays retain their prior conflict policy; RGB, points and rectangles remain atomic values.
- Empty imported region vision objects are labeled as missing region bounds, rather than claiming that common bounds apply.
- The production DOM compares 32 starts across four modes, refresh ON/OFF, target present/missing, and button/F2. It checks identical outgoing options, common/region color separation, completion coordinates, and independent F8 edits.

Validation:

```sh
node --test ui/tests/cancellation-audit.test.js ui/tests/model.test.js ui/tests/profile-sync.test.js ui/tests/continuous-flow.test.js ui/tests/start-integration.test.js ui/tests/start-shortcut.test.js
```

Result: 77 passed, 0 failed. These tests verify JavaScript behavior and controlled IPC payloads, not native execution, live keyboard receipt, or ticket-site success. Runtime and engine checks are recorded separately.
