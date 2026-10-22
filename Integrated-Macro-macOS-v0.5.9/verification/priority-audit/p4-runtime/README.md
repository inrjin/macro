# P4 internal-refresh capture audit

Four `runtime::priority_p4_tests` pass. `production-harness.log` is the targeted Rust run. This adds tests only; refresh behavior is unchanged.

The harness invokes actual runtime cache, pre-click adoption, timestamp/sequence fence, color validation, execution-option filtering, and Engine code. Small test glue dispatches returned observations and simulated refresh ACKs, matching `observe_frame` and `execute_actions`. Refresh generation/completion is never asserted by the fixture. The second pre-click native query is explicitly unchanged; P1 separately checks changed pixels, geometry, transparency and errors.

## Results

- With the configured 1,000 ms interval and no response/new image, ten simulated button deliveries occur across ten simulated seconds, never faster than the configured interval. The engine remains in Seats, runs eleven scans, emits no eligible seat and keeps generation zero. Only the registered internal button is emitted; no keyboard refresh is involved.
- Refresh OFF keeps scanning the static map without button input. Refresh ON keeps the observation/refresh loop running even if the image remains unchanged. Its candidate freshness barrier remains active until a qualifying image arrives.
- An unread higher sequence captured before the latest button post is blocked. A later valid image containing a real extracted candidate immediately permits the seat path.
- Identical pixels with a genuinely new image sequence/post-input capture time release the barrier. Idle-only unchanged metadata does not manufacture a new capture.
- A latest-response image and an older request response rendered after the latest post are indistinguishable to these pixel/timestamp observations. The model permits the same candidate in both cases and does not claim to identify a completed server refresh.

## Limits

No Worker/AppHandle, native driver, browser, network, wall-clock scheduler or physical input ran. Timing is deterministic simulated Engine time and ACK receipt; counts do not measure actual requests or successful refreshes. Native freshness establishes capture ordering, not website transaction identity or render completion. Actual capture errors/foreground guards are covered separately and are not bypassed by this model.
