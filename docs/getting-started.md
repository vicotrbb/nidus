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

`cargo nidus new` refuses to overwrite an existing destination directory.
Generated artifacts are written under their feature directory, the matching `mod.rs` index is updated, and the feature directory is declared from `src/main.rs` or `src/lib.rs`.
Artifact names must start with an ASCII letter after normalization; names such as `user.profile` are normalized to Rust module filenames such as `user_profile.rs` and Rust types such as `UserProfileService`.

The generated project starts as a small Nidus HTTP server with a composed route
and a macro-defined root `AppModule`. It can add modules, providers,
controllers, and route metadata as the application grows.

Inspect generated controller metadata:

```bash
cargo nidus routes
cargo nidus graph
cargo nidus openapi
cargo nidus check
```

`cargo nidus routes` prints HTTP methods, normalized paths such as `/users/{id}`, OpenAPI summaries, and route annotations such as guards, pipes, and validation markers when metadata includes them.
`cargo nidus graph` prints root and feature modules plus any explicit imports, providers, controllers, and exports discovered from `#[module]` field metadata or `ModuleBuilder` metadata.
`cargo nidus check` validates required project files and catches stale generated `mod.rs` index entries.
