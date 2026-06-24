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
- Nidus hello-world app construction
- Nidus controller setup
- Nidus controller + service requests
- Nidus controller + service app construction
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
| raw Axum baseline request | 628.93 ns | baseline |
| Nidus hello-world request | 608.36 ns | same shape as raw Axum in this run |
| Nidus hello-world app | 2.8517 us | app construction microbenchmark |
| Nidus controller + service request | 721.70 ns | about 1.15x raw Axum |
| Nidus controller + service app | 3.6173 us | app construction with DI setup |
| Nidus guarded route | 901.20 ns | about 1.43x raw Axum |
| Nidus validation route | 1.9878 us | about 3.16x raw Axum |
| Nidus request-scoped route | 1.1925 us | about 1.90x raw Axum |
| Nidus controller setup | 264.63 ns | local setup microbenchmark |
| raw Axum route composition | 1.7508 us | startup/composition baseline |
| Nidus controller route composition | 6.0412 us | about 3.45x raw Axum composition |
| Nidus singleton dependency resolution | 23.376 ns | direct container lookup |

These results support the current design constraints: default request handling
does not resolve the dependency graph per request, request-scoped providers are
explicit opt-in overhead, and validation or guard layers add measurable Tower
middleware cost. Published results must compare against equivalent raw Axum code
and document overhead honestly.
