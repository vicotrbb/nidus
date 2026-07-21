# Installation

Install the Nidus CLI from crates.io:

```bash
cargo install cargo-nidus --version 1.0.15
cargo nidus new hello-nidus
cd hello-nidus
cargo run
```

During local framework development, install directly from this checkout:

```bash
cargo install --path crates/cargo-nidus
cargo nidus new hello-nidus
```

Applications depend on the facade crate and opt into feature groups explicitly:

```toml
[dependencies]
nidus = { package = "nidus-rs", version = "1.0.15", features = ["http", "config", "openapi", "validation"] }
```

Official adapters are separate crates, so the core facade stays lean:

```toml
nidus-sqlx = { version = "1.0.15", features = ["sqlite"] }
nidus-cache = { version = "1.0.15", features = ["moka"] }
```

The embedded dashboard is optional through the facade:

```toml
nidus = { package = "nidus-rs", version = "1.0.15", features = ["dashboard"] }
```

## Feature Flags

| Feature | Use when |
| --- | --- |
| `http` | composing Axum routers, controllers, middleware, health, metrics, and server defaults |
| `config` | loading typed settings from JSON, files, pairs, or environment values |
| `openapi` | collecting route metadata and rendering OpenAPI JSON |
| `validation` | validating DTOs through garde-backed pipes and extractors |
| `auth` | defining guard traits, guard combinators, or Tower guard layers |
| `events` | dispatching in-process application events |
| `jobs` | running sync or async job queues |
| `observability` | wiring logs, metrics, traces, lifecycle validation, and adapter instrumentation |
| `dashboard` | mounting the protected embedded runtime cockpit, dashboard APIs, capture, auth, and storage |
| `otel` | enabling OpenTelemetry trace-context helpers through the HTTP surface |

## Common Imports

Use the prelude in application entrypoints:

```rust
use nidus::prelude::*;
```

The prelude keeps app-composition traits such as `NidusApplicationExt`,
`ApplicationHttpExt`, and `ApiDefaultsObservabilityExt` in scope. Prefer the
facade builder path, `Nidus::create::<AppModule>().with_router(router)`, when
composing manual Axum routes with module routes.

## Ownership Boundary

Nidus owns framework composition points: module metadata, provider
registration, controller metadata, guard and pipe hooks, OpenAPI route
metadata, production HTTP defaults, observed events, observed jobs, and
official adapter builders.

Axum, Tower, Tokio, serde, garde, utoipa, SQLx, Moka, and tracing remain normal
Rust ecosystem tools. Raw SQL queries, cache-client behavior, persistence
migrations, deployment manifests, and external queues stay application-owned
unless the app chooses an adapter or middleware boundary.
