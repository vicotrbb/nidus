# Routing Benchmark Result - Wave 46

This is a local Criterion result artifact for the Wave 46 benchmark evidence pass.

- Command: `cargo bench --bench routing`
- Date: 2026-06-27
- Runtime code baseline: `0ea6861` (`test(bench): preserve request lifecycle result evidence`);
  Wave 46 adds docs, tests, and result artifacts only
- Scope: routing benchmark only
- Interpretation: local directional evidence, not a universal performance claim. Lower time is
  faster. Change intervals compare against Criterion's local saved baseline in `target/criterion`.

## Results

| Benchmark | Estimate | 95% CI | Change vs local baseline |
| --- | ---: | ---: | ---: |
| `raw axum route composition` | 1.809 us | 1.793-1.826 us | -4.72% [-11.80%, +0.82%] |
| `nidus controller route composition` | 5.737 us | 5.682-5.800 us | +0.61% [-1.91%, +2.77%] |

## Notes

Criterion reported no detected performance change for both routing scenarios. The raw Axum baseline
had 9 outliers among 100 measurements; the Nidus controller route-composition benchmark had 6
outliers among 100 measurements. Treat this as local smoke evidence that the saved routing benchmark
surface still runs cleanly, not as release-machine baseline proof.
