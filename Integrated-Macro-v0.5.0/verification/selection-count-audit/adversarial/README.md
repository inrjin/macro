# Independent selected-count reader audit

Status: the final independent reader checks pass for the reproduced cases. No reproduced blocker remains in this corpus. This is not a universal OCR or live-site guarantee; support and timing limitations remain below. This is source/image validation, not a live booking, browser, accessibility, or OS-input test. No installed app, user setting, old distribution, or release ZIP was changed.

## Independent inputs and acceptance

Five CoreText generators render full scenes without reading production templates: `generate.swift`, `generate-extras.swift`, `generate-zero-controls.swift`, `radius-negative/generate.swift`, and `empty-neighbor/generate.swift`. Their manifests retain strings, requested/resolved fonts, sizes, placements and intended results. The frozen corpus has 220 RGBA scenes: 130 base, 26 extra zero-state negatives, 26 normal zero controls, 32 empty-message contradictions and six same-row UI controls. `all-fixtures-final.json` records every input hash and acceptance policy. The base generator's separate reproduction run is recorded in `generator-check.json`.

`crates/macro-core/tests/selection_count_audit_reader.rs` calls the public reader and adds 112 opaque white-strip occlusions to independently rendered 1/2/3/7 controls. Blue-pixel bounds only locate fixture mutations; they are not passed to recognition as ground truth. Forty-three required positive scenes must produce their exact count. Seventy-one required negatives must be unreadable. The other 106 scenes may be unreadable but must never substitute a different count. Rejection is not reported as recognition.

`final-adopted-corpus.log`, `final-adopted-performance.log`, and `source-final-adopted-before.json` bind the final runs. Nine ordinary tests pass; the separate manual timing test also passes when explicitly selected. Reader, layout, both assets and this test file are byte-identical before and after these final runs. `source-final-reviewed.json` also binds generators/manifests; `final-summary.json` contains exact machine-readable results. Earlier `final-corpus.log`/`final-performance.log` are clearly superseded checkpoints from before the later counterexamples were found.

| Scene group | Inputs | Correct count | Unreadable | Wrong count |
| --- | ---: | ---: | ---: | ---: |
| Held-out sizes/fractional placements | 50 | 46 | 4 | 0 |
| Held-out digit fonts | 40 | 26 | 14 | 0 |
| Gray/blue contrast | 20 | 20 | 0 | 0 |
| Uncut numeral controls | 4 | 4 | 0 | 0 |
| Nonwhite backgrounds | 4 | 0 | 4 | 0 |
| Normal zero scales/positions/colors | 26 | 26 | 0 | 0 |
| Original zero panels and dimmed negative | 3 | 2 | 1 | 0 |
| Extra zero contradictions | 26 | 0 | 26 | 0 |
| Misleading/duplicate headings | 9 | 0 | 9 | 0 |
| Empty-message font/scale contradictions | 32 | 0 | 32 | 0 |
| Nearby other UI, with/without empty message | 6 | 3 | 3 | 0 |
| Additional numeral occlusions | 112 | 36 | 76 | 0 |
| Procedurally rendered dense-map control | 1 | 1 | 0 | 0 |

All 43 required positives and 71 required rejections pass. Optional support is 84/106; its 22 unreadable cases are listed in `final-summary.json`. The dense-map control is additional to the 220 file fixtures: 1,800 solid gray seats, 15×13 pixels with horizontal gap 3, must not prevent reading a separate independently rendered count 3. The zero controls were measured against frozen earlier source: 24/26 originally read zero. An intermediate strict correction regressed six of those 24; the final checkpoint restores them and also reads the two previously unreadable controls. No zero-positive expectation was weakened to hide a regression.

## Findings, correction and counter-review

`before.log` and `source-before/` preserve initial behavior. `최대 선택 좌석 3` incorrectly returned 3 and `미선택 좌석 2` returned 2. Two visible headings in different fonts returned only the recognized heading's value. The corrected local word-boundary test and additional AppleGothic heading assets close these finite examples. They do not prove recognition of every possible heading or distant context.

All ten digits beside a fractional 23pt heading were initially unreadable. An independently measured 84×21 ink rectangle scored 0.80593 against an existing template. Anchor translation missed it. After translation correction, a higher-scoring rectangle one pixel too narrow caused the count-isolation check to reject genuine remaining heading ink. `diagnostic-origin.jsonl` separates correct digit score 0.87313 from that context failure. Bounded connected-ink correction restores the heading without lowering matching thresholds.

The low-contrast zero panel has actual outer RGB247 and white panel RGB255. The former edge tolerance of 8 erased this visible boundary. The final layout uses a separate edge tolerance of 3 while retaining continuous side-boundary, rounded completion-control and semantic-label checks. A whole white viewport and short divider remain unknown. No unobscured real zero-state screenshot is available, so this is synthetic support, not live-site proof.

`zero-extra-before.log` records six false zeros among the first 17 added negatives. `zero-extra-followup.log` extends this to same-color/near-white annotations and records ten false zeros among 26. An overly permissive label-padding rectangle and raw-background tolerance swallowed real extra ink. The current blank body requires exact flat background outside recognized labels and their immediate antialias fringe. A transitive padding flood is no longer allowed. A zero result also requires the numeric band to be blank even when small text did not qualify as a digit proposal.

Bounding correction alone then absorbed a tiny numeral attached to the final heading glyph. `annotation-pixel-differences.json` establishes that these are not byte-identical pictures: the black 7pt example adds 12 changed pixels outside the original bbox, including six previously white pixels. It is incorrect to dismiss this measured failure as a proven information-theoretic impossibility.

`edge-template-diagnostic.jsonl` tests a proposed alternative. Both genuine fringe ink and tiny added numerals fall on expected last-glyph template strokes, so template-edge presence alone does not distinguish them. That copied diagnostic is pre-final-guard evidence, not a final reader run. The adopted zero-only `empty_heading_bounds` allows at most one pixel per side and requires raw outer RGB to be no darker than the neighboring original pixel. `uniform_heading_ink`, the empty numeric band and strict panel content remain separate checks. This is a narrow raster-boundary rule, not proof of arbitrary textual meaning. The six previously regressed normal scenes and all 26 contradictory zero scenes now pass their original policies.

## Code review boundaries

Reviewed production paths: `find_label`, `possible_empty_line`, `complete_heading_bounds`, `empty_heading_bounds`, `isolated_count`, `standalone_heading`, `read_with_issue`, `label_scope`, `panel_geometry`, and their public diagnostic/result wrapper. The correction keeps the numeric score/margin unchanged and the original global missing-alpha/duplicate-heading/contradictory-empty checks. Diagnostics expose an enum reason, not recognized text, pixels, or an extra OCR pass. The implementation agent owns numeric-completeness fixtures and the parent owns runtime authorization/cache checks; their reports are separate evidence.

The reader still uses a finite font/template bank. Four nonwhite-background controls and 18 other font/size controls are unreadable. Uniform gray/blue headings work in the finite zero corpus, but Windows subpixel text rendering and real Windows execution were not tested. A tiny same-color glyph wholly within an accepted word, unsupported distant context, new fonts/layouts, arbitrary overlap and semantic reinterpretation remain unproven. The current fixes close the reproduced failures; they do not establish universal OCR correctness.

## Later blockers and their corrections

The proposed search-radius reduction prompted 32 additional independent scenes, stored separately in `radius-negative/`. They contain the same readable count 2 plus the contradictory empty phrase elsewhere, rendered at 20/23/26/32pt, four fractional phases, and Neo Regular/AppleGothic. The baseline and proposed implementation both rejected only 8/32 and incorrectly confirmed 2 in 24/32. Thus this is an existing negative-evidence failure, not a newly introduced performance regression. Equality between the versions is not a passing correctness result.

`radius-negative/before-production.log` reproduces the 24 failures through the actual public production API in the newly added integration test. An initial fixture-path mistake is retained separately in `initial-harness-path-error.log`; it is not counted as a product failure. `radius-negative/diagnostic.jsonl` and the copied `probe-empty-range/source.json` distinguish failures: exact-ink Neo20/23 rectangles score 0.7515/0.7660, below the unchanged 0.77 label threshold; Neo26 scores 0.7879 yet has no matcher proposal; Neo32 scores 0.9323 and is rejected correctly. Gothic20 scores 0.6621 with the existing bank. These are different coverage/proposal failures, not evidence for indiscriminately lowering thresholds.

The parent stopped the performance proposal until the false confirmations were addressed. A negative-evidence-only text-line recognizer first closed all 32 cases (`after-negative-corpus.log`). This was insufficient: `empty-neighbor/before.log` reproduces two further failures when a nearby same-row dark UI block or `닫기` text makes the greedy whole-line box too wide. The distant-block negative still rejected, and all three numeric-only counterpart scenes still read 2. These are unchanged positive/negative controls, not expectations weakened after implementation.

The final `possible_empty_line` examines bounded contiguous parts of candidate text lines, using 78 independently generated reference glyphs and the unchanged 0.77 full-word threshold. It runs only before numeric authorization and can only reject a count; its `empty_negative` assets are never used to authorize zero. Its conservative resource limit produces Unknown, not an assertion that contradictory text is absent.

A first substring implementation then exhausted its candidate budget on a normal dense map. `implementation/after-subruns-before-pattern-cache.log` preserves that regression. The final implementation memoizes only fully rejected exact `(width,height,binary pixels)` patterns, with full vector equality rather than hash-only identity. Scoring depends on those exact pixels/dimensions, and the memo is local to one call. Equivalent row work is deduplicated only by the complete row-selection key. Memory/work limits remain explicit; they do not establish a strict time deadline. The final dense positive, six neighbor controls, all 32 contradictions, all prior positives/negatives and occlusions pass together.

The subsequent radius change expands heading origin search to ±2 while empty/completion labels retain ±1. It is now verified with the negative-evidence correction in place, including the cases that invalidated the earlier performance-only proposal. No whole-frame heading uniqueness scan was removed.

## Final performance and remaining cost

`final-adopted-performance.log` runs the optimized test executable on static 3024×1608 scenes, three samples each. It passes its required count/unknown assertions. Capture, worker scheduling, native input and website response are excluded.

| Scene | Initial reader | Final source |
| --- | --- | --- |
| Blank, no label | 26.1–31.2 ms | 25.3–33.9 ms |
| 1,500 gray seats, no label | 119.7–132.3 ms | 159.3–164.1 ms |
| Count 3 plus map | 189.4–191.1 ms | 242.5–253.2 ms |
| Zero panel plus map | 200.9–204.1 ms, rejected low-contrast panel | 248.5–260.5 ms, correct strong-boundary panel |

The zero benchmark uses a stronger boundary than the initial rejected control, so that row is not an identical-input speed comparison. Count 3 is identical and remains approximately 27–34% slower than the initial reader. The heading-only radius is faster than the pre-final broad-radius checkpoint (287.6–306.0 ms), but does not erase the added validation cost. These are three local static samples on macOS 26.5.2 arm64, not a cross-platform latency guarantee or an end-to-end booking benchmark.

## Reproduction and limits

From the repository root with its local Rust toolchain, run `cargo test --release --offline -p macro-core --test selection_count_audit_reader -- --nocapture --test-threads=1`. The manual timing test additionally needs its name and `--ignored`. Ordinary tests contain no timing threshold. Do not regenerate fixtures while tests are reading them.

The real user count-3 screenshot is a separate parent-run control; its heading was included in the earlier template bank. This audit's rendered fonts/digits and mutations are independent fixtures but not every family is outside the bank. Correct website UI count remains distinct from server-side seat ownership. Reader success alone does not validate whole Worker/IPC behavior or remove the capture-to-input race. No deployment build was produced by this reviewer.
