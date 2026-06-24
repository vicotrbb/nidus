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

The table below is a local quick validation run, not a universal performance
claim. It was captured on 2026-06-24 with `rustc 1.96.0` on
`aarch64-apple-darwin` / Darwin 23.5.0 using the reduced Criterion commands
shown above. Publish-grade numbers should rerun the same targets with longer
measurement windows on the release machine.

| Benchmark | Central estimate | Local comparison |
| --- | ---: | --- |
| raw Axum baseline request | 640.85 ns | baseline |
| Nidus hello-world request | 622.01 ns | same shape as raw Axum in this run |
| Nidus controller + service request | 734.96 ns | about 1.15x raw Axum |
| Nidus guarded route | 906.63 ns | about 1.42x raw Axum |
| Nidus validation route | 2.0153 us | about 3.14x raw Axum |
| Nidus request-scoped route | 1.1494 us | about 1.79x raw Axum |
| Nidus controller setup | 265.27 ns | local setup microbenchmark |
| raw Axum route composition | 3.1499 us | startup/composition baseline |
| Nidus controller route composition | 8.3362 us | about 2.65x raw Axum composition |
| Nidus singleton dependency resolution | 145.82 ns | direct container lookup |

These results support the current design constraints: default request handling
does not resolve the dependency graph per request, request-scoped providers are
explicit opt-in overhead, and validation or guard layers add measurable Tower
middleware cost. Published results must compare against equivalent raw Axum code
and document overhead honestly.
