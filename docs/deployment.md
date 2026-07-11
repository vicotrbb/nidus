# Deployment

Nidus applications are normal Rust binaries.

Recommended production defaults:

- build with `--release`
- configure addresses, logging, and secrets through typed config
- use `tracing` subscribers appropriate for the deployment platform
- place reverse proxy, TLS, compression, and rate limiting where they best fit the system

Nidus should not impose a hosting platform.

## Public Website Deployment

The canonical public documentation domain is `https://rustnidus.com/`.
Repository-side Pages builds use `NIDUS_SITE_BASE=/` and
`NIDUS_SITE_DOMAIN=rustnidus.com`, which emits `website/dist/CNAME` and root
asset links for the custom-domain deployment.

Verify the canonical output locally:

```bash
npm run verify:domain --prefix website
```

The static generator also supports the GitHub project-page base for fallback or
diagnostics:

```bash
npm run verify:project --prefix website
```

GitHub Pages still requires repository settings outside this checkout: Pages
must be enabled for the Actions workflow, the custom domain must be set to
`rustnidus.com`, and DNS must point the domain at GitHub Pages through the
provider that manages `rustnidus.com`.

After the crates are published, verify the external package surface before
announcing the release:

```bash
bash scripts/verify-published-release.sh 1.0.10
```

The verifier checks crates.io, docs.rs, and the standalone external examples.

## Build

Build release binaries with Cargo:

```bash
cargo build --release
```

For workspace examples or generated applications, run the normal validation
commands before packaging:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --all-features --no-deps
```

## Runtime Configuration

Keep runtime configuration outside the binary. Load defaults from explicit JSON
or pair sources, overlay environment variables with a prefix, deserialize into a
typed struct, and fail startup if required values are missing.

```rust
let config = Config::from_json_file("config/defaults.json")?
    .merge(Config::from_env_prefix("APP"));
let settings = config.deserialize::<AppConfig>()?;
```

Secrets should come from the deployment platform's secret mechanism and enter
the app through typed config, not hard-coded constants.

Adapter crates such as `nidus-sqlx` and `nidus-cache` should read the same typed
configuration at startup. Keep URLs, pool sizes, cache namespaces, and TTLs in
application config, then pass explicit values into adapter builders.

## Common Imports And Extension Traits

Deployment entrypoints should normally start with:

```rust
use nidus::prelude::*;
```

That import keeps app-composition types and extension traits visible:

- `NidusApplicationExt` enables `Nidus::create::<AppModule>()`.
- The facade builder supports `.with_router(router)` and
  `.build_with_router(router)` for composing manual Axum routes with module
  routes.
- `ApplicationHttpExt` remains available for lower-level
  `Nidus::bootstrap::<AppModule>()?.with_router(router)` composition.
- `ApiDefaultsObservabilityExt` enables `.observability(&observability)` and
  observability-aware API defaults.

Common compile errors usually mean one of those traits is missing:

- `no method named with_router` after `Nidus::bootstrap`: import
  `ApplicationHttpExt` or `nidus::prelude::*`; after `Nidus::create`, call the
  builder's `.with_router(router)` before `.build().await`.
- `no method named listen` or `no method named into_router`: import
  `NidusApplicationExt` or `nidus::prelude::*`.
- `no method named observability`: import `ApiDefaultsObservabilityExt` or
  `nidus::prelude::*`.

## HTTP Boundary

Nidus builds on Axum and Tower, so standard Rust deployment patterns apply:

- Bind to an explicit address such as `0.0.0.0:3000` in containers.
- Terminate TLS at the load balancer, reverse proxy, or app depending on your platform.
- Use Tower middleware or upstream infrastructure for compression, CORS, timeout, and rate limiting.
- Emit structured tracing events through a subscriber configured by the app.

`ApiDefaults::production(service_name)` provides a higher-level starting point
for common API boundary concerns while still returning a normal Axum `Router`.
It composes middleware and routes; it does not replace Axum routing or prevent
additional Tower layers.

```rust
use nidus::prelude::*;

let observability = Observability::production("users-api")
    .version("1.2.3")
    .environment("prod")
    .prometheus()
    .tracing();
let app = ApiDefaults::production("users-api")
    .observability(&observability)
    .request_ids(RequestIdConfig::production().mode(RequestIdMode::Strict))
    .body_limit(1024 * 1024)
    .timeout(std::time::Duration::from_secs(30))
    .apply(router.merge(observability.routes()));
```

Default-on concerns have an opt-out or replacement hook:

- `without_request_ids()` or `request_ids(RequestIdConfig::...)`
- `without_request_context()`
- `without_error_envelope()`
- `without_health()` or `health(HealthRegistry::...)`
- `without_body_limit()` or `body_limit(max_bytes)`
- `without_security_headers()` or `security_headers()`
- `without_timeout()` or `timeout(duration)`

Metrics and rate limiting are opt-in:

- `observability(&observability)` installs request recording when Prometheus
  is enabled; merge `observability.routes()` separately to expose `/metrics`
- `metrics(PrometheusMetrics::new())` remains available as the lower-level HTTP-only path
- `rate_limit(RateLimitConfig::...)` installs rate limiting

The built-in `listen` and `serve` helpers populate Axum `ConnectInfo`, so
`client_ip_identity()` can classify by the direct peer IP and ignores
`X-Forwarded-For` by default. If the deployment intentionally trusts a reverse
proxy to set `X-Forwarded-For`, use `trusted_proxy_client_ip_identity([...])`
with explicit trusted proxy IPs.

Use the lower-level helpers directly when an application needs a different
composition order.

## Logging And Tracing

`LoggingConfig::production(service)` builds a JSON `tracing-subscriber`
configuration for production log pipelines. `LoggingConfig::development(service)`
uses pretty local output. Subscriber setup is explicit and optional:

```rust
let _ = LoggingConfig::production("users-api")
    .version("1.2.3")
    .environment("prod")
    .redact_header("x-api-key")
    .init();
```

`StructuredMakeSpan` records service, environment, request ID, method, route,
target, and trace fields on HTTP spans. Header redaction is exposed as config so
applications can use the same policy in their own logs.

## OpenTelemetry

OpenTelemetry helpers are behind the `otel` feature. They provide
backend-optional building blocks: OTLP endpoint config, resource attributes,
W3C `traceparent` extraction/injection, observed span helpers, exception
recording, and shutdown hooks.

```toml
nidus = { package = "nidus-rs", version = "1.0.10", features = ["otel"] }
```

```rust
let otel = OtelConfig::new("users-api")
    .version("1.2.3")
    .environment("prod")
    .with_otlp_endpoint("http://collector:4317");
```

The helpers do not force a specific exporter. Applications can map
`OtelConfig::resource_attributes()` into the OpenTelemetry SDK they choose.

## Health And Shutdown

Expose health routes for the platform's readiness checks. `HealthRegistry`
ships `/health/live` and `/health/ready` helpers, supports named async checks,
applies per-check timeouts, runs readiness checks in parallel, and can hide
diagnostic messages from production responses.

```rust
let health = HealthRegistry::new()
    .live_check_sync("process", HealthStatus::up)
    .ready_check("database", || async { HealthStatus::up() })
    .hide_details();
let app = router.merge(health.routes());
```

Adapters compiled with their `health` feature can contribute readiness checks
from their typed providers:

```rust
let database = container.resolve::<nidus_sqlx::SqlitePoolProvider>()?;
let health = database.register_ready_check(HealthRegistry::new(), "database");
```

For configured async adapters such as SQLx, register the provider explicitly
with its builder or a module async initializer before resolving it from the
container. `ModuleBuilder::provider_typed` is only appropriate for providers
that implement synchronous default registration, such as the local Moka cache
provider.

Keep startup validation strict so a bad config, missing provider, invalid module
graph, or failed lifecycle hook prevents serving traffic.

When lifecycle hooks manage external resources, register them with clear startup
and shutdown behavior so tests and production shutdown paths exercise the same
logic.
