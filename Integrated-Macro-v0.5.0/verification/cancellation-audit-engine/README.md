# Cancellation engine audit

These standalone Rust programs link the compiled workspace `macro_core::Engine` and supply synthetic events. They do not extract or replace engine logic. No application, OS input, browser, capture permission, user profile, or release package was used.

`probe.rs` reproduces residual `automatic_refresh` and `region_priority` changing the legacy cancellation route. It asserts the observed baseline defect, so its passing output is reproduction evidence, not proof of a fix. Runtime-only mode normalization is reviewed separately.

`invariants.rs` checks confirmed/provisional retained seats, refresh ON/OFF, rejected-candidate retry, exact shortfall and stale acknowledgement/result exclusion (four combinations).

`long_run.rs` checks 1,000 repeated failures while retaining one seat for each refresh setting, including 200 cancelled-Escape pause/resume cases per setting; 1,000 empty-map cycles each with refresh OFF/ON, then a new seat; and 1,000 legacy zone changes with generation, color, ROI and completion fallback assertions. No additional engine invariant failure was found.

An initial long-run harness assertion scheduled an observation 1 ms before the configured interval. Correcting the harness start from 100 to 101 ms produced the retained passing output; this was not a product defect.

## Exact compiler setup

Commands ran from the project directory with:

```sh
export CARGO_HOME="$PWD/.build-tools/cargo"
export RUSTUP_HOME="$PWD/.build-tools/rustup"
export RUSTUP_TOOLCHAIN=stable
'/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/.build-tools/cargo/bin/rustc' --edition=2021 /private/tmp/macro-cancellation-engine-audit/probe.rs --extern 'macro_core=/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/target/debug/deps/libmacro_core-67a29fe26a635523.rlib' -L '/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/target/debug/deps' -o /private/tmp/macro-cancellation-engine-audit/probe
/private/tmp/macro-cancellation-engine-audit/probe
'/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/.build-tools/cargo/bin/rustc' --edition=2021 /private/tmp/macro-cancellation-engine-audit/invariants.rs --extern 'macro_core=/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/target/debug/deps/libmacro_core-67a29fe26a635523.rlib' -L '/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/target/debug/deps' -o /private/tmp/macro-cancellation-engine-audit/invariants
/private/tmp/macro-cancellation-engine-audit/invariants
'/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/.build-tools/cargo/bin/rustc' --edition=2021 /private/tmp/macro-cancellation-engine-audit/long_run.rs --extern 'macro_core=/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/target/debug/deps/libmacro_core-67a29fe26a635523.rlib' -L '/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/target/debug/deps' -o /private/tmp/macro-cancellation-engine-audit/long_run
/private/tmp/macro-cancellation-engine-audit/long_run
```

The compiler commands, linked library SHA-256, and inspected engine SHA-256 are preserved in this directory. These synthetic engine checks do not establish live browser/OS behavior or correctness of runtime capture fences.
