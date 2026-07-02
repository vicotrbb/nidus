# Examples

The workspace includes small examples that exercise the public crates without
requiring external services by default.

| Example | Purpose |
| --- | --- |
| `hello-world` | Minimal Nidus HTTP server with a macro-defined controller route on `127.0.0.1:3000`. |
| `rest-api` | Macro-defined Nidus controller route with an Axum JSON response and request-scoped provider extraction. |
| `auth-api` | Guard trait implementation with explicit guard failure mapping in a Nidus-composed route. |
| `openapi` | Controller metadata converted into an OpenAPI JSON document plus `/openapi.json` and `/docs` routes. |
| `background-jobs` | In-memory job queue execution with success and failure reporting. |
| `dashboard-api` | Embedded Nidus Dashboard runtime cockpit with bearer or local-disabled auth, SQLite storage, metadata-only capture, route snapshots, Atlas graph, Timeline event/job filters, dashboard APIs, SSE, and live curl checks. |
| `modular-monolith` | Macro-defined module graph imports, providers, controllers, and exports. |
| `realworld-api` | Production-shaped team tasks API with modules, SQLite persistence, validation, OpenAPI, health, observability, request IDs, guards, CORS, limits, timeouts, events, and jobs. |
| `production-api` | Production API preset (`nidus-example-production-api`) with health, observability, request context extraction, validated request IDs, error envelopes, and route-local rate limiting. |
| `sqlx-app` | Separate `nidus-sqlx` SQLite adapter with repository injection and direct SQLx query access. |
| `cache-app` | Separate `nidus-cache` Moka adapter with an optional cache dependency in a service. |
| `integrations-production` | Production-shaped integration wiring with typed config, SQLite, Moka cache, health checks, and adapter observability without binding a live port in tests. |
| `external-support-desk` | Standalone external-user support desk API with crates.io-style dependencies, DI, tickets, comments, priorities, statuses, assignment, close transition, validation failures, `x-api-key` auth, request IDs, not-found behavior, live curl instructions, and `nidus-testing`. |
| `external-commerce` | Standalone external-user commerce API with crates.io-style dependencies, `nidus-sqlx` SQLite, `nidus-cache`, products, carts, inventory, idempotent checkout, env config, health/readiness, metrics, live curl instructions, and database/cache tests. |

Run an example with Cargo's package selector:

```bash
cargo run -p nidus-example-openapi
```

Server examples bind to `127.0.0.1:3000` and keep running until interrupted:

```bash
cargo run -p nidus-example-hello-world
curl http://127.0.0.1:3000/
```

HTTP server examples use `#[nidus::main]`, macro-defined application modules,
and either the facade builder path
`Nidus::create::<AppModule>().build_with_router(router).await?` or
`Nidus::create::<AppModule>().build().await?.map_router(...)` when applying
production defaults after module composition.

All examples are workspace members, so they are checked by the normal workspace
validation commands:

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

The examples also include focused tests for their important runtime paths:
HTTP routing uses `nidus_testing::TestApp`, module examples validate generated
metadata and container resolution, job examples cover sync and async execution,
and production-shaped examples cover health, metrics, request IDs, error
envelopes, limits, timeouts, CORS, guards, OpenAPI, validation, and persistence.
The adapter examples use SQLite in memory and Moka by default, keeping the
default example suite free of external service requirements.

## External Full-Stack Examples

`examples/external-support-desk` and `examples/external-commerce` are positioned
as "copy this when building a real app" examples. They are not normal workspace
members; each has its own `[workspace]` table and uses published dependency
declarations such as:

```toml
nidus = { package = "nidus-rs", version = "1.0.4", features = ["http"] }
nidus-sqlx = { version = "1.0.4", features = ["sqlite", "health", "observability"] }
nidus-cache = { version = "1.0.4", features = ["health", "observability"] }
nidus-testing = "1.0.4"
```

Verify both examples with:

```bash
bash scripts/verify-external-examples.sh
```

Before `1.0.4` is published to crates.io, use the pre-publish proof mode:

```bash
NIDUS_EXTERNAL_EXAMPLES_LOCAL_PATCH=1 bash scripts/verify-external-examples.sh
```

That mode copies both external examples into a temporary directory and appends
temporary `[patch.crates-io]` entries there only. It proves the examples against
the current local `1.0.4` crates without adding path dependencies to the
checked-in manifests. The default command remains the post-publish crates.io
verification path.

## Common Imports And Extension Traits

Use this import in app entrypoints and copyable examples:

```rust
use nidus::prelude::*;
```

It brings in common app-composition types and extension traits:

- `NidusApplicationExt` enables `Nidus::create::<AppModule>()`.
- The facade builder supports `.with_router(router)` and
  `.build_with_router(router)` for composing manual Axum routes with module
  routes.
- `ApplicationHttpExt` remains available for lower-level
  `Nidus::bootstrap::<AppModule>()?.with_router(router)` composition.
- `ApiDefaultsObservabilityExt` enables `.observability(&observability)` and
  observability-aware API defaults.

Common compile errors:

- `no method named with_router` after `Nidus::bootstrap`: import
  `ApplicationHttpExt` or `nidus::prelude::*`; after `Nidus::create`, call the
  builder's `.with_router(router)` before `.build().await`.
- `no method named listen` or `no method named into_router`: import
  `NidusApplicationExt` or `nidus::prelude::*`.
- `no method named observability`: import `ApiDefaultsObservabilityExt` or
  `nidus::prelude::*`.
