# Getting Started

Install the CLI after the 1.0 crate is published:

```bash
cargo install cargo-nidus
```

During local framework development, install from this checkout:

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
When a generated feature module already exists, generating a same-name service,
repository, or controller refreshes that generated module's provider,
controller, and export metadata. Generating the module after those artifacts
does the same discovery in the other direction. Hand-written module bodies are
left untouched.

The generated project starts as a small Nidus HTTP server with a macro-defined
root `AppModule`. Applications can define controllers and executable routes with
`#[controller]`, `#[routes]`, and HTTP method attributes, while still dropping
down to explicit `Controller` and `RouteDefinition` builders when useful.

Inspect generated controller metadata:

```bash
cargo nidus routes
cargo nidus graph
cargo nidus expand --dry-run
cargo nidus openapi
cargo nidus openapi --title "Users API" --version "1.2.3"
cargo nidus check
```

`cargo nidus routes` prints HTTP methods, normalized paths such as `/users/{id}`, OpenAPI summaries, and route annotations such as guards, pipes, and validation markers when metadata includes them.
`cargo nidus graph` prints root and feature modules plus any explicit imports, providers, controllers, and exports discovered from `#[module]` field metadata or `ModuleBuilder` metadata.
`cargo nidus expand --dry-run` prints the `cargo expand` invocation for inspecting generated macro code. Without `--dry-run`, it runs `cargo expand` against the project manifest. Install `cargo-expand` first with `cargo install cargo-expand`.
`cargo nidus openapi` prints OpenAPI JSON with default `info.title` and `info.version` values; pass `--title` and `--version` when the generated document should match an application-specific API identity.
`cargo nidus check` validates required project files, accepts either
`src/main.rs` or `src/lib.rs` as the crate root, catches stale generated
`mod.rs` index entries, and verifies that generated feature directories are
declared from a crate root.
