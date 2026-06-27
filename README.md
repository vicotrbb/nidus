# Nidus

Nidus is a modular Rust backend framework for teams that want NestJS-like application organization with Rust-native explicitness. It composes Axum, Tower, Tokio, serde, tracing, garde, utoipa, SQLx adapters, cache adapters, and normal Cargo workflows instead of replacing them.

## Install

After the 1.0 crates are published:

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
nidus = { version = "1.0", features = ["http", "config", "openapi", "validation"] }
```

Official integrations are separate crates:

```toml
nidus-sqlx = { version = "1.0", features = ["sqlite"] }
nidus-cache = { version = "1.0", features = ["moka"] }
```

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
    let app = Nidus::bootstrap::<AppModule>()?
        .with_router(UsersController.into_router());

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
- **Events and jobs:** in-process event buses, sync/async queues, and observed runners.
- **Testing:** `nidus_testing::TestApp` for in-memory request tests and provider overrides.

## Production Defaults

`nidus-http` provides opt-in production API defaults for request IDs, request context, health, readiness checks, metrics, CORS, body limits, timeout responses, security headers, structured logging, error envelopes, and OpenTelemetry trace-context helpers. The defaults return normal Axum routers and Tower layers, so applications can replace or reorder the boundary.

## Adapter Story

The `nidus` facade stays lean. SQLx and cache integration live in `nidus-sqlx` and `nidus-cache`, with direct access to the underlying ecosystem clients. This keeps vendor dependencies out of core applications until they are explicitly installed.

## Examples

- `examples/hello-world`: minimal server.
- `examples/openapi`: OpenAPI JSON and docs routes.
- `examples/production-api`: production middleware defaults.
- `examples/realworld-api`: team tasks API with modules, SQLite, validation, OpenAPI, health, metrics, request IDs, guards, CORS, limits, timeouts, events, and jobs.
- `examples/sqlx-app` and `examples/cache-app`: official adapter wiring.

Run an example:

```bash
cargo run -p nidus-example-realworld-api
```

## Documentation

- Local Markdown docs: [docs/](docs/README.md)
- Generated website source: [website/](website/)
- GitHub Pages build: `.github/workflows/pages.yml`

Build and check the static website locally:

```bash
cd website
npm run verify
```

## 1.0 Status

This repository is preparing the Nidus 1.0 launch. Local dry-runs prove packageability; actual crates.io publishing and GitHub Pages deployment require external credentials or repository settings and should be reported separately when they are not performed.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). Changes should be small, tested, documented, and aligned with Rust ecosystem expectations.

## License

Licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
