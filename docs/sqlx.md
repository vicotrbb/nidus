# SQLx

`nidus-sqlx` provides official SQLx adapter primitives for pool registration,
optional config loading, health checks, and observability hooks.

```toml
nidus-sqlx = { version = "1.0.7", features = ["sqlite", "nidus-config", "health", "observability"] }
```

Use `sqlite` or `postgres` to select the SQLx backend. Add `nidus-config` when
pool settings should come from Nidus config, `health` when readiness should
validate database connectivity, and `observability` when adapter-owned
operations should emit framework observability.

Nidus does not own your schema migrations, query design, ORM layer, or
transaction policy. Those stay in SQLx and application code.
