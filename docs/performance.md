# Performance

Nidus is designed to add minimal overhead over raw Axum.

Current benchmark targets:

```bash
cargo bench --workspace --all-features
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

Published results must compare against equivalent raw Axum code and document overhead honestly.
