# P2 delayed-popup / retained-attempt audit

Three standalone scenarios compiled and executed successfully. These use synthetic seat pixels through production `extract_candidates`, production `Engine`, and the production `VisualRecovery` file included directly by Rust path. No native delivery, site request, browser rendering, or real CAPTCHA was executed. Production code was not modified.

## Results

1. **On-time latest-seat popup control passed.** Two actual pixel candidates are clicked (delivery is represented by ACK). The first is retained when the second popup appears. Actual visual samples trigger a `Suspected` signal, the signal produces Esc, unchanged popup pixels do not count as closure, restoration pixels do, and the next scan selects the third pixel candidate. An old queued completion ACK is inert.
2. **Delayed earlier-seat attribution limitation reproduced.** An independent fixture schedule declares request A failed late and B accepted. Those outcomes are never given to the application. After B delivery replaces the visual watch, the popup pixels generate a signal naming B. Feeding only that actual signal removes B while retaining A's optimistic estimate. The application lacks request identity in a generic popup image, so this is an assumption limit of its latest-attempt policy; it is not evidence that a real platform produces this ordering.
3. **Popup after delivered completion is outside the watch.** The test mirrors the runtime's documented successful completion reset, acknowledges completion, then supplies popup pixels. There is no remaining visual signal. This reflects the agreed end-of-selection scope; it is not automatically a defect to change without product approval.

## Fidelity and limits

The harness does not inject a pre-known failed seat ID. Candidate IDs come from the real image extractor, and failure identity comes from `VisualRecovery::observe`. The independent request schedule is used only to compare the expected owner with the detected owner. Successful server-side selections are not fed as Engine events.

Native ACK, the small runtime signal-to-Event dispatch, and successful-completion watch reset are modeled explicitly in the harness. This is not a complete Worker event-loop replay. The run covers single-threaded ordering; it cannot certify OS timing, popup rendering, actual refresh behavior, or real-site retained seats. Production runtime places observation turns between inputs, but no observation can identify a popup that has not yet appeared.

## Reproduce

`metadata.json` records the exact rustc command, environment-compatible linked core library path/hash, and source hashes. From the project root, build the core with:

```sh
CARGO_HOME="$PWD/.build-tools/cargo" RUSTUP_HOME="$PWD/.build-tools/rustup" RUSTUP_TOOLCHAIN=stable "$PWD/.build-tools/cargo/bin/cargo" test -p macro-core --test priority_audit --offline --locked
```

Then compile `visual_path.rs` as a normal Rust executable (not `--test`) with the recorded `--extern macro_core` and `-L target/debug/deps` arguments. Normal compilation includes the actual VisualRecovery implementation without running its unrelated internal tests. `results.txt` is the captured process output. An initial harness compile used a nonexistent convenience `RgbaFrame::view` method; the harness was corrected to construct `RgbaView` explicitly before this successful run. No product fix was required for that compile error.
