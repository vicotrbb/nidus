# Nidus

Nidus is a modular Rust backend framework for building explicit, production-ready services with typed dependency injection, module graphs, Axum routes, Tower middleware, validation, OpenAPI, observability, testing, and separately installable adapters. It composes Axum, Tower, Tokio, serde, tracing, garde, utoipa, SQLx adapters, cache adapters, and normal Cargo workflows instead of replacing them.

## Install

Install the Nidus CLI from crates.io:

```bash
cargo install cargo-nidus
cargo nidus new hello-nidus
cd hello-nidus
cargo run
```

During local framework development, install the CLI from this checkout:

```bash
cargo install --path crates/cargo-nidus
cargo nidus new hello-nidus
```

Application dependencies stay explicit:

```toml
[dependencies]
nidus = { package = "nidus-rs", version = "1.0.9", features = ["http", "config", "openapi", "validation"] }
```

For production observability through the facade:

```toml
nidus = { package = "nidus-rs", version = "1.0.9", features = ["observability", "events", "jobs", "otel"] }
```

For embedded dashboard introspection:

```toml
nidus = { package = "nidus-rs", version = "1.0.9", features = ["dashboard"] }
```

Official integrations are separate crates:

```toml
nidus-sqlx = { version = "1.0.9", features = ["sqlite"] }
nidus-cache = { version = "1.0.9", features = ["moka"] }
```

## Which Crate Do I Install?

- Use `cargo-nidus` for `cargo nidus new`, route inspection, graph inspection, and OpenAPI generation.
- Use `nidus-rs` as the application facade. Import it as `nidus` in `Cargo.toml`.
- Enable facade features such as `http`, `config`, `openapi`, `validation`, `auth`, `events`, `jobs`, `observability`, and `otel` only when the app needs them.
- Add `nidus-sqlx` or `nidus-cache` when choosing those official adapters.
- Depend on lower-level crates such as `nidus-core` or `nidus-http` only when building framework extensions.

## Common Imports And Extension Traits

Use the prelude at application entrypoints:

```rust
use nidus::prelude::*;
```

The prelude is the recommended import because it keeps common app composition
types and extension traits in scope:

- `NidusApplicationExt` enables `Nidus::create::<AppModule>()`.
- The facade builder supports `.with_router(router)` and
  `.build_with_router(router)` for composing manual Axum routes with module
  routes.
- `ApplicationHttpExt` remains available for lower-level
  `Nidus::bootstrap::<AppModule>()?.with_router(router)` composition.
- `ApiDefaultsObservabilityExt` enables `.observability(&observability)` and
  observability-aware API defaults when the `observability` feature is enabled.

## Common Compile Errors

- `no method named with_router` after `Nidus::bootstrap`: import
  `ApplicationHttpExt` or `nidus::prelude::*`; after `Nidus::create`, call the
  builder's `.with_router(router)` before `.build().await`.
- `no method named listen` or `no method named into_router`: import
  `NidusApplicationExt` or `nidus::prelude::*`.
- `no method named observability`: import `ApiDefaultsObservabilityExt` or
  `nidus::prelude::*`.

## Learning Path

1. Run `cargo nidus new hello-nidus` and start the generated server.
2. Inspect the generated module, controller, and service with `cargo nidus routes` and `cargo nidus graph`.
3. Add one feature controller or service with `cargo nidus generate`.
4. Add `config`, `validation`, or `openapi` when the first real route needs it.
5. Add `nidus-sqlx` or `nidus-cache` only after the application has a real persistence or cache boundary.

## Quickstart

```rust
use nidus::prelude::*;

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[get("/:id")]
    async fn find_one(&self, Path(id): Path<i64>) -> String {
        format!("user {id}")
    }
}

#[module]
struct AppModule {
    controllers: (UsersController,),
}

#[nidus::main]
async fn main() -> nidus::Result<()> {
    let app = Nidus::create::<AppModule>()
        .build_with_router(UsersController.into_router())
        .await?;

    app.listen("127.0.0.1:3000").await?;
    Ok(())
}
```

## Core Concepts

- **Modules:** explicit imports, providers, controllers, and exports.
- **Providers:** Rust types registered by type, with singleton, transient, request-scoped, lazy, optional, and factory patterns.
- **Controllers:** Axum-backed route composition with Nidus route metadata.
- **Guards and pipes:** explicit authorization and validation boundaries.
- **Config:** typed configuration from JSON, files, pairs, and environment variables.
- **OpenAPI:** route metadata, schemas, and generated documents.
- **Observability:** additive production setup for HTTP metrics, traces, events, jobs, lifecycle validation, and official adapter operations.
- **Dashboard:** optional protected `/nidus/dashboard` runtime cockpit with Home, Atlas, Routes, Timeline, Adapters, Settings, JSON APIs, route snapshots, timeline storage, and SSE stream.
- **Events and jobs:** in-process event buses, sync/async queues, and observed runners.
- **Testing:** `nidus_testing::TestApp` for in-memory request tests and provider overrides.

## Production Defaults

`nidus-http` provides opt-in production API defaults for request IDs, request context, health, readiness checks, metrics, CORS, body limits, timeout responses, security headers, structured logging, error envelopes, unmatched-route `not_found` fallbacks, and OpenTelemetry trace-context helpers. The defaults return normal Axum routers and Tower layers, so applications can replace or reorder the boundary.

Recommended production observability is additive:

```rust
use nidus::prelude::*;

let observability = Observability::production("users-api")
    .version(env!("CARGO_PKG_VERSION"))
    .environment("prod")
    .prometheus()
    .tracing()
    .otel_from_env();

let app = Nidus::create::<AppModule>()
    .with_observability(observability.clone())
    .build()
    .await?;
```

Automatic instrumentation applies where Nidus owns the integration point:
HTTP middleware, `ObservedEventBus`, `ObservedJobRunner`, module validation, and
official adapter builders. Raw SQLx queries, raw cache clients, ORMs, queues,
and HTTP clients remain explicit application instrumentation.

## Adapter Story

The `nidus` facade stays lean. SQLx and cache integration live in `nidus-sqlx` and `nidus-cache`, with direct access to the underlying ecosystem clients. This keeps vendor dependencies out of core applications until they are explicitly installed.

## Examples

- `examples/hello-world`: minimal server.
- `examples/openapi`: OpenAPI JSON and docs routes.
- `examples/production-api`: production middleware defaults.
- `examples/dashboard-api`: embedded dashboard runtime cockpit with bearer or local-disabled auth, SQLite storage, metadata-only capture, route snapshots, Atlas graph, Timeline event/job filters, APIs, SSE, and live curl checks.
- `examples/realworld-api`: team tasks API with modules, SQLite, validation, OpenAPI, health, observability, request IDs, guards, CORS, limits, timeouts, events, and jobs.
- `examples/sqlx-app` and `examples/cache-app`: official adapter wiring.
- `examples/external-support-desk`: copyable external-user support desk API using crates.io-style dependencies, DI, ticket lifecycle routes, API-key auth, request IDs, validation errors, not-found behavior, and `nidus-testing`.
- `examples/external-commerce`: copyable external-user commerce API using crates.io-style dependencies, `nidus-sqlx` SQLite wiring, `nidus-cache`, products, carts, inventory, idempotent checkout, health/readiness, metrics, and `nidus-testing`.

Run an example:

```bash
cargo run -p nidus-example-realworld-api
```

The `external-*` examples are standalone Cargo packages with their own
`[workspace]` tables. Verify them from their folders or with
`bash scripts/verify-external-examples.sh`; they intentionally do not use local
workspace path dependencies. Before `1.0.9` is published to crates.io, verify
the same examples against temporary local patches:

```bash
NIDUS_EXTERNAL_EXAMPLES_LOCAL_PATCH=1 bash scripts/verify-external-examples.sh
```

That mode copies the external examples to a temp directory and appends
temporary `[patch.crates-io]` entries there only. The checked-in examples stay
copyable crates.io-style manifests.

## Documentation

- Local Markdown docs: [docs/](docs/README.md)
- Generated website source: [website/](website/)
- GitHub Pages build: `.github/workflows/pages.yml`

Build and check the static website locally:

```bash
cd website
npm run verify
```

## Release Status

Nidus 1.0.0 established the public crate set. The current release track is
1.0.9, focused on allocation-conscious route normalization and in-place OpenAPI
schema registration, with benchmark and deterministic regression evidence for
both changes.

## Fuzzing

The `fuzz/` package uses cargo-fuzz to compile deterministic fuzz targets for
config parsing, route path normalization, and OpenAPI path normalization:

```bash
cargo +nightly fuzz build
cargo +nightly fuzz run route_paths
```

Use short local runs for development and CI compile checks for release hygiene.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). Changes should be small, tested, documented, and aligned with Rust ecosystem expectations.

## License

Licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
