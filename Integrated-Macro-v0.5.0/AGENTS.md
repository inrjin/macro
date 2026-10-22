# Repository Guidelines

## Project Structure

`crates/macro-core/` contains shared state machines and image recognition; `crates/macro-platform/` contains platform integration. `src-tauri/` hosts the desktop runtime, and `ui/` contains the interface. JavaScript tests live in `ui/tests/` and `tests/`; Rust integration tests live under each crate's `tests/`. Use `verification/` for reproduction evidence and review records. Preserve historical release packages outside this source directory.

## Development and Validation

- `npm test`: run JavaScript tests.
- `cargo test --workspace`: run Rust workspace tests; configure the repository's local Rust toolchain when necessary.
- `npm run dev`: launch the development app when interactive execution is authorized.
- `npm run build`: build the desktop application when authorized; this alone does not establish release readiness.

Match surrounding formatting and naming. Keep UI labels and diagnostic instructions consistent. Preserve configuration compatibility and optional feature defaults. Avoid unrelated formatting changes.

## Mandatory Multi-Agent Code Review

For every code change, assign separate agents to implementation, independent code review, and adversarial/regression review. Reviewers must inspect the actual code and affected callers, not merely the implementer's summary. They must exchange objections and challenge assumptions, including whether the change introduces failures elsewhere.

Review relevant state transitions, asynchronous races, cancellation and retries, normal booking and cancellation-seat searching, option combinations, coordinate transforms, platform differences, and latency. Scale the depth to the change while retaining independent review.

Record findings with file references, triggering conditions, consequences, and supporting evidence. The implementer must address each finding with a correction or a reasoned response. Reviewers must inspect corrections and resolve disagreements against code or reproducible evidence. Agreement alone is not proof of correctness.

Run meaningful checks for the changed behavior and affected regressions; add regression tests for demonstrated defects when useful. Repeat review and affected tests after substantive corrections. Do not stop because a preset review count was reached, or repeat unchanged checks without a new reason.

Before declaring completion, connect review results and test evidence to the final source revision or hashes. Unresolved blocking defects prevent a readiness claim. Report remaining limitations, untested operating systems, and missing live-site validation explicitly. If independent review is unavailable, disclose that the required review remains incomplete.

## Delivery and Collaboration

Code review does not itself authorize a distribution build or release. Follow the user's current authorization, including authorization already given, without requesting it again. Summarize behavior changes, verification, unresolved risks, and whether a build was produced. No Git history is available here; use concise imperative commit subjects if Git is introduced.
