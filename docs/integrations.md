# Integrations

Nidus integrations are optional adapter crates that make ecosystem libraries
feel native to Nidus without hiding those libraries.

Core Nidus stays lean. Database, cache, queue, storage, search, email, and
observability backends should live in separately installable crates so
applications only compile the vendors they choose.

## Philosophy

Official adapters should provide Nest-like organization with Rust-native
explicitness:

- register dependencies by Rust type, not string token
- use explicit module metadata and imports
- expose typed config structs and builder APIs
- preserve direct access to raw ecosystem clients
- keep backend dependencies behind adapter crate features
- preserve source errors instead of erasing them
- avoid global registries, runtime reflection, and hidden background tasks

Use a Nidus adapter when it reduces framework wiring, testing boilerplate, or
health/config integration. Use the raw ecosystem crate directly when the
adapter adds no value for a specific dependency.

## Adapter Contract

Every official adapter crate should document and test this shape:

- typed config struct
- builder API
- module/provider registration path
- exported provider or client wrapper type
- direct access to the underlying client
- lifecycle behavior where the backend needs explicit shutdown
- health check integration where useful
- deterministic tests that avoid external services by default
- feature flags for backend-specific dependencies
- errors that preserve source errors
- runnable workspace examples

Third-party adapters should follow the same shape. They do not need a core
framework trait unless they are sharing behavior with another crate; current
Nidus extension points are `Container`, `ProviderRegistrant`, `ModuleBuilder`,
`Config`, `HealthRegistry`, and lifecycle hooks.

## SQLx

Install `nidus-sqlx` directly:

```toml
nidus = { version = "0.1", features = ["http", "config"] }
nidus-sqlx = { version = "0.1", features = ["sqlite"] }
```

The adapter registers typed providers such as `SqlitePoolProvider` and
`PostgresPoolProvider`. Those providers expose the real SQLx pool:

```rust
let mut container = nidus::prelude::Container::new();
nidus_sqlx::SqlitePoolProvider::builder()
    .database_url("sqlite::memory:")
    .max_connections(1)
    .register(&mut container)
    .await?;

let provider = container.resolve::<nidus_sqlx::SqlitePoolProvider>()?;
let pool: &sqlx::SqlitePool = provider.pool();
```

Default SQLx adapter tests use SQLite in memory. Postgres support is compiled
and metadata-tested by default, but live Postgres connectivity is intentionally
not required by the workspace test suite.

## Cache

Install `nidus-cache` directly:

```toml
nidus = { version = "0.1" }
nidus-cache = { version = "0.1", features = ["moka"] }
```

`MokaCacheProvider` is a local in-memory cache provider with namespace, TTL, and
capacity settings:

```rust
let cache = nidus_cache::MokaCacheProvider::builder()
    .namespace("users")
    .time_to_live(std::time::Duration::from_secs(60))
    .max_capacity(10_000)
    .build();

cache.insert("42", b"Ada".to_vec()).await;
let value = cache.get("42").await;
let raw = cache.inner();
```

The local provider is deterministic and tested without external services. Redis
is reserved for feature-gated distributed cache support; default tests must not
require a Redis server.

## Migration From `sqlx-postgres`

The old facade feature:

```toml
nidus = { version = "0.1", features = ["sqlx-postgres"] }
```

has been replaced by a separate adapter dependency:

```toml
nidus = { version = "0.1", features = ["http", "config"] }
nidus-sqlx = { version = "0.1", features = ["postgres"] }
```

Imports move from the `nidus` prelude or `nidus::sqlx` to `nidus_sqlx` and
direct `sqlx` usage:

```rust
use nidus_sqlx::{PostgresPoolConfig, PostgresPoolProvider};
```

This keeps SQLx out of core Nidus unless the application explicitly installs
the adapter.

## Current Limitations

- `nidus-sqlx` currently proves SQLite runtime wiring and Postgres metadata
  without requiring live external services.
- `nidus-cache` currently proves local Moka cache wiring. Distributed Redis
  semantics are not part of the default verified behavior yet.
- `nidus-auth-jwt`, `nidus-queue`, `nidus-observability`, `nidus-storage`,
  `nidus-search`, and `nidus-email` are future adapter families.
