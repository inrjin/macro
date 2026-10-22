# Vision allocation benchmark

The executable imports the repository macro-core crate and scans a synthetic 3024×1780 fully matching image five times. Its allocator measures additional outstanding heap allocations during each scan; timings use Rust Instant. This is an oversized-component stress case, not ticket-site latency or whole-app resident memory.

Before/after outputs: `../audit-vision-before.log`, `../audit-vision-after.log`. The retained pixel list was bounded while the complete component continues to be traversed and rejected. Accepted candidates remain unchanged; a large rejected component plus separate valid seat is a regression in `crates/macro-core/tests/vision_clock.rs`.

The Cargo manifest retains the original absolute repository dependency for traceability; change its path when reproducing elsewhere. Test executable compilation does not package or replace the application.
