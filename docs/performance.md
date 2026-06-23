# Performance

Nidus is designed to add minimal overhead over raw Axum.

Current benchmark targets:

```bash
cargo bench --workspace --all-features
```

Benchmarks cover:

- routing composition
- singleton dependency resolution
- raw Axum route setup
- Nidus controller setup

Published results must compare against equivalent raw Axum code and document overhead honestly.

