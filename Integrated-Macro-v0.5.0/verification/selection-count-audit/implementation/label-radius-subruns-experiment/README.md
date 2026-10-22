# Final isolated radius comparison

This snapshot contains the negative-only textline sub-run correction, exact
rejected-pattern memoization and row deduplication. The only experimental
feature changes `find_label` horizontal translation: heading stays ±2 pixels,
while positive empty/completion labels use ±1. No score, zero authorization,
negative reference or contradiction search is removed.

`source.json` records the captured source hashes and literal experiment delta.
The latest public correctness tests include the 32 contradictory empty phrases,
the adjacent UI cases and the 1,800-seat dense progress control. The
`heading-only-tests.log` result is 23 passed, 0 failed, 1 timing test ignored:
17 public regressions and six private snapshot tests. This run is separate from
the final 17-test production run in `../after-final-negative-radius.log`.

`comparison.json` records three optimized pure-core samples per scenario. Median
count3+map was 283.931ms before and 243.518ms with heading-only expansion;
zero+map was 298.574ms and 252.964ms. Map-only was 157.415ms and 170.540ms, so
this experiment does not show every scenario becoming faster. The final reader
still costs more than the earlier approximately 190ms count3 baseline. Capture,
platform input, UI scheduling, server behavior and total booking time are not
included. No app or release was built.

The production reader subsequently adopted only this radius distinction. The
same negative-only defenses remain active, including for positive numeric
results whose empty-wording contradiction is missed by the old anchor matcher.
Earlier radius experiments, including the one exposing 24 pre-existing wrong
confirmations, remain preserved in their separate directories.
