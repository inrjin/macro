# P4 refresh replay review

The reviewer read `runtime::priority_p4_tests` alongside production `observe_frame`, the action delivery path, and the shared capture/fence helpers. No discrepancy affecting the declared scenarios was found. The owner reports four passing cases in `../p4-runtime/production-harness.log`; this reviewer did not rerun them.

The replay applies the actual execution-option normalization, extraction, Engine, cached capture decisions, and pre-click freshness/color helpers. Its delivery acknowledgements and native capture returns are scripted. After a refresh delivery, production and the replay record a sequence/time fence. Before that fence is satisfied, an empty candidate observation still drives the Engine's timed search loop. A disappeared refresh response therefore does not itself stop subsequent refresh proposals in these conditions.

The 10 button deliveries over 10 seconds use the explicitly configured 1,000 ms interval and synthetic time. They are not measured browser input, network request frequency, or successful refresh responses. Refresh OFF is separately checked against an unchanged map. The tests distinguish a delayed image captured before the latest input from a later image; pixel-identical fresh frames can satisfy the metadata fence.

The truth labels for a latest response and an older response rendered later are not provided to the algorithm. Identical runtime-visible inputs produce identical decisions, demonstrating that callback freshness does not identify the request answered. No successful refresh or reservation is inferred from this result.

The pre-click native query is explicitly unchanged; changed pixels, opacity, geometry and capture errors are covered by the P1 component tests. A constructed Worker, compositor timing, native input, the real browser, and network behavior were not exercised. The replay does not include every visual-dialog or context-recovery branch, so its passing assertions cannot certify those paths.
