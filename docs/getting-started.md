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

