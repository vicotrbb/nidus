# Performance

Nidus is designed to add minimal overhead over raw Axum.

Current benchmark targets:

```bash
cargo bench --workspace --all-features
```

Benchmarks cover:

- raw Axum baseline setup
- routing composition
- singleton dependency resolution
- Nidus hello-world setup
- Nidus controller setup
- Nidus controller + service setup
- guarded route checks
- validation pipe route input checks

Published results must compare against equivalent raw Axum code and document overhead honestly.
