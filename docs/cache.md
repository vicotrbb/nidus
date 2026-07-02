# Cache

`nidus-cache` provides official cache adapter primitives, including Moka-backed
cache modules.

```toml
nidus-cache = { version = "1.0.5", features = ["moka", "health", "observability"] }
```

Use `moka` for the default async cache backend. Add `health` when readiness
should expose cache-state checks, and `observability` when adapter-owned
operations should emit framework observability.

Nidus does not decide cache keys, TTL policy, invalidation semantics, or data
consistency guarantees. Those remain application architecture decisions.
