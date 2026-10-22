# Independent release review — 0.5.10

Reviewer: `/root/fix_cancellation_start`. Final review completed on 2026-10-21. No blocking finding remains within the inspected build and packaging scope. This review did not launch the application or send operating-system input.

## Source and version continuity

The final selection-count audit's 131 source hashes were checked before the release metadata update. The actual update changes exactly nine version files and the separately approved README; the other 121 files remain byte-identical. Replaying the 13 authorized version replacements from the saved preimages reproduces all nine updated files. Cargo.lock changes only the three workspace package versions; dependency versions remain unchanged. Tauri's identifier, CSP, permissions, window configuration and feature defaults are unchanged apart from the main window's version title.

`independent-review-source.json` records this comparison. `independent-review-baseline.json` contains the original audited hashes but its observed metadata was collected after the concurrent version update began; it is explicitly not a byte-for-byte pre-update snapshot. The actual preimages are under `version-update/before/` and were checked against the audited hashes.

## Built application

The completed check report records JavaScript 150 passed, Rust 425 passed / 0 failed / 5 manual tests ignored, both native architecture compilation checks, host synthetic ABI checks, packaging and signature success. Both source-stability gates pass. Independently, all 131 pre-build audit files and all 122 extension-filtered build fingerprints still match the inspected sources.

Read-only inspection of the completed Mach-O and application bundle confirms:

- Both bundle version fields are `0.5.10`; identifier is `dev.integrated-macro.desktop`.
- The executable is thin `arm64`. Info.plist and the Mach-O load command both specify macOS `13.0` minimum.
- `codesign --verify --deep --strict` succeeds. The signature is ad hoc, with no TeamIdentifier or Apple notarization.
- Every dynamic dependency resolves by name to `/System/Library/` or `/usr/lib/`; no build-machine or Homebrew dependency is recorded.
- All nine top-level UI assets have exact compressed payloads present in the executable. Brotli decoding reproduces seven source files byte-for-byte. The two HTML files match parser tokens after whitespace/boolean-attribute normalization, and the index contains `0.5.10`. This establishes payload inclusion, not rendered webview behavior.

Reproduction script and full command output: `independent-review-artifact.py` and `independent-review-artifact.json`. Executable SHA-256: `53a848be4d714fb4276f7c85dc2f68fab0f41c82fb245a6b7619c17e9da48a3d`.

## Distribution and peer review

All four files in the distributed `.app` match the built bundle's hashes and file modes; the distributed app's signature also verifies. ZIP SHA-256 matches the distribution receipt: `560901a742636165171de2739df467060c65cb7724e364243279d9610ab3757b`. See `independent-review-distribution.json`.

`/root/input_adversarial_review` read the inspection script/results and found no blocker. Their separate ZIP CRC, fresh extraction, 18-file hash/mode comparison, extracted signature check and preservation of 67 historical files are recorded in `adversarial-review.package.json`. Those checks are attributed to that reviewer rather than presented as this reviewer's own extraction run.

## Limits

The delivered app is Apple Silicon only; compiling the native library for x86_64 does not produce an Intel application. Actual macOS capture/input/overlay, Windows execution, authenticated booking, permissions inheritance, and end-to-end latency remain unverified. Signature validity does not establish notarization or live functionality. Existing application packages, user profiles, permissions and historical audit records were not modified by this review.

Review documents and inspection artifacts are frozen; no production source was changed by this reviewer.
