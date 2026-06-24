# Examples

The workspace includes small examples that exercise the public crates without
requiring external services by default.

| Example | Purpose |
| --- | --- |
| `hello-world` | Minimal Nidus HTTP server with a composed route on `127.0.0.1:3000`. |
| `rest-api` | Nidus controller route composition with an Axum JSON handler and request-scoped provider extraction. |
| `auth-api` | Guard trait implementation with explicit guard failure mapping in a Nidus-composed route. |
| `sqlx-postgres` | Facade `sqlx-postgres` feature plus typed provider registration around Postgres pool options without opening a database connection. |
| `openapi` | Controller metadata converted into an OpenAPI JSON document. |
| `background-jobs` | In-memory job queue execution and reporting. |
| `modular-monolith` | Macro-defined module graph imports, providers, controllers, and exports. |

Run an example with Cargo's package selector:

```bash
cargo run -p nidus-example-openapi
```

Server examples bind to `127.0.0.1:3000` and keep running until interrupted:

```bash
cargo run -p nidus-example-hello-world
curl http://127.0.0.1:3000/
```

All examples are workspace members, so they are checked by the normal workspace
validation commands:

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```
