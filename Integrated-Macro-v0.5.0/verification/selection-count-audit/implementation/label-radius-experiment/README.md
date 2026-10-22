# Deferred label-radius performance experiment

Production source is unchanged. `source.json` records its hashes and the only
experimental behavior difference: with feature `heading_only`, heading retains
horizontal translation ±2px while empty/completion labels use ±1px.

The snapshot reader/layout and copied tests are actual reader code. Test import
paths select the experiment; fixture paths point to the preserved repository
corpora. No browser, app, input, profile or distribution build is involved.

The original focused corpora and snapshot private tests pass (20 tests,
1 manual timing test ignored). Separate optimized reader-only measurements are
in `current-radius-timing.log` and `heading-only-timing.log`, three samples per
synthetic3024×1608 frame. Median count3+map is295.6ms versus240.4ms; clear zero
panel+map305.2ms versus249.7ms. These are not total application latencies.

The Engine reviewer identified that `empty` is also negative evidence for a
numeric result, so false rejection is not the only possible regression.
The independent renderer supplied32 new count2+empty contradiction scenes in
`adversarial/radius-negative/`, varying empty font, size and fractional origin.
`contradictions-*.jsonl` records actual public-reader outcomes and
`comparison.json` separates new failures from existing failures:

- Both versions reject8 scenes.
- Neither version introduces a relative difference on this32-scene set.
- Both wrongly confirm2 in24 scenes, including common NeoRegular20/23/26 cases.
  This is an existing negative-evidence recognition defect, not a successful
  safety result for the proposed optimization.

The performance change is **not adopted**. The newly demonstrated existing
contradiction failures reopen the correctness audit; passing earlier finite
corpora does not establish readiness. Parent and independent reviewer were
notified immediately. Follow-up diagnosis must distinguish missed proposal
geometry from missing font templates before changing production recognition.
