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

The CLI is source-driven. It inspects Rust files and macro metadata rather than
depending on hidden runtime registration. Use it before commits when route
shape, module graph shape, or OpenAPI output matters.
