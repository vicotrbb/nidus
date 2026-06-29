# Getting Started

Install the CLI from crates.io:

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

For an application that starts from an existing manifest, install the facade
crate directly:

```toml
[dependencies]
nidus = { package = "nidus-rs", version = "1.0.1", features = ["http"] }
```

Use `nidus-rs` for applications, `cargo-nidus` for the CLI, and adapter crates
such as `nidus-sqlx` or `nidus-cache` only when the app chooses those backends.
Feature groups keep the facade explicit:

```toml
nidus = { package = "nidus-rs", version = "1.0.1", features = ["http", "config", "openapi", "validation"] }
nidus-sqlx = { version = "1.0.1", features = ["sqlite"] }
nidus-cache = { version = "1.0.1", features = ["moka"] }
```

## Common Imports And Extension Traits

Use the prelude in application entrypoints and examples:

```rust
use nidus::prelude::*;
```

This is the recommended pattern because it imports the extension traits that
make the fluent application API available:

- `ApplicationHttpExt` enables `.with_router(...)`.
- `NidusApplicationExt` enables `Nidus::create::<AppModule>()`, `.listen(...)`,
  and `.into_router()`.
- `ApiDefaultsObservabilityExt` enables `.observability(&observability)` and
  observability-aware API defaults when the `observability` feature is enabled.

## Common Compile Errors

- `no method named with_router`: import `ApplicationHttpExt` or
  `nidus::prelude::*`.
- `no method named listen` or `no method named into_router`: import
  `NidusApplicationExt` or `nidus::prelude::*`.
- `no method named observability`: import `ApiDefaultsObservabilityExt` or
  `nidus::prelude::*`.

`cargo nidus new` refuses to overwrite an existing destination directory.
Generated artifacts are written under their feature directory, the matching `mod.rs` index is updated, and the feature directory is declared from `src/main.rs` or `src/lib.rs`.
Artifact names must start with an ASCII letter after normalization; names such as `user.profile` are normalized to Rust module filenames such as `user_profile.rs` and Rust types such as `UserProfileService`.
When a generated feature module already exists, generating a same-name service,
repository, or controller refreshes that generated module's provider,
controller, and export metadata. Generating the module after those artifacts
does the same discovery in the other direction. Hand-written module bodies are
left untouched.

The generated project starts as a small Nidus HTTP server with a macro-defined
root `AppModule`, one controller, and one injected service. Applications can
define controllers and executable routes with `#[controller]`, `#[routes]`, and
HTTP method attributes, while still dropping down to explicit `Controller` and
`RouteDefinition` builders when useful.

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

From hello world to a real app, keep the path incremental:

1. Run the generated server and curl `/`.
2. Inspect routes and the module graph before adding features.
3. Generate one controller or service for the first domain boundary.
4. Add config, validation, OpenAPI, or auth only when a route needs it.
5. Add SQLx or cache adapter crates only after choosing those backends.
