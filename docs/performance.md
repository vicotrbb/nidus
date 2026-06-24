# Performance

Nidus is designed to add minimal overhead over raw Axum.

Current benchmark targets:

```bash
cargo bench --workspace --all-features
```

For a quick local validation pass with reduced Criterion sample sizes, run the
bench targets directly so Criterion flags are not passed to workspace binary
test harnesses:

```bash
cargo bench --bench dependency_resolution -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
cargo bench --bench routing -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
cargo bench --bench request_lifecycle -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
```

Benchmarks cover:

- raw Axum baseline requests
- raw Axum route composition
- routing composition
- singleton dependency resolution
- Nidus hello-world requests
- Nidus controller setup
- Nidus controller + service requests
- guarded route checks
- validation pipe route input checks
- request-scoped provider route checks

## Local Results

The table below is a local validation run, not a universal performance claim.
It was captured on 2026-06-24 with `cargo bench`, `rustc 1.96.0
(ac68faa20 2026-05-25)`, and `aarch64-apple-darwin` on macOS 14.5
(`23F79`). Publish-grade numbers should rerun the same targets on the release
machine and compare against equivalent raw Axum code.

| Benchmark | Central estimate | Local comparison |
| --- | ---: | --- |
| raw Axum baseline request | 688.24 ns | baseline |
| Nidus hello-world request | 658.03 ns | same shape as raw Axum in this run |
| Nidus controller + service request | 776.67 ns | about 1.13x raw Axum |
| Nidus guarded route | 959.56 ns | about 1.39x raw Axum |
| Nidus validation route | 2.0739 us | about 3.01x raw Axum |
| Nidus request-scoped route | 1.2117 us | about 1.76x raw Axum |
| Nidus controller setup | 283.25 ns | local setup microbenchmark |
| raw Axum route composition | 1.7466 us | startup/composition baseline |
| Nidus controller route composition | 5.5737 us | about 3.19x raw Axum composition |
| Nidus singleton dependency resolution | 23.15 ns | direct container lookup |

These results support the current design constraints: default request handling
does not resolve the dependency graph per request, request-scoped providers are
explicit opt-in overhead, and validation or guard layers add measurable Tower
middleware cost. Published results must compare against equivalent raw Axum code
and document overhead honestly.
