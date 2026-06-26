# Nidus

Nidus is a modular Rust application framework inspired by NestJS ergonomics and built on Rust-native explicitness, compile-time checks, Axum, Tower, and Tokio.

## What Nidus Is

Nidus is a framework for building organized Rust backend services with modules, typed providers, controllers, guards, pipes, OpenAPI metadata, testing helpers, and CLI tooling. It aims to make large services easier to structure without hiding the Rust ecosystem underneath.

## What Nidus Is Not

Nidus is not a custom HTTP server, a runtime reflection container, or TypeScript translated into Rust. It composes Axum, Tower, Tokio, serde, tracing, validator, utoipa, and other proven crates directly.

## Quickstart

```bash
cargo nidus new hello-nidus
cd hello-nidus
cargo run
```

During local framework development, the CLI can generate a project against this checkout:

```bash
cargo run -p cargo-nidus -- nidus new hello-nidus --path /tmp --nidus-path "$PWD/crates/nidus"
cd /tmp/hello-nidus
cargo check
```

For the broader framework mental model, see [docs/](docs/README.md).

## Example

```rust
use nidus::prelude::*;

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[get("/:id")]
    async fn find_one(&self) {}
}
```

## Features

- Typed dependency injection primitives with `Inject<T>`, `Optional<T>`, `Lazy<T>`, `Factory<T>`, and `Scoped<T>`.
- Explicit module definitions and circular import detection.
- Axum-backed controller route composition.
- Guard, validation, config, OpenAPI, events, jobs, request-scope, production API defaults, health, metrics, structured logging, OTel helpers, security boundary layers, request context, and testing support crates.
- `cargo-nidus` project generation.
- Compile-fail tests for invalid macro usage.
- Criterion benchmark targets for routing, dependency resolution, and request lifecycle setup.
- Separately installable official adapters for SQLx and cache integration.

Default features enable the core HTTP, config, and tracing facade. Optional
facade features are available for `openapi`, `validation`, `auth`, `events`,
`jobs`, and `testing`. Ecosystem integrations such as SQLx and cache live in
separate crates including `nidus-sqlx` and `nidus-cache`.

## Status

Nidus is under active implementation as a pre-1.0 framework. The repository contains the working framework surface described here, and public APIs can still change before the first published stable version.

## Roadmap

- Continue hardening route-level guard and pipe ergonomics while preserving the explicit Tower and extractor-based execution model.
- Continue hardening official adapters, jobs, events, and production API examples into production-shaped applications.
- Keep benchmark result tables current as the raw Axum and Nidus overhead baselines evolve.
- Keep broadening compile-fail and CLI regression coverage as the public API settles.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). Changes should be small, tested, documented, and aligned with Rust ecosystem expectations.

## License

Licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
