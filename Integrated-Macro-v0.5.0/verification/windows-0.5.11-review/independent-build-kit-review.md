# Independent Windows build-kit source review

Reviewer: `/root/fix_cancellation_start`. Reviewed the actual staging CMD, PowerShell, workflow, README, manifests and file inventory. This is a source/caller review on macOS; neither PowerShell/CMD nor a Windows executable/installer was executed here.

## Conclusion

No additional blocking issue remains in the reviewed script path after the corrections below. This approves the scripts' stated intent and observed source structure, not a successful Windows build. The kit explicitly identifies itself as preparation material rather than an EXE or installer. Windows compilation, installation, WebView2 availability, key delivery, capture, focus recovery and site operation remain unverified.

## Findings and corrections

1. **MSVC environment was not initialized by the original workflow.** The PowerShell preflight required `cl.exe`, `link.exe`, `rc.exe` and `VSCMD_ARG_TGT_ARCH=x64`, while the initial workflow only prepared Node/Rust. Both independent reviewers identified the mismatch. The final CMD discovers installed Visual Studio through `vswhere`, invokes its `VsDevCmd.bat` with x64 host/target, checks the result, and then invokes the PS script in that child environment. The workflow now calls this same CMD. `setlocal` scopes changes to the process; no tool installation or persistent environment/policy modification was added.
2. **Partially verified output could have been uploaded.** The parent identified that the first EXE could exist in `artifacts/` before all later output checks finished. The final script uses `candidate-artifacts/`, completes all PE/hash/metadata collection, and only then moves the directory to `artifacts/`. The workflow's always-upload paths exclude the candidate directory.
3. **Windows Git line-ending conversion could invalidate release hashes.** The adversarial reviewer reproduced the mismatch. The final root `.gitattributes` sets `* -text`; README instructs preserving it and `.github/` when importing the kit. This preserves the supplied bytes rather than changing the immutable source manifest.

## Final source checks

- Source manifest requires exactly 138 release entries; fixture manifest requires 417 separate entries. The staging source tree was independently enumerated: exactly 555 regular files, no missing or unlisted files.
- Paths, duplicate/overlapping manifests and hash mismatches are rejected. Input-tree reparse points are rejected. Verified inputs are copied into a unique work directory, and Cargo output uses a unique target directory, preventing old EXEs from satisfying the build check.
- The immutable source tree is checked before and after building. Work-tree checks allow only the top-level installed `node_modules` dependency tree and the four explicitly listed Tauri-generated schema files; schema before/after hashes are recorded. The original source schemas are not changed.
- Dependencies use `npm ci`. JavaScript tests precede `cargo test --workspace --locked --target x86_64-pc-windows-msvc`, followed by the locally installed Tauri CLI with NSIS and `--locked`. Each external invocation rejects both missing and nonzero native exit codes. The target tests, if actually run on Windows, include Windows-only Rust tests; the macOS host results are not substituted for them.
- The main fresh executable must be a PE with x64 machine `0x8664`; NSIS setup executables must exist and have a recognized x86/x64 PE machine. Installer launchers may be x86 even when they install the x64 application. Artifact hashes, sizes, PE type, file/product versions and Authenticode status are recorded. Authenticode status is reported, not claimed to be signed.
- CMD propagates failure. PowerShell reports failure and exits nonzero; it does not launch the generated application or installer. The workflow is manually triggered, has read-only repository contents permission, and uploads output/evidence without publishing a GitHub Release.
- README distinguishes the unchanged 0.5.11 source from the unreleased calibration fix and states that source/test integrity is not Windows runtime validation.

## Remaining limitations

The local cross-target attempt failed with missing target `core` (`E0463`). No Windows SDK/MSVC build, PS parser/runtime execution, live OS input, or app launch has been validated in this environment. A successful future script run will still require inspecting its actual artifact receipt and performing authorized Windows runtime checks before claiming practical operation. Dependency locks constrain package versions, but no bit-reproducible PE claim is made across toolchain/SDK versions.

The reviewed code and documentation hashes, full 555-file staging integrity check, and review-document hash are recorded in `independent-build-kit-review.json`. Platform-specific limitations are separately recorded in `platform-review.md`.
