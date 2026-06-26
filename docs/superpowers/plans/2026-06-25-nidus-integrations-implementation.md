# Nidus Integrations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the official adapter contract plus `nidus-sqlx` and `nidus-cache` reference adapters with docs, examples, and verification.

**Architecture:** Keep adapter dependencies out of core `nidus`. Implement adapters as separate crates that compose existing `Container`, `ModuleBuilder`, `Config`, and `HealthRegistry` APIs. Prove the pattern with deterministic default tests and examples that install adapters directly.

**Tech Stack:** Rust 2024, Cargo workspace, Nidus core/module APIs, SQLx, Moka, optional Redis, Tokio tests, `thiserror`, `serde`.

---

## File Map

- Create: `crates/nidus-sqlx/Cargo.toml`
- Create: `crates/nidus-sqlx/src/lib.rs`
- Create: `crates/nidus-sqlx/tests/sqlite_adapter.rs`
- Create: `crates/nidus-sqlx/tests/postgres_metadata.rs`
- Create: `crates/nidus-cache/Cargo.toml`
- Create: `crates/nidus-cache/src/lib.rs`
- Create: `crates/nidus-cache/tests/moka_cache.rs`
- Create: `crates/nidus-cache/tests/module_metadata.rs`
- Create: `examples/sqlx-app/Cargo.toml`
- Create: `examples/sqlx-app/src/main.rs`
- Create: `examples/cache-app/Cargo.toml`
- Create: `examples/cache-app/src/main.rs`
- Create: `examples/integrations-production/Cargo.toml`
- Create: `examples/integrations-production/src/main.rs`
- Create: `docs/integrations.md`
- Modify: `Cargo.toml`
- Modify: `README.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/examples.md`
- Modify: `docs/deployment.md`
- Modify: `crates/nidus/Cargo.toml`
- Modify: `crates/nidus/src/lib.rs`
- Modify: `crates/nidus/src/prelude.rs`
- Modify: `crates/nidus/tests/facade_features.rs`
- Delete: `examples/sqlx-postgres/Cargo.toml`
- Delete: `examples/sqlx-postgres/src/main.rs`

## Milestone 1: Adapter Workspace Foundation

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/nidus/Cargo.toml`
- Modify: `crates/nidus/src/lib.rs`
- Modify: `crates/nidus/src/prelude.rs`
- Modify: `crates/nidus/tests/facade_features.rs`
- Create: `crates/nidus-sqlx/Cargo.toml`
- Create: `crates/nidus-sqlx/src/lib.rs`
- Create: `crates/nidus-cache/Cargo.toml`
- Create: `crates/nidus-cache/src/lib.rs`

- [ ] **Step 1: Write facade boundary regression first**

Add this assertion to `crates/nidus/tests/facade_features.rs`:

```rust
#[test]
fn facade_does_not_reexport_sqlx_adapter_dependencies() {
    let manifest = std::fs::read_to_string("crates/nidus/Cargo.toml").unwrap();
    assert!(!manifest.contains("sqlx ="), "{manifest}");
    assert!(!manifest.contains("nidus-sqlx"), "{manifest}");
    assert!(!manifest.contains("nidus-cache"), "{manifest}");
}
```

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test -p nidus facade_does_not_reexport_sqlx_adapter_dependencies
```

Expected: FAIL because `crates/nidus/Cargo.toml` still contains the legacy optional `sqlx` dependency.

- [ ] **Step 3: Scaffold adapter crates and remove facade SQLx**

Implement:

- Add `crates/nidus-sqlx` and `crates/nidus-cache` to workspace members.
- Remove `examples/sqlx-postgres` from workspace members.
- Remove `sqlx-postgres` feature, `sqlx` dependency, and `sqlx` re-exports from `crates/nidus`.
- Keep `sqlx`, `moka`, and `redis` as workspace dependency declarations only if adapter crates use them.
- Create minimal crate roots with `#![deny(missing_docs)]` and crate-level docs.

- [ ] **Step 4: Verify GREEN**

Run:

```bash
cargo test -p nidus facade_does_not_reexport_sqlx_adapter_dependencies
cargo check -p nidus-sqlx --no-default-features
cargo check -p nidus-cache --no-default-features
```

Expected: all commands exit 0.

- [ ] **Step 5: Commit**

Run:

```bash
git add Cargo.toml crates/nidus crates/nidus-sqlx crates/nidus-cache examples/sqlx-postgres
git commit -m "feat: add adapter crate foundation"
```

## Milestone 2: `nidus-sqlx` Reference Adapter

**Files:**
- Modify: `crates/nidus-sqlx/src/lib.rs`
- Create: `crates/nidus-sqlx/tests/sqlite_adapter.rs`
- Create: `crates/nidus-sqlx/tests/postgres_metadata.rs`

- [ ] **Step 1: Write failing SQLite adapter test**

Create `crates/nidus-sqlx/tests/sqlite_adapter.rs`:

```rust
use nidus_core::{Container, ModuleBuilder};
use nidus_sqlx::{SqlitePoolConfig, SqlitePoolProvider};

#[tokio::test]
async fn sqlite_provider_registers_real_pool_in_container() {
    let mut container = Container::new();

    SqlitePoolProvider::builder()
        .database_url("sqlite::memory:")
        .max_connections(1)
        .register(&mut container)
        .await
        .unwrap();

    let provider = container.resolve::<SqlitePoolProvider>().unwrap();
    sqlx::query("SELECT 1").execute(provider.pool()).await.unwrap();
}

#[tokio::test]
async fn sqlite_config_from_nidus_config_uses_nested_database_url() {
    let config = nidus_config::Config::from_json_str(
        r#"{"database":{"url":"sqlite::memory:","max_connections":1}}"#,
    )
    .unwrap();

    let settings = SqlitePoolConfig::from_config_path(&config, ["database"]).unwrap();

    assert_eq!(settings.database_url(), "sqlite::memory:");
    assert_eq!(settings.max_connections(), Some(1));
}

#[test]
fn sqlite_module_declares_provider_and_export() {
    let module = ModuleBuilder::new("DatabaseModule")
        .provider_typed::<SqlitePoolProvider>()
        .export_typed::<SqlitePoolProvider>()
        .build();

    assert_eq!(module.providers(), ["SqlitePoolProvider"]);
    assert_eq!(module.exports(), ["SqlitePoolProvider"]);
}
```

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test -p nidus-sqlx --features sqlite,nidus-config sqlite_adapter
```

Expected: FAIL because `SqlitePoolConfig` and `SqlitePoolProvider` do not exist yet.

- [ ] **Step 3: Implement SQLite provider**

Implement in `crates/nidus-sqlx/src/lib.rs`:

- `SqlxError` with `#[from] sqlx::Error` and config-preserving variants.
- `SqlitePoolConfig` with `new`, `database_url`, `max_connections`, builder setters, and `from_config_path` behind `nidus-config`.
- `SqlitePoolProvider` with `builder`, `pool`, `into_pool`, `connect`, and `register`.
- `ProviderRegistrant` implementation that returns `Ok(())` so `ModuleBuilder` can declare metadata while runtime connection remains explicit.

- [ ] **Step 4: Verify GREEN**

Run:

```bash
cargo test -p nidus-sqlx --features sqlite,nidus-config sqlite_adapter
```

Expected: all tests pass.

- [ ] **Step 5: Write failing Postgres metadata test**

Create `crates/nidus-sqlx/tests/postgres_metadata.rs`:

```rust
use nidus_core::ModuleBuilder;
use nidus_sqlx::{PostgresPoolConfig, PostgresPoolProvider};

#[test]
fn postgres_provider_preserves_raw_sqlx_options_and_module_metadata() {
    let config = PostgresPoolConfig::new("postgres://localhost/nidus")
        .max_connections(5)
        .min_connections(1);

    assert_eq!(config.database_url(), "postgres://localhost/nidus");
    assert_eq!(config.max_connections(), Some(5));
    assert_eq!(config.min_connections(), Some(1));

    let module = ModuleBuilder::new("DatabaseModule")
        .provider_typed::<PostgresPoolProvider>()
        .export_typed::<PostgresPoolProvider>()
        .build();

    assert_eq!(module.providers(), ["PostgresPoolProvider"]);
    assert_eq!(module.exports(), ["PostgresPoolProvider"]);
}
```

- [ ] **Step 6: Verify RED**

Run:

```bash
cargo test -p nidus-sqlx --features postgres postgres_metadata
```

Expected: FAIL because Postgres provider/config types do not exist yet.

- [ ] **Step 7: Implement Postgres provider metadata and builder**

Implement in `crates/nidus-sqlx/src/lib.rs`:

- `PostgresPoolConfig`
- `PostgresPoolProvider`
- `PostgresPoolBuilder`
- `ProviderRegistrant` for `PostgresPoolProvider`
- direct accessors returning `&sqlx::PgPool`

Do not add a default live Postgres test.

- [ ] **Step 8: Verify GREEN**

Run:

```bash
cargo test -p nidus-sqlx --features postgres postgres_metadata
cargo test -p nidus-sqlx --features sqlite,postgres,nidus-config
```

Expected: all commands exit 0 without requiring a live external service.

- [ ] **Step 9: Commit**

Run:

```bash
git add crates/nidus-sqlx Cargo.toml Cargo.lock
git commit -m "feat: add nidus sqlx adapter"
```

## Milestone 3: `nidus-cache` Reference Adapter

**Files:**
- Modify: `crates/nidus-cache/src/lib.rs`
- Create: `crates/nidus-cache/tests/moka_cache.rs`
- Create: `crates/nidus-cache/tests/module_metadata.rs`

- [ ] **Step 1: Write failing Moka cache behavior test**

Create `crates/nidus-cache/tests/moka_cache.rs`:

```rust
use std::time::Duration;

use nidus_cache::{CacheConfig, MokaCacheProvider};
use nidus_core::Container;

#[tokio::test]
async fn moka_cache_namespaces_keys_and_expires_values() {
    let cache = MokaCacheProvider::builder()
        .namespace("users")
        .time_to_live(Duration::from_millis(50))
        .max_capacity(100)
        .build();

    cache.insert("42", b"Ada".to_vec()).await;

    assert_eq!(cache.get("42").await.unwrap(), b"Ada".to_vec());
    assert!(cache.get("users:42").await.is_none());

    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(cache.get("42").await.is_none());
}

#[tokio::test]
async fn moka_cache_registers_in_container() {
    let mut container = Container::new();
    let config = CacheConfig::new().namespace("sessions");

    MokaCacheProvider::builder()
        .config(config)
        .register(&mut container)
        .unwrap();

    let cache = container.resolve::<MokaCacheProvider>().unwrap();
    cache.insert("abc", b"token".to_vec()).await;
    assert_eq!(cache.get("abc").await.unwrap(), b"token".to_vec());
}
```

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test -p nidus-cache --features moka moka_cache
```

Expected: FAIL because cache types do not exist yet.

- [ ] **Step 3: Implement Moka cache provider**

Implement in `crates/nidus-cache/src/lib.rs`:

- `CacheError`
- `CacheConfig`
- `CacheKey`
- `MokaCacheProvider`
- `MokaCacheBuilder`
- namespace handling
- TTL and max-capacity builder options
- `insert`, `get`, `invalidate`, `inner`, `health_status`
- `register`

- [ ] **Step 4: Verify GREEN**

Run:

```bash
cargo test -p nidus-cache --features moka moka_cache
```

Expected: all tests pass.

- [ ] **Step 5: Write failing module metadata test**

Create `crates/nidus-cache/tests/module_metadata.rs`:

```rust
use nidus_cache::MokaCacheProvider;
use nidus_core::ModuleBuilder;

#[test]
fn moka_cache_module_metadata_is_typed_and_exported() {
    let module = ModuleBuilder::new("CacheModule")
        .provider_typed::<MokaCacheProvider>()
        .export_typed::<MokaCacheProvider>()
        .build();

    assert_eq!(module.providers(), ["MokaCacheProvider"]);
    assert_eq!(module.exports(), ["MokaCacheProvider"]);
}
```

- [ ] **Step 6: Verify RED**

Run:

```bash
cargo test -p nidus-cache --features moka module_metadata
```

Expected: FAIL until `ProviderRegistrant` exists for `MokaCacheProvider`.

- [ ] **Step 7: Implement module metadata registration**

Implement `ProviderRegistrant` for `MokaCacheProvider`. The registrant should
return `Ok(())` because runtime cache sizing/config remains explicit through the
builder unless the user registers a default instance.

- [ ] **Step 8: Verify GREEN**

Run:

```bash
cargo test -p nidus-cache --features moka
cargo check -p nidus-cache --all-features
```

Expected: all commands exit 0.

- [ ] **Step 9: Commit**

Run:

```bash
git add crates/nidus-cache Cargo.toml Cargo.lock
git commit -m "feat: add nidus cache adapter"
```

## Milestone 4: Examples And Documentation

**Files:**
- Create: `examples/sqlx-app/Cargo.toml`
- Create: `examples/sqlx-app/src/main.rs`
- Create: `examples/cache-app/Cargo.toml`
- Create: `examples/cache-app/src/main.rs`
- Create: `examples/integrations-production/Cargo.toml`
- Create: `examples/integrations-production/src/main.rs`
- Create: `docs/integrations.md`
- Modify: `README.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/examples.md`
- Modify: `docs/deployment.md`

- [ ] **Step 1: Write SQLx example test first**

`examples/sqlx-app/src/main.rs` should include this test proving SQLx provider
wiring:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use nidus_sqlx::SqlitePoolProvider;

    #[tokio::test]
    async fn example_wires_sqlite_provider() {
        let container = build_container().await.unwrap();
        assert!(container.resolve::<SqlitePoolProvider>().is_ok());
    }
}
```

- [ ] **Step 2: Write cache example test first**

`examples/cache-app/src/main.rs` should include this test proving optional cache
wiring:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use nidus_cache::MokaCacheProvider;

    #[tokio::test]
    async fn example_wires_cache_provider() {
        let container = build_container().await.unwrap();
        assert!(container.resolve::<MokaCacheProvider>().is_ok());
        assert!(container.resolve::<UsersService>().is_ok());
    }
}
```

- [ ] **Step 3: Write production integration example test first**

`examples/integrations-production/src/main.rs` should include this test proving
config, SQLx, cache, and health wiring:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use nidus_cache::MokaCacheProvider;
    use nidus_sqlx::SqlitePoolProvider;

    #[tokio::test]
    async fn example_wires_production_integrations() {
        let app = build_app(test_config()).await.unwrap();
        assert!(app.container().resolve::<SqlitePoolProvider>().is_ok());
        assert!(app.container().resolve::<MokaCacheProvider>().is_ok());
    }
}
```

- [ ] **Step 4: Verify RED**

Run:

```bash
cargo test -p nidus-example-sqlx-app
cargo test -p nidus-example-cache-app
cargo test -p nidus-example-integrations-production
```

Expected: FAIL until examples exist and compile.

- [ ] **Step 5: Implement examples**

Implement:

- `sqlx-app`: SQLite memory pool provider plus repository injection.
- `cache-app`: optional cache dependency in a service.
- `integrations-production`: typed config, SQLite adapter, Moka cache, health
  checks, and test helper wiring without binding a live port in tests.

- [ ] **Step 6: Update docs**

Document:

- adapter philosophy
- separate installability
- adapter contract
- official versus third-party adapter authorship
- raw library usage versus Nidus adapter usage
- `sqlx-postgres` migration path
- examples and limitations

- [ ] **Step 7: Verify GREEN**

Run:

```bash
cargo test -p nidus-example-sqlx-app
cargo test -p nidus-example-cache-app
cargo test -p nidus-example-integrations-production
cargo test --workspace --all-features --doc
```

Expected: all commands exit 0.

- [ ] **Step 8: Commit**

Run:

```bash
git add README.md docs examples Cargo.toml Cargo.lock
git commit -m "docs: document official integrations"
```

## Milestone 5: Final Cleanup And Verification

**Files:**
- Modify only files needed to satisfy verification failures.

- [ ] **Step 1: Run formatting and focused checks**

Run:

```bash
cargo fmt --all --check
cargo test -p nidus-sqlx --features sqlite,postgres,nidus-config
cargo test -p nidus-cache --all-features
```

Expected: all commands exit 0.

- [ ] **Step 2: Run full required gates**

Run:

```bash
git diff --check
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
cargo test --workspace --all-features
cargo tree -d
```

Expected: all commands exit 0 except `cargo tree -d`, which may list duplicate
transitive dependencies and must be reported exactly.

- [ ] **Step 3: Run optional tools if installed**

Run:

```bash
command -v cargo-deny && cargo deny check
command -v cargo-audit && cargo audit
command -v cargo-machete && cargo machete
```

Expected: installed tools run to completion; missing tools are reported as not
available.

- [ ] **Step 4: Benchmark decision**

If no hot-path HTTP, DI, request lifecycle, metrics, or core module behavior was
changed, record that benchmarks were not required. If any such code changed,
run:

```bash
cargo bench --bench dependency_resolution
cargo bench --bench routing
cargo bench --bench request_lifecycle
```

- [ ] **Step 5: Targeted security and reliability review**

Inspect the final diff for:

- core dependency bloat
- feature flag mistakes
- runtime panics
- hidden global state
- unbounded memory growth
- blocking work in async contexts
- missing shutdown/lifecycle paths
- erased source errors
- test-only assumptions in production APIs
- docs claiming more than tests prove

- [ ] **Step 6: Commit final cleanup if needed**

Run only if cleanup changes were made:

```bash
git add .
git commit -m "chore: finalize integration adapters"
```

## Risk Register

- Core dependency bloat: protect with a facade boundary regression and manifest review.
- Feature flag drift: test `--no-default-features`, focused feature sets, and
  `--all-features`.
- External service flakiness: default tests use SQLite memory and Moka only.
- Overclaimed docs: docs must label live Postgres and Redis checks as opt-in.
- Runtime config capture: avoid promising captured module initializers until
  core explicitly supports that shape.
- Cache memory growth: expose `max_capacity` and document local cache limits.

## Rollback Strategy

- Revert adapter implementation commits independently because spec and plan are
  separate commits.
- If `nidus-cache` fails late, keep `nidus-sqlx` only if docs and examples are
  adjusted to one reference adapter and the objective is explicitly marked
  partially proven.
- If all adapter implementation must be reverted, keep the spec and plan commits
  as phase decomposition artifacts and report implementation as not yet proven.
- Do not push any commits.

## Self-Review

- Spec coverage: the plan covers adapter contract documentation, separate
  crates, SQLx, cache, examples, migration docs, validation, optional tools,
  benchmarks, and targeted review.
- Placeholder scan: no deferred-detail markers or unspecified implementation
  placeholders remain.
- Type consistency: `SqlitePoolProvider`, `PostgresPoolProvider`, and
  `MokaCacheProvider` names match between tests, docs, and implementation tasks.
- Scope control: auth JWT, queue, observability, storage, search, and email
  remain future adapter families and are not part of this first phase.
