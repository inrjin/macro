# Repeated F8 target_occluded investigation

The user reports that the macro settings minimize, Chrome is visibly exposed,
and F8 still returns target_occluded, including after explicitly clicking Chrome.
The latest runtime log confirms repeated target_occluded after reconnecting; it
also contains geometry-change and missing-capture-target errors during earlier
reconnection attempts. None of the occlusion errors identifies the blocking window.

Read-only overlay audit found no concrete ID-lifetime, initialization ordering,
NSNumber conversion or status-window registration defect. The status NSWindow
is reused across show/hide. This does not prove actual runtime overlay behavior.

The native validator treats every nontransparent above-target window rectangle
containing the pointer as an occluder. Existing synthetic tests omit cursor-layer
windows. A Window Server cursor is a possible false positive, but it has NOT been
observed in the user's failing sample and must not be stated as the confirmed cause.
Apple documents CGWindowLevelKey.cursorWindow as the cursor level:
https://developer.apple.com/documentation/coregraphics/cgwindowlevelkey/cursorwindow

A read-only CGWindow metadata probe in the agent environment returned an empty
window list, unavailable foreground app and (0,0) pointer. This is an unavailable
inspection environment, not evidence that the user's window is missing.

scripts/diagnose-macos-windows.swift was added and passed swiftc -typecheck. It
waits five seconds, samples three times, and prints window owner/PID/ID/layer,
alpha, bounds and pointer metadata. It does not request permission, capture pixels,
read browser titles or URLs, or send input. The user has been asked to run it in
Terminal and return the output with Chrome's failing button visible under the mouse.

No speculative exemption or new release was made. Identify the real blocker
before changing validation and add a regression from its metadata. Keep true
occluders, other browser windows, app settings, menus and dialogs protected.

## Follow-up: user metadata, 0.5.3 correction

The second user run has Chrome foreground PID 19961. At
`(1129.640625,724.62890625)`, the first containing window is Dock ID 13,
PID 1397, layer 20, alpha 1, bounds `(0,0,1512,982)`. Chrome ID 123013
has bounds `(0,33,1512,891)`. A normal-layer Chrome helper ID 125397,
`(10,44,66,20)`, appears before the actual window. These exact values
reproduce the old rejection and first-window selection defects. No cursor
record is present. Other applications behind Chrome are irrelevant and not
retained as fixtures. The third point `(546.5625,981.98046875)` is outside
Chrome and must still be rejected.

The window list alone does not prove the Dock surface is noninteractive.
The correction recognizes only the system Dock bundle, dock layer, and exact
active-display bounds. It additionally requires a successful system-wide AX
hit test identifying the bound browser window by unique PID/geometry. Failure,
actual Dock hits, wrong windows, and subsequent occluders still block. F7 uses
AXFocusedWindow and a unique CG match instead of choosing the first normal
window. No private API is used. The terminal diagnostic's screen permission
flag is not interpreted as the macro application's permission state.

Public API basis:
[AX hit test](https://developer.apple.com/documentation/applicationservices/1462077-axuielementcopyelementatposition),
[focused window](https://developer.apple.com/documentation/applicationservices/kaxfocusedwindowattribute),
[containing window](https://developer.apple.com/documentation/applicationservices/kaxwindowattribute),
[bounded AX messaging](https://developer.apple.com/documentation/applicationservices/1459345-axuielementsetmessagingtimeout).

Regression fixtures inject AX outcomes into the production decision logic.
They do not demonstrate that the user's live Chrome exposes those outcomes.
Final compilation/test results are recorded in `local-checks.json`; actual
desktop capture, AX responses, and mouse input remain unverified here.
