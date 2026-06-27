# Request Lifecycle Benchmark Result - Wave 43

This is a local Criterion result artifact for the Wave 43 request-identity hardening pass.

- Command: `cargo bench --bench request_lifecycle`
- Date: 2026-06-27
- Commit under test: `b82df7e` (`fix(http): require trusted proxies for forwarded identity`)
- Scope: request lifecycle benchmark only
- Interpretation: local directional evidence, not a universal performance claim. Lower time is
  faster. Change intervals compare against Criterion's local saved baseline in `target/criterion`.

## Results

| Benchmark | Estimate | 95% CI | Change vs local baseline |
| --- | ---: | ---: | ---: |
| `raw axum baseline request` | 665.04 ns | 661.62-668.78 ns | -3.02% [-3.59%, -2.34%] |
| `nidus hello-world request` | 629.39 ns | 627.27-631.54 ns | -4.11% [-5.38%, -3.15%] |
| `nidus hello-world app` | 2.831 us | 2.820-2.842 us | -1.23% [-1.71%, -0.75%] |
| `nidus controller + service request` | 754.26 ns | 749.42-759.81 ns | -2.28% [-3.03%, -1.40%] |
| `nidus controller + service app` | 3.643 us | 3.626-3.662 us | -0.79% [-1.49%, -0.02%] |
| `nidus controller setup` | 268.17 ns | 267.09-269.28 ns | -0.52% [-1.12%, +0.05%] |
| `nidus guarded route` | 975.24 ns | 969.76-981.55 ns | -4.16% [-8.74%, -1.27%] |
| `nidus validation route` | 1.825 us | 1.817-1.834 us | -1.33% [-1.84%, -0.76%] |
| `nidus request-scoped route` | 1.263 us | 1.259-1.268 us | -2.09% [-2.64%, -1.50%] |
| `nidus middleware security headers request` | 965.08 ns | 961.01-969.47 ns | -2.25% [-3.06%, -1.58%] |
| `nidus middleware body limit request` | 827.13 ns | 824.29-830.17 ns | -5.39% [-7.81%, -3.75%] |
| `nidus middleware legacy request id request` | 1.457 us | 1.444-1.476 us | -1.48% [-2.55%, -0.02%] |
| `nidus middleware validated request id request` | 1.679 us | 1.593-1.797 us | +5.66% [+0.27%, +12.76%] |
| `nidus middleware request context request` | 1.308 us | 1.301-1.317 us | -2.91% [-3.56%, -2.22%] |
| `nidus middleware error envelope success request` | 1.018 us | 1.013-1.024 us | -3.42% [-4.24%, -2.65%] |
| `nidus middleware timeout response request` | 909.39 ns | 906.07-912.92 ns | -2.30% [-2.81%, -1.77%] |
| `nidus api defaults production request` | 3.719 us | 3.706-3.733 us | -1.27% [-1.72%, -0.81%] |
| `nidus api defaults production with metrics request` | 4.402 us | 4.378-4.430 us | -1.64% [-2.39%, -0.86%] |
| `nidus metrics record response` | 248.81 ns | 244.87-253.70 ns | +3.21% [+1.66%, +5.34%] |
| `nidus metrics record inner error` | 291.26 ns | 290.29-292.28 ns | +0.16% [-0.59%, +0.82%] |
| `nidus metrics render text` | 53.693 us | 53.385-54.085 us | +0.94% [+0.12%, +1.80%] |

## Notes

Most request and middleware scenarios were faster than the saved local baseline. The identity
hardening change did not touch metrics recording. The positive movements in
`nidus metrics record response`, `nidus middleware validated request id request`, and
`nidus metrics render text` should therefore be treated as local Criterion noise or residual risk
to watch in future runs, not as proven regressions from Wave 43.
