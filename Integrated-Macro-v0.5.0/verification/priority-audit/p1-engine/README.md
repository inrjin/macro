# P1 engine/vision boundary audit

These are passing reproductions of current limitations, not a fix or live-browser certification. No production engine behavior changed. The test feeds synthetic RGBA through the real core extractor and Engine; it does not supply preselected candidate IDs or successful server outcomes.

1. After the real legacy navigation ACK and configured delay, the engine assigns a new search generation. Old-map pixels still produce an old coordinate; the intended new-map control produces a different coordinate.
2. A new image can reflect only a loading indicator outside the seat ROI. Fresh capture sequence/time cannot by itself establish web navigation completion. This test proves the content counterexample; the runtime reviewer separately tests actual timestamp gates.
3. Identical pixels and capture/idle metadata can represent both a delayed old map and a completed identical map. Any decision using only that evidence must behave identically. Requiring image change can stall the valid static case; accepting it can permit the stale case. The metadata tuple is an evidence model, not an OS capture.

## Reproduce

Run from the project root:

```sh
CARGO_HOME="$PWD/.build-tools/cargo" RUSTUP_HOME="$PWD/.build-tools/rustup" RUSTUP_TOOLCHAIN=stable "$PWD/.build-tools/cargo/bin/cargo" test -p macro-core --test priority_audit --offline --locked -- --nocapture
```

Three tests passed. `metadata.json` records exact source hashes. `results.txt` preserves result messages (parallel test output ordering normalized). `priority_audit.rs` is a source snapshot; the runnable target remains `crates/macro-core/tests/priority_audit.rs`.

## Boundaries

No Chromium screenshots or live ticket page were used. The parent attempted isolated headless Chromium, but the sandbox rejected startup, so browser rendering/input remain unverified. Fixed waits, newer image numbers, and callback timestamps do not prove that a specific web section has completed loading. An explicit visual or user readiness signal would be a separate product decision.
