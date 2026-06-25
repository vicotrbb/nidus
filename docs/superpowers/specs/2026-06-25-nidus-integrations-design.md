# Nidus Integrations Design

## Goal

Create Nidus' official adapter pattern and prove it with two production-shaped,
separately installable adapter crates: `nidus-sqlx` and `nidus-cache`.

The design keeps Nidus aligned with its current Rust-native model:

- type-keyed dependency injection through `Container`
- explicit module metadata through `ModuleBuilder`
- explicit async initialization through module async initializers
- direct access to underlying ecosystem clients
- optional facade features only when they do not pull adapter dependencies into
  the core `nidus` crate
- no hidden global registries, runtime reflection, forced vendors, or runtime
  NestJS-style magic

## Current Repo Evidence

The current workspace has:

- `nidus-core` for `Container`, `ProviderRegistrant`, `ModuleBuilder`,
  `AsyncProviderInitializer`, `LifecycleHook`, and module graph validation.
- `nidus` as a public facade that currently exposes optional features such as
  `http`, `config`, `openapi`, `validation`, `auth`, `events`, `jobs`,
  `testing`, and a legacy `sqlx-postgres` facade feature.
- `nidus-config` for explicit typed config loading from pairs, JSON, files, and
  environment prefixes.
- `nidus-http::health::HealthRegistry` for liveness and readiness checks.
- `examples/sqlx-postgres`, which only demonstrates a facade `sqlx-postgres`
  feature and `PgPoolOptions` injection without a reusable adapter crate.
- `examples/realworld-api`, which uses app-local SQLite setup through a
  hand-written `DatabaseModule` async initializer.

The target adapter system should reuse these extension points instead of adding
new core runtime behavior.

## Architecture

Official adapters are normal workspace crates named `nidus-*`. Each adapter owns
its market dependencies and exposes small Nidus-style helpers around those
dependencies. Core framework crates remain independent from adapter crates.

The shared adapter shape is documented rather than enforced by a new core trait
because the current extension points already provide the required composition
surface. A trait in core would either be too weak to cover different dependency
types or too strong and force adapters into a single runtime model.

Each official adapter crate must provide:

- a typed config struct
- a builder API
- explicit `Container` registration helpers
- optional module-definition helpers when the dependency has startup work
- direct exported ecosystem client/provider types
- health integration where meaningful
- lifecycle or shutdown behavior where the backend needs it
- deterministic tests that do not require external services by default
- feature flags for backend-specific dependencies
- errors that preserve source errors
- docs and runnable examples

## Crate Layout

Add these crates:

- `crates/nidus-sqlx`
- `crates/nidus-cache`

Add these examples:

- `examples/sqlx-app`
- `examples/cache-app`
- `examples/integrations-production`

Remove the legacy example from the workspace once the replacement SQLx adapter
example exists:

- `examples/sqlx-postgres`

Keep `nidus` facade dependencies lean:

- Remove the legacy `sqlx-postgres` facade feature.
- Do not add `nidus-sqlx` or `nidus-cache` as dependencies of `nidus`.
- Users install adapters directly beside `nidus`:

```toml
nidus = { version = "0.1", features = ["http", "config"] }
nidus-sqlx = { version = "0.1", features = ["sqlite"] }
nidus-cache = { version = "0.1", features = ["moka"] }
```

## Public API Shape

### `nidus-sqlx`

Features:

- `postgres`: enables `sqlx/postgres`
- `sqlite`: enables `sqlx/sqlite`
- `mysql`: optional future backend, not implemented in the first phase unless
  it stays low risk
- `nidus-config`: enables helpers that read database config from
  `nidus_config::Config`
- `health`: enables readiness check helpers that return `HealthStatus`

Default features:

- none

Primary types:

- `SqlxError`: adapter error preserving `sqlx::Error` and config errors
- `DatabaseConfig`: explicit database URL and pool options common enough to be
  backend-neutral
- `PostgresPoolConfig`: Postgres-specific pool settings
- `SqlitePoolConfig`: SQLite-specific pool settings
- `PostgresPoolProvider`: wrapper around `sqlx::PgPool`
- `SqlitePoolProvider`: wrapper around `sqlx::SqlitePool`
- `PostgresModule`: module helper for registering and exporting
  `PostgresPoolProvider`
- `SqliteModule`: module helper for registering and exporting
  `SqlitePoolProvider`

Builder examples:

```rust
let provider = SqlitePoolProvider::builder()
    .database_url("sqlite::memory:")
    .max_connections(1)
    .connect()
    .await?;

let pool: &sqlx::SqlitePool = provider.pool();
```

Container registration:

```rust
let mut container = Container::new();
SqlitePoolProvider::builder()
    .database_url("sqlite::memory:")
    .register(&mut container)
    .await?;

let provider = container.resolve::<SqlitePoolProvider>()?;
let pool = provider.pool();
```

Module helper:

```rust
let module = SqliteModule::new("DatabaseModule")
    .config(SqlitePoolConfig::new("sqlite::memory:"))
    .definition();
```

Because `ModuleBuilder::async_initializer` currently accepts function pointers,
module helpers cannot capture runtime config directly in closures. The first
phase should therefore provide typed module definitions for well-known provider
types and explicit async registration functions for runtime config. If module
helpers need capture support later, that is a separate core design.

Health:

```rust
let health = HealthRegistry::new().ready_check("database", {
    let provider = provider.clone();
    move || {
        let provider = provider.clone();
        async move { provider.health_check().await }
    }
});
```

Default tests use `sqlite::memory:` for SQLx behavior. Postgres tests compile
provider builders and module metadata but do not require a live service unless a
developer opts into an ignored or environment-gated test.

### `nidus-cache`

Features:

- `moka`: enables in-memory local cache support
- `redis`: enables Redis support
- `health`: enables distributed-backend readiness checks

Default features:

- `moka`

Primary types:

- `CacheError`: adapter error preserving backend errors
- `CacheConfig`: namespace and default TTL settings
- `CacheKey`: namespaced key helper
- `MokaCacheProvider<K, V>` or a concrete string/bytes provider if generic DI
  ergonomics are too noisy
- `RedisCacheProvider`: Redis client wrapper behind the `redis` feature
- `CacheModule`: module helper for simple in-memory cache registration

The first phase should implement a concrete, deterministic local cache provider
for string keys and byte values because it proves TTL, namespace, typed DI, test
substitution, and direct backend access without forcing a complex generic cache
trait. A small abstraction is acceptable only if it simplifies user code and
keeps raw backend access available.

Moka API shape:

```rust
let cache = MokaCacheProvider::builder()
    .namespace("users")
    .time_to_live(Duration::from_secs(60))
    .max_capacity(10_000)
    .build();

cache.insert("42", b"Ada".to_vec()).await;
let value = cache.get("42").await;
let raw = cache.inner();
```

Health:

- Local in-memory cache health is always up if the provider exists.
- Redis health pings the backend and is feature-gated.

Default tests use Tokio time control for deterministic TTL behavior and do not
start Redis. Redis tests are compile-only or ignored unless an explicit
environment variable supplies a Redis URL.

## Feature Flags And Dependency Boundaries

Workspace dependencies may include optional adapter dependencies, but core crates
must not depend on them unless selected by an adapter crate. In particular:

- `crates/nidus/Cargo.toml` should not depend on `sqlx`, `moka`, or `redis`.
- `crates/nidus-core` should not depend on adapter crates.
- `crates/nidus-http` should not depend on adapter crates.
- Adapter examples should depend on adapter crates directly.

The root workspace can hold shared version declarations for `sqlx`, `moka`, and
`redis`, but that does not make them part of core Nidus.

## Testing Strategy

Use TDD for behavior changes:

- Write focused adapter tests first.
- Run each new test and confirm it fails because the public API is missing.
- Implement the minimum code to pass.
- Re-run focused tests.
- Run package checks after each coherent slice.

Default tests must avoid external services:

- SQLx: use SQLite memory pools for runtime tests.
- Postgres: compile and metadata tests only by default.
- Cache: use Moka with Tokio time.
- Redis: feature-gated compile tests and ignored live tests only.

Full verification before final answer:

```bash
git diff --check
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
cargo test --workspace --all-features
cargo tree -d
```

Optional local tools should run if installed:

```bash
cargo deny check
cargo audit
cargo machete
```

Benchmarks are not required for the first adapter phase if no hot-path HTTP,
DI, request lifecycle, metrics, or core module behavior changes. If the
implementation changes those areas, run:

```bash
cargo bench --bench dependency_resolution
cargo bench --bench routing
cargo bench --bench request_lifecycle
```

## Documentation

Add `docs/integrations.md` covering:

- integration philosophy
- separate installability
- adapter contract
- how official and third-party adapters should be shaped
- raw library usage versus adapter usage
- limitations and non-goals
- migration from `nidus` facade `sqlx-postgres` to `nidus-sqlx`

Update:

- `README.md`
- `docs/README.md`
- `docs/architecture.md`
- `docs/examples.md`
- `docs/deployment.md` if health/config examples use adapters

## Migration From Current `sqlx-postgres`

The legacy facade feature:

```toml
nidus = { path = "../../crates/nidus", features = ["sqlx-postgres"] }
```

should become:

```toml
nidus = { path = "../../crates/nidus", features = ["http", "config"] }
nidus-sqlx = { path = "../../crates/nidus-sqlx", features = ["postgres"] }
```

Imports move from:

```rust
use nidus::prelude::{PgPoolOptions, ...};
```

to:

```rust
use nidus_sqlx::{PostgresPoolConfig, PostgresPoolProvider};
```

Users still access the real SQLx type:

```rust
let pool: &sqlx::PgPool = provider.pool();
```

## Non-Goals

The first phase does not:

- implement every expected adapter family
- add `nidus-auth-jwt`, `nidus-queue`, `nidus-observability`,
  `nidus-storage`, `nidus-search`, or `nidus-email`
- add a new adapter trait to `nidus-core`
- add a global adapter registry
- force an ORM, cache, queue, or observability vendor
- hide raw ecosystem clients
- run live Postgres or Redis tests by default
- add background tasks without explicit lifecycle/shutdown behavior

## Risks

- Generic cache providers can make type-keyed DI less discoverable. Prefer a
  concrete first provider unless tests show the generic API is still ergonomic.
- `ModuleBuilder::async_initializer` function-pointer shape limits captured
  runtime config in module helpers. Keep runtime-config registration explicit in
  the first phase instead of widening core behavior.
- `all-features` can accidentally enable multiple SQLx backends. Tests must
  cover feature combinations and avoid mutually exclusive compile paths.
- Documentation can overclaim live service support. Docs must distinguish
  proven default tests from opt-in live tests.

## Self-Review

- Placeholder scan: no deferred-detail markers or unspecified implementation
  placeholders remain.
- Scope check: the spec intentionally narrows implementation to adapter
  contract, `nidus-sqlx`, and `nidus-cache`; later adapter families are listed
  as non-goals.
- Dependency boundary check: core `nidus` remains free of adapter dependencies;
  adapters are installed directly.
- API consistency check: public API examples use `Container`, `ModuleBuilder`,
  `Config`, and `HealthRegistry` extension points observed in the current repo.
- Contradiction check: module helpers are included, but runtime-captured module
  config is explicitly not promised because current async initializers require
  function pointers.
