# Examples

The workspace includes small examples that exercise the public crates without
requiring external services by default.

| Example | Purpose |
| --- | --- |
| `hello-world` | Minimal Tokio binary used as the smallest runnable project shape. |
| `rest-api` | Nidus controller route composition with an Axum JSON handler served on `127.0.0.1:3000`. |
| `auth-api` | Guard trait implementation with explicit guard failure mapping in a Nidus-composed route. |
| `sqlx-postgres` | Facade `sqlx-postgres` feature plus typed provider registration around Postgres pool options without opening a database connection. |
| `openapi` | Controller metadata converted into an OpenAPI JSON document. |
| `background-jobs` | In-memory job queue execution and reporting. |
| `modular-monolith` | Explicit module graph imports, providers, controllers, and exports. |

Run an example with Cargo's package selector:

```bash
cargo run -p nidus-example-openapi
```

Server examples bind to `127.0.0.1:3000` and keep running until interrupted:

```bash
cargo run -p nidus-example-rest-api
curl http://127.0.0.1:3000/users/1
```

All examples are workspace members, so they are checked by the normal workspace
validation commands:

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```
