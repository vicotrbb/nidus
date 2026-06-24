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
- routing composition
- singleton dependency resolution
- Nidus hello-world requests
- Nidus controller setup
- Nidus controller + service requests
- guarded route checks
- validation pipe route input checks
- request-scoped provider route checks

Published results must compare against equivalent raw Axum code and document overhead honestly.
