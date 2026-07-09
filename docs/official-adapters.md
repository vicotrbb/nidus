# Official Adapters

Official adapters are separately installable crates. The facade stays lean, and
vendor dependencies enter the application only when the application chooses
that backend.

```toml
nidus = { package = "nidus-rs", version = "1.0.7", features = ["http", "config"] }
nidus-sqlx = { version = "1.0.7", features = ["sqlite", "health", "observability"] }
nidus-cache = { version = "1.0.7", features = ["moka", "health", "observability"] }
```

Adapters should register typed providers, expose health/readiness hooks when
useful, add observability at adapter-owned boundaries, and still leave direct
access to the underlying ecosystem client.

See [SQLx](sqlx.md), [Cache](cache.md), and the detailed
[integration contract](integrations.md) for the concrete adapter shape.
