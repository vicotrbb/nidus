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
- Guard, validation, config, OpenAPI, events, jobs, request-scope, and testing support crates.
- `cargo-nidus` project generation.
- Compile-fail tests for invalid macro usage.
- Criterion benchmark targets for routing, dependency resolution, and request lifecycle setup.

Default features enable the core HTTP, config, and tracing facade. Optional
facade features are available for `openapi`, `validation`, `auth`, `events`,
`jobs`, `testing`, and `sqlx-postgres`.

## Status

Nidus is under active implementation. The current repository is a working foundation, not a finished production release. Public APIs can change before the first published version.

## Roadmap

- Expand macro code generation from validation-only attributes into explicit registration code.
- Add richer module graph provider validation.
- Integrate request-scoped providers into higher-level controller examples.
- Expand REST, auth, SQLx, jobs, and modular monolith examples into production-shaped applications.
- Measure and publish overhead against raw Axum baselines.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). Changes should be small, tested, documented, and aligned with Rust ecosystem expectations.

## License

Licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
