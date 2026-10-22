# 0.5.10 release adversarial review

## Scope and current result

Production read-only review. Own 119-file pre-version inventory and the preceding 131-file selected-count audit both differ only in nine version-bearing files and README.md. Exact text checks confirm version replacements only; Cargo.lock changes only the three local packages and retains deranged 0.5.9. README documentation changes are separate. No new executable behavior was introduced during versioning. Independent package review completed without a packaging blocker.

## Evidence continuity

`adversarial-review.json` binds source hashes and previous final audit manifests. Nonround model/vision/shape/selection implementation remains as reviewed. Subsequent changes to engine, runtime, reader/layout/assets and count tests are covered by the newer selected-count audit, which reported 425 Rust and 150 JavaScript checks. These are prior source checks, not the new release build result.

## Configuration and resources

Bundle identifier `dev.integrated-macro.desktop`, CSP, capabilities, native permission code and storage code are unchanged. The three new optional features remain default OFF; saved settings can retain previous explicit choices. Stable identifiers do not guarantee macOS permission carryover under ad-hoc signing. The guide correctly avoids a blanket permission reset.

Both count glyph JSON banks use Rust `include_str!`; no external glyph file load is required. Tauri frontendDist is `../ui`, and native Swift is statically linked for the host target. Testing native ABI for both architectures does not make this app universal: the proposed package accurately says Apple Silicon arm64 and macOS 13+.

## Documentation review

F1–F9, feature labels, default OFF, log name and app status were checked against callers. One ambiguity was corrected: the top overlay is visible only when its separate option is enabled. README now distinguishes app status from that optional overlay. The package includes the referenced core verification records. Its verification/README.txt explicitly identifies the full test corpora and review history retained in the source repository.

## Limits and final package checks

No installed-app launch, actual input, authenticated site, Windows runtime, or live 0→1→2 count transition is claimed. The approx. 240–260ms figure is reader-only static synthetic timing; it excludes capture, input and server response. A displayed count does not prove server ownership.

The ZIP SHA-256 is `560901a742636165171de2739df467060c65cb7724e364243279d9610ab3757b` (5,010,843 bytes). CRC verification passed; no duplicate, absolute, parent-traversal or special-file entries were found. Its 18 distribution files match the source package by SHA-256 and Unix permissions, both when streamed directly and after an independent `ditto -x -k` extraction. The 24 AppleDouble metadata entries are separate from the actual files. The extracted app passes `codesign --verify --deep --strict`, retains the executable permission, and reports 0.5.10 / `dev.integrated-macro.desktop` / minimum macOS13.

All 67 protected previous-release/AGENTS entries retain baseline bytes and hashes; all 131 reviewed source files remain unchanged after the build. README, local fixtures, build logs/results, source manifest and selected-count audit summary were matched to their repository originals. `adversarial-review.package.json` records the extraction location and full file manifest.

The other reviewer’s actual Mach-O inspection confirms arm64, system-only dynamic dependencies, valid ad-hoc signing, and inclusion of all nine UI root assets. I read that inspection’s code and output: JavaScript/CSS/JSON compare exact decompressed bytes; HTML compares normalized parser tokens. This proves packaged payload presence, not WebView rendering.

The current release build result reports 425 Rust checks passed (five manual checks ignored), 150 JavaScript checks passed, native compile/host synthetic ABI checks passed, and unchanged source hashes. Those checks do not establish real input behavior or macOS13 runtime compatibility. No app was launched and no settings were modified in this review. No unresolved release-packaging blocker was found.
