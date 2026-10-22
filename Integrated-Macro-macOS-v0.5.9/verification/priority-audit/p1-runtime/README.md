# P1 runtime capture-boundary audit

The six `runtime::priority_p1_tests` pass using production helpers, actual `OwnedFrame` buffers, candidate extraction, and the real Engine. `production-harness.log` records the run. The pre-extraction SHA matches `cancellation-audit-source-freeze.json`; `runtime-production-extraction.diff` isolates the behavior-preserving production extraction and `runtime-full.diff` includes tests. Runtime transition policy is unchanged.

## Reproduced conditions

- Original two-region and arrow-key navigation can produce an eligible seat from a cached pre-input image, including an unread newer sequence captured before the input. The second native query is explicitly scripted as unchanged in this reproduction.
- A hypothetical strict new-pixel fence blocks a legitimately identical destination indefinitely: no scan progress through an hour of simulated engine time. This test-only policy is not installed.
- Fresh idle metadata and fresh image callbacks can correspond either to a rendered identical destination or to the old region while the website is still loading. Runtime-visible evidence is identical. Native freshness does not identify the displayed region.
- The actual shared pre-click capture handler accepts newer matching pixels, rejects a removed or alpha-249 seat, and returns errors for changed geometry or capture failure. Rejected candidates are cancelled before any simulated delivery.
- Cancelled/non-delivered region input creates no new region fence; legacy navigation keeps its existing policy.

## Limits

This is a scripted native-return/production-component harness, not a constructed Worker or physical browser run. Native validation, compositor scheduling, foreground hit testing, and OS input delivery were not executed. The fixture's destination identity is a test oracle and is never supplied as a production event. No actual selection success is asserted. A fresh callback can still contain an old website view, so neither a new sequence nor a post-input timestamp proves transition completion.

The current legacy mode retains its timed current-screen assumption. These results characterize that limitation; they do not claim it is fixed. Adding a strict fence alone would introduce the separate static-screen stall reproduced here.

## Deferred P7 hypothesis

A fractional pixel scale such as 1.75 may round `(pixel / scale) * scale` below the original pixel when `OwnedFrame.color_at` floors it. The current extraction preserves that expression. P7 will test actual candidate extraction, click-time validation and pixel-center conversion before any policy change; this is not evidence about the user's current macOS scale-2 incident.
