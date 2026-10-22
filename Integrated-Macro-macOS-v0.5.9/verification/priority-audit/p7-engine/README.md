# P7 pause, context-loss and Stop matrix

Thirteen normal-contract scenarios passed using production Engine, production candidate extraction from synthetic RGBA, and the directly included production VisualRecovery implementation. No production engine code changed. The tested retained seats are acknowledged input **attempts**, not server-confirmed reservations.

## Scenario matrix

| Stage | Control | Expected and observed | Cases |
| --- | --- | --- | --- |
| First seat queued, or first ACK followed by second queued | Pause / ContextLost | Cancel unsent ID first; old ACK inert; retained attempt unchanged; resume waits for a candidate observation and queues only the remaining seat | 4 |
| Popup from actual visual sample; Esc queued or already ACKed | Pause / ContextLost | Unsent Esc may be reissued once after resume; delivered Esc is not repeated; stale completion ACK inert; restoration pixels release search; retained first attempt and rejection remain | 4 |
| First/second/completion/Esc queued, or completion already ACKed | Stop | Resume/Tick/old ACK cannot revive stopped run; a new run clears attempt estimates and rejects old ACKs | 5 |

The popup path consumes only the failure identity emitted by the real `VisualRecovery` signal. It does not inject an oracle failed-seat ID.

## Adapter contract

The harness deliberately models the actual runtime ordering: `cancel_queued_actions` emits ActionCancelled before Pause/ContextLost; Stop discards the queue and resets the visual watcher; restored images emit ConflictDialogGone. These are small copied dispatch steps, not a complete Worker simulation. Native input delivery is represented by ACKs. The matrix verifies engine responses under this documented adapter contract; it does not prove the OS obeys an interrupt, or that a successful native post selected a ticket.

## Reproduce and evidence

`metadata.json` records exact rustc arguments, linked core library SHA256, and production/test source hashes. Compile `interruptions.rs` as a normal executable with `--extern macro_core=<recorded library>` and `-L target/debug/deps`, using project-local Rust environment (`CARGO_HOME=.build-tools/cargo`, `RUSTUP_HOME=.build-tools/rustup`, `RUSTUP_TOOLCHAIN=stable`). Do not compile with `--test`; the module's existing unit suite is outside this standalone matrix.

`results.txt` contains the captured execution output. `compile.txt` contains compiler output. The executable was created only in `/private/tmp`. No release build, app launch, OS input, browser page, user configuration, or permission change occurred.
