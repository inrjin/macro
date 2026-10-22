# Exact threshold comparison review

The original multiplication bound rejects `I=55, U=100, remaining=0, minimum=.55` because `.55*100` rounds above55, while the existing final division score is exactly equal to the floating threshold.

The parent fixed this before the private-function copy was captured. `main.rs` is the verbatim corrected scoring source with minimal types, and `old-comparison.rs` reconstructs only the exact earlier comparison shown in the independent source review. `source.json` records this distinction; neither probe invokes the full matcher or Worker.

The corrected bound `(intersection+remaining)/(union+remaining)` is the maximum possible completion score: all remaining expected ink matches, all remaining background is empty. The division uses the same rounding form as final scoring. Thus it cannot reject a prefix whose maximum could reach the acceptance threshold. This preserves threshold eligibility; it does not establish reader accuracy on unseen fonts or layouts.
