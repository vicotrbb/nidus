# Getting Started

Install the CLI from this workspace during development:

```bash
cargo install --path crates/cargo-nidus
```

Create an application:

```bash
cargo nidus new hello-nidus
cd hello-nidus
cargo run
```

The generated project starts as a small Axum server and can add Nidus modules, providers, controllers, and route metadata as the application grows.

Inspect generated controller metadata:

```bash
cargo nidus routes
cargo nidus openapi
```

`cargo nidus routes` prints HTTP methods, normalized paths such as `/users/{id}`, and OpenAPI summaries when route metadata includes them.
