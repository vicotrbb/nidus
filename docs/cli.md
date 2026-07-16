# CLI

`cargo-nidus` provides project generation and source inspection commands:

```bash
cargo nidus new hello-nidus
cargo nidus check
cargo nidus routes
cargo nidus graph
cargo nidus openapi
cargo nidus expand --dry-run
```

| Command | Purpose | Expected output |
| --- | --- | --- |
| `cargo nidus new <name>` | create a starter service | a Cargo project with `src/main.rs`, one module, one controller, and one injected service |
| `cargo nidus check` | validate generated project structure | success when crate roots, generated modules, and feature directories are consistent |
| `cargo nidus routes` | inspect controller route metadata | HTTP methods, normalized paths, summaries, guards, pipes, and validation markers when present |
| `cargo nidus graph` | inspect module metadata | root and feature modules plus imports, providers, controllers, and exports |
| `cargo nidus openapi` | render OpenAPI JSON from route metadata | a JSON document with configurable title and version |
| `cargo nidus expand --dry-run` | show the macro expansion command | the `cargo expand` invocation without running it |

The CLI is source-driven. It recursively inspects Rust files under `src` and
macro metadata rather than depending on hidden runtime registration, so both
generated `src/controllers` projects and feature-oriented layouts such as
`src/users/controller.rs` are supported. A controller definition and its
`#[routes]` implementation may also live in separate source files when the
controller's short type name is unique; file-local definitions take precedence
and ambiguous cross-file short names produce an actionable error instead of an
incomplete route list. Use the CLI before commits when route shape, module graph
shape, or OpenAPI output matters.
