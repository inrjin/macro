# P6 selection oracle review

Reviewed `crates/macro-core/tests/priority_selection_oracle.rs` independently; this reviewer did not alter it or rerun its three scenarios.

The site's selected set is independent of the Engine's candidate IDs and estimated count. Site clicks are hit-tested against its own seat rectangles. Its selected set controls the subsequent RGBA image. The production extractor and Engine receive pixels/candidates and delivery acknowledgements, with no `SelectionObserved` or successful-selection truth injected.

Accordingly, the accepted-click control and silent-failure counterexamples test the chosen optimistic counting policy: successful delivery can produce a completion proposal even when the independent site retains fewer seats. The first-click-ignore and second-click-clears-previous policies are distinct from visible failure-dialog recovery.

The test stops when `CompleteSelection` is proposed. It does not prove completion-button delivery, a successful reservation, or payment. It also omits the full runtime's pre-click capture, visual-dialog monitoring, native input and browser behavior. Within that declared Engine/vision scope, no oracle leakage or circular success assertion was found.
