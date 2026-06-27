# Dependency Resolution Benchmark Result - Wave 46

This is a local Criterion result artifact for the Wave 46 benchmark evidence pass.

- Command: `cargo bench --bench dependency_resolution`
- Date: 2026-06-27
- Runtime code baseline: `0ea6861` (`test(bench): preserve request lifecycle result evidence`);
  Wave 46 adds docs, tests, and result artifacts only
- Scope: dependency resolution benchmark only
- Interpretation: local directional evidence, not a universal performance claim. Lower time is
  faster. Change intervals compare against Criterion's local saved baseline in `target/criterion`.

## Results

| Benchmark | Estimate | 95% CI | Change vs local baseline |
| --- | ---: | ---: | ---: |
| `nidus singleton dependency resolution` | 25.42 ns | 24.80-26.16 ns | -0.40% [-4.36%, +3.72%] |

## Notes

Criterion reported no detected performance change for singleton dependency resolution
(`p = 0.85 > 0.05`) and 7 outliers among 100 measurements. Treat this as local smoke evidence that
the saved benchmark surface still runs cleanly, not as release-machine baseline proof.
