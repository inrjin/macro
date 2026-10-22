# Selection-count reader implementation audit

The reader corrections are frozen at `source-final.json`. This audit changes the
shared Rust reader, its empty-panel check, a heading template supplement and
a separate negative-only text template bank.
It does not run a browser, send input, change profiles or produce a distribution.
The later negative-text follow-up and its radius experiment are included below;
the earlier provisional final reports remain preserved as `*.fringe-stage`.
Historical verification directories and distributed apps are not edited.

## Reproduced failures and corrections

1. **Truncated compound count.** The old component-size filter discarded signs,
   decimal points, small suffix digits and a second digit beyond the proposal
   band. `before.log` records 30 wrong confirmations among 51 compound-expression
   inputs, while twelve ordinary number controls remained readable. For example,
   rendered `12` at gap32 became count1. The parent separately connected the
   preserved original reader to the unchanged real Engine and demonstrated an
   inappropriate target1 completion authorization. The new `isolated_count`
   checks all binary ink in the relative numerical band, including components
   too small to be numerical proposals. The evidence rectangle covers that band.

2. **Heading substring and missed duplicate.** The independent corpus found
   `최대 선택 좌석 3` and `미선택 좌석 2` accepted as the selection total, and a
   second AppleGothic heading missed beside a Neo heading. A blank immediate
   left word boundary now rejects the reproduced prefixes. Eight AppleGothic
   heading templates supplement the original assets. The original 287 templates,
   including every numeral, empty label and completion label, are unchanged.
   This is not a general semantic parser for remote qualifiers or all fonts.

3. **Fractional glyph proposal.** A 23px heading was readable at its actual ink
   bounds, but anchor rounding missed the word origin. Heading horizontal translation
   refinement is now ±2px; empty/completion positive matching retain ±1px.
   The separate numeric contradiction check is described below. Both keep the
   original recognition thresholds. A connected-ink completion bounded to two
   pixels repairs a slightly cropped matched heading for normal digit reading.
   The attempted broader width search was not retained.

4. **False zero from overlooked content.** Independent cases placed small
   nonzero numerals beside the heading, or faint wrapped numerals in the panel.
   These are not blank content. The zero branch checks the otherwise-empty
   numeric band, exact flat panel background outside recognized labels and
   directly adjacent one-pixel label fringes. Rectangular free padding and
   transitive fringe flooding are not permitted. Same-panel header, explicit
   empty label, bounded rounded control and its completion label remain required.

5. **Zero heading absorption versus normal raster edges.** Merely using the
   original matched heading closed the tiny-number cases but regressed six
   previously readable fractional zero controls. Conversely, unrestricted
   connected-ink completion swallowed an attached tiny black numeral. The final
   zero-only `empty_heading_bounds` permits at most one pixel per side, and all
   raw RGB pixels in the extension must be no darker in any channel than their
   nearest original glyph pixel. This supports the observed fading raster edge
   and rejects a new stronger outward stroke. A uniform ink-direction check also
   rejects the reproduced different-colored attached numeral. Normal numeric
   recognition keeps its separate connected-glyph completion. These are local
   image checks, not proof of a word's meaning in every possible typography.

6. **Low-contrast panel progress.** Continuous outer panel-edge detection now
   distinguishes a channel difference greater than3, separately from button
   fill checks and the exact-background body rule. The original 90% continuous
   edge requirement, ownership geometry and recognized button remain intact.
   The independently rendered RGB247 exterior / white panel now reads zero;
   unbounded white pages and short dividers remain unreadable. Gradients or
   extra explanatory content are not silently treated as a clear empty panel.

7. **Missed contradictory empty wording.** A proposed radius optimization led
   to32 additional independent count2+empty scenes. Both experimental variants
   wrongly confirmed2 in24 scenes. Actual-bounds diagnosis separated anchor
   proposal failure (Neo26), scale/phase fit (Neo20/23) and missing font family
   (AppleGothic). The defect was also connected to actual Engine completion by
   the parent; it was not counted as a successful equivalence check.

   `possible_empty_line` now runs immediately before authorizing a numeric
   result. It proposes actual component textline bounds and compares them with
   the unchanged0.77 whole-word score against separate `empty_negative` assets.
   These78 references are rendered with NeoRegular, NeoMedium and AppleGothic
   at16–40px in2px steps, at whole/half-pixel phases. They contain no fixture
   crops. Their matches only withhold a numeric authorization; they cannot
   authorize zero, alter the zero font bank or create another positive result.

   A greedy maximal line alone still missed empty wording followed by nearby
   UI. Two independent adjacent block/text cases reproduced that failure; far
   noise and no-empty controls separated correct rejection from lost progress.
   The final helper inspects contiguous bounded sub-runs. Its width early-stop
   uses the maximum possible row height, so a later taller glyph is not wrongly
   excluded. A naive sub-run scan exhausted its budget on the independent
   1,800-seat dense map; that intermediate failure is preserved. Exact local
   `(width,height,binary pixels)` memoization restores the normal count3 result.
   HashSet equality compares actual bytes, not only a scalar hash. Only patterns
   that failed all negative references are cached, and the cache is call-local.

   Identical `(top,bottom,seed height)` rows are examined once: the row set,
   component-height filter and join distance have exactly those inputs. Limits
   remain explicit:2,000,000 row/window visits,128MiB of pattern-pixel inspection,
   1,024 unique rejected patterns and4MiB stored pattern bytes. Exhaustion means
   unreadable numeric context, never absence of contradictory wording. Very
   complex unsupported scenes may therefore pause count authorization.

## Diagnostics and call contract

`diagnose_selection_count` returns `CountDiagnosis { read, issue }` from the same
single analysis. `read_selection_count` is its compatibility `.read` wrapper.
The issue enum distinguishes invalid frame, missing opaque pixels, absent or
ambiguous heading, unsupported heading context, incomplete numeric context,
unrecognized digit, unsupported empty-panel layout and missing count. It emits
no recognized strings, image pixels or private information. Unknown never turns
into zero. The parent owns runtime cache/message integration and input-time
proof revalidation; this directory does not claim a whole Worker/OS test.

## Final focused checks

`after-final-negative-radius.log`: 17 tests passed, 0 failed, 1 manual timing test
ignored, using the three integration test files `selection_count_completeness`,
`selection_count_audit_reader` and `selection_count_reader_review` in optimized
core mode. This is not an app build. `focused-result.json` separates recognition
coverage from correctness:

- 82 implementation-rendered inputs, including normal numbers, compound inputs,
  unrelated controls, diagnostic causes and twelve normal empty-panel variants.
- 220 independently rendered scenes:130 base scenes,26 extra zero negatives,
  26 normal zero controls,32 contradictory-wording scenes and6 nearby-UI
  controls. All32 contradiction scenes and both nearby-UI contradiction cases
  are unreadable; the no-empty controls retain count2. The separate dense-map
  control retains count3 with1,800 close-spaced seats. All26 zero negatives are
  unreadable; all26 normal zero controls read0. Unsupported fonts still produce unknown in18 typography
  scenes; these are coverage limits, not incorrect numbers or successful reads.
- 112 opaque white-strip numeral occlusions:36 retain the correct count,
  76 are unknown, and none becomes a different count. This finite mutation set
  is not a guarantee for all possible occlusions.
- All108 historical reader scenes retain their documented mandatory results.
- The diagnostic test covers all nine issue variants and compatibility-wrapper
  equivalence. The independent correctness tests use rendered text, not pasted
  production template masks, and mandatory positives prevent reject-all success.

Run from the source root with the repository toolchain:

```sh
CARGO_HOME="$PWD/.build-tools/cargo" RUSTUP_HOME="$PWD/.build-tools/rustup" PATH="$PWD/.build-tools/cargo/bin:$PATH" cargo test -p macro-core --release --offline --locked --test selection_count_completeness --test selection_count_audit_reader --test selection_count_reader_review -- --nocapture
```

## Reproduction and evidence provenance

`generate.swift` renders whole strings with CoreText and records resolved fonts
and text geometry. `generate-heading-supplement.swift` renders only the missing
heading font family into separate template assets. Recreating both into
`/private/tmp` yielded byte-identical82 RGBA fixtures plus manifest and the exact
final template asset; see `reproduction-check.json`. The negative-only generator
was independently rerun into a temporary path and produced the exact final
78-template asset; see `negative-reproduction-check.json`. The preserved original
287-template prefix is byte/structure-equivalent in the final asset.

`probe-origin/` and `probe-zero/` contain instrumented snapshots used for diagnosis,
not production calls or final-version tests. `zero-heading-probe2.jsonl` records
why the observed fringe and added dark stroke differ. Intermediate failed tests
and compile logs are intentionally retained. Only the final source hash and
final passing log establish the final focused result. `*-final.diff` compares
the final implementation against the preserved audit baseline.

`label-radius-subruns-experiment/` preserves the last isolated radius comparison
with all final correctness cases. It passed23 tests (17 public regression tests
plus6 private snapshot tests) and separately measured four static large scenes.
For the same negative/sub-run logic, count3+map median fell from283.9 to243.5ms;
zero+map from298.6 to253.0ms (three samples each). Production then adopted only
the heading-specific radius change and passed the17 public tests again. These
reader-only results remain slower than the original pre-audit count3 baseline
of about190ms: correctness checks and broader supported typography have a cost.
The final independent production reader measurement was 242.5–253.2ms for
count3+map and 248.5–260.5ms for zero+map, consistent with the direction of the
copy experiment. These are not total UI/site timings. Supplied-screenshot
verification is recorded separately by the parent.

The independent reviewer owns the final reproducibility/performance report and
source review; the Engine reviewer also checked the actual zero/numeric callers.
The parent owns final workspace/runtime/UI verification and supplied-screenshot
rechecks. Optimized synthetic reader timings exclude capture, UI scheduling,
input delivery and server response; no total ticketing latency is promised.

## Remaining limits

This remains a narrow image reader for a supported NOL label/layout. A visually
read total is neither server-side ownership nor attribution to a particular
attempt. Unknown font/layout, very faint or overlapping ink, semantically
qualified labels outside the checked boundary and occlusion can still defeat
recognition. A separate button or price inside the numerical band causes a
conservative unknown; controls outside that band are positive controls here.
The zero layout deliberately requires a flat clear panel, not arbitrary text
arrangements. Screen-edge-clipped numerical context is unknown. The immediate
word-boundary and monotone-fringe rules are not a general language or glyph
meaning proof. Shared Rust behavior is exercised on synthetic RGBA images;
Windows capture, native input, live-site behavior and end-to-end timing are not
validated by this audit.
