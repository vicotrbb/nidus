# Observability

`nidus-observability` is the recommended production composition layer for
logs, traces, metrics, events, jobs, lifecycle validation, HTTP, and official
adapter operations.

It is additive. The lower-level APIs remain available:

- `PrometheusMetrics`
- `HttpMetricsHook`
- `LoggingConfig`
- `StructuredMakeSpan`
- `ObservedEventBus`
- `ObservedJobRunner`
- `HealthRegistry`
- `OtelConfig`
- `trace_layer`
- `route_trace_layer`

## Install

Enable the facade feature when using `nidus::prelude::*`:

```toml
nidus = { package = "nidus-rs", version = "1.0.11", features = ["observability", "events", "jobs", "otel"] }
```

Official adapters expose observability hooks behind their own feature flags:

```toml
nidus-sqlx = { version = "1.0.11", features = ["sqlite", "health", "observability"] }
nidus-cache = { version = "1.0.11", features = ["health", "observability"] }
nidus-opentelemetry = { version = "1.0.11", features = ["health", "dashboard"] }
nidus-sentry = { version = "1.0.11", features = ["health", "dashboard"] }
```

## Common Imports And Extension Traits

Use the prelude when composing observability at the application boundary:

```rust
use nidus::prelude::*;
```

The prelude imports:

- `NidusApplicationExt`, which enables `Nidus::create::<AppModule>()`.
- The facade builder supports `.with_router(router)` and
  `.build_with_router(router)` for composing manual Axum routes with module
  routes.
- `ApplicationHttpExt`, which remains available for lower-level
  `Nidus::bootstrap::<AppModule>()?.with_router(router)` composition.
- `ApiDefaultsObservabilityExt`, which enables
  `.observability(&observability)` and observability-aware API defaults.

Common compile errors:

- `no method named with_router` after `Nidus::bootstrap`: import
  `ApplicationHttpExt` or `nidus::prelude::*`; after `Nidus::create`, call the
  builder's `.with_router(router)` before `.build().await`.
- `no method named listen` or `no method named into_router`: import
  `NidusApplicationExt` or `nidus::prelude::*`.
- `no method named observability`: import `ApiDefaultsObservabilityExt` or
  `nidus::prelude::*`.

## Application-Wide Setup

```rust
use nidus::prelude::*;

let observability = Observability::production("users-api")
    .version(env!("CARGO_PKG_VERSION"))
    .environment("prod")
    .prometheus()
    .tracing()
    .otel_from_env();

let app = Nidus::create::<AppModule>()
    .with_observability(observability.clone())
    .build()
    .await?;
```

`with_observability` merges `/metrics`, applies HTTP metrics when Prometheus is
enabled, records module graph validation metrics, and installs the standard HTTP
trace layer when `.tracing()` is set.

The facade's existing `otel_from_env()` remains a metadata/configuration helper
for compatibility. Install `nidus-opentelemetry` when the application needs a
real SDK pipeline and OTLP exporter. Neither path installs a process-global
subscriber; the application retains explicit ownership and shutdown order.

## Tower-First Setup

Use the same object when composing routers directly:

```rust
use nidus::prelude::*;

let observability = Observability::production("users-api")
    .prometheus()
    .tracing();

let router = router
    .merge(observability.routes())
    .layer(observability.http_layer());
```

`ApiDefaults` has an extension trait when `Observability` is in scope:

```rust
let app = ApiDefaults::production("users-api")
    .observability(&observability)
    .apply(router.merge(observability.routes()));
```

## Opt-Outs And Caps

```rust
let observability = Observability::production("api")
    .prometheus()
    .without_http_metrics()
    .without_event_metrics()
    .without_job_metrics()
    .without_adapter_instrumentation()
    .max_series(500)
    .exclude_route("/health/live");
```

Disabling a surface removes telemetry for that surface only. Application
behavior stays unchanged.

`max_series` caps low-cardinality labels per non-HTTP metric family. After the
cap is reached, new labels collapse into `"<overflow>"`. HTTP metrics use the
same overflow behavior through `PrometheusMetrics::with_max_series`.

## Metrics

HTTP metric names are unchanged:

- `nidus_http_requests_total`
- `nidus_http_request_duration_seconds`
- `nidus_http_in_flight_requests`
- `nidus_http_errors_total`

The observability layer adds first-class metrics for Nidus-owned surfaces:

- `nidus_events_published_total{event}`
- `nidus_jobs_started_total{job}`
- `nidus_jobs_finished_total{job,status}`
- `nidus_job_duration_seconds{job,status}`
- `nidus_lifecycle_total{operation,status}`
- `nidus_lifecycle_duration_seconds{operation,status}`
- `nidus_adapter_operations_total{adapter,operation,status}`
- `nidus_adapter_operation_duration_seconds{adapter,operation,status}`

Labels should be stable names such as route templates, event names, job names,
lifecycle operation names, adapter names, and operation names. Do not put user
IDs, raw URLs, SQL text, cache keys, or tenant-controlled strings into labels.

## Events And Jobs

Use Nidus-owned wrappers to get event and job metrics:

```rust
let observed = observability.observed_event_bus::<UserCreated>();
observed.publish_named("user.created", UserCreated { id: 42 });

let runner = observability.job_runner();
runner.run(&SendDigest)?;
```

Plain `EventBus::publish`, `JobQueue::run_all`, and `AsyncJobQueue::run_all`
continue to work, but they do not emit observability metrics unless routed
through the observed wrappers.

## Adapter Operations

Official adapters expose explicit hooks for adapter-owned operations:

```rust
let database = nidus_sqlx::SqlitePoolProvider::builder()
    .database_url("sqlite::memory:")
    .observability(observability.adapter_observer())
    .connect()
    .await?;

let cache = nidus_cache::MokaCacheProvider::builder()
    .namespace("users")
    .observability(observability.adapter_observer())
    .build();
```

This instruments operations owned by the adapter, such as pool connection,
adapter health checks, and Moka provider `insert`, `get`, and `invalidate`.

It does not automatically trace arbitrary raw SQLx queries, raw Moka calls,
HTTP clients, ORMs, queues, or cache clients used outside Nidus wrappers. Keep
those integrations explicit in application code or in a dedicated adapter.

## Tracing Conventions

Nidus-owned paths emit stable span names:

- `http.request` through HTTP tracing layers and structured span makers
- `event.publish`
- `job.run`
- `lifecycle.startup`
- `lifecycle.shutdown`
- `module.graph.validate`
- `adapter.operation`

Export depends on the subscriber/exporter the application installs.

## OpenTelemetry SDK and OTLP

`nidus-opentelemetry` builds a real OpenTelemetry SDK tracer provider with a
bounded `BatchSpanProcessor`, parent-based trace ID ratio sampling, resource
attributes, the `tracing-opentelemetry` bridge, and either OTLP/gRPC or
OTLP/HTTP protobuf over rustls.

```rust
use nidus_opentelemetry::{OpenTelemetryConfig, OpenTelemetryPipeline};
use tracing_subscriber::prelude::*;

let config = OpenTelemetryConfig::from_env("users-api")?
    .with_service_version(env!("CARGO_PKG_VERSION"))
    .with_environment("production")
    .with_batching(8_192, 512, std::time::Duration::from_secs(5))?;
let pipeline = OpenTelemetryPipeline::init(config)?;
let subscriber = tracing_subscriber::registry().with(pipeline.tracing_layer());
let _default = tracing::subscriber::set_default(subscriber);
```

Standard `traceparent`, `tracestate`, and baggage propagation is available
through `inject_current_context`, `extract_context`, and
`set_parent_from_headers`. `install_global_propagator` is explicit for clients
that require the upstream global propagator; local header helpers do not mutate
global state.

On shutdown, stop accepting work, drain application tasks, then call
`force_flush().await` and `shutdown().await`. Both move blocking SDK work off
Tokio worker threads and are safe to call repeatedly. HTTPS endpoints are
required unless loopback plaintext is explicitly enabled. Exporter header
values are redacted from `Debug` output.

`register` makes the pipeline available through typed DI, and its lifecycle
hook shuts the provider down. With the `health` feature,
`register_ready_check` reports whether shutdown has begun. With `dashboard`,
`record_dashboard_status` writes a redaction-safe readiness operation to the
dashboard timeline.

## Sentry

`nidus-sentry` owns client initialization, tracing and panic capture, bounded
duplicate suppression, request scrubbing, and graceful transport flushing.
Its Tower layer creates an isolated hub per request and names performance
transactions from Axum's matched route rather than the raw URL, preventing
cross-request scope leakage and high-cardinality transaction names.

```rust
use axum::body::Body;
use nidus_sentry::{SentryConfig, SentryIntegration};
use tracing_subscriber::prelude::*;

let sentry = SentryIntegration::init(
    SentryConfig::from_env()?
        .with_release(env!("CARGO_PKG_VERSION"))
        .with_environment("production")
        .with_sample_rates(1.0, 0.1)?,
)?;
let router = router.layer(sentry.tower_layer::<Body>());
let subscriber = tracing_subscriber::registry().with(sentry.tracing_layer());
```

Authorization, cookies, proxy credentials, API keys, query strings, request
bodies, users, and other PII-bearing fields are removed before transport.
`send_default_pii` stays disabled. Errors can be captured through
`capture_error`, error-level tracing events become Sentry events, warnings and
information become breadcrumbs, and the panic integration captures panics.
Call `flush().await` before shutdown when needed, then `shutdown().await` to
restore the prior hub client and drain the transport off the async runtime.

`register` makes the integration available through typed DI, and its lifecycle
hook performs graceful shutdown. The optional `health` and `dashboard`
features provide `register_ready_check` and `record_dashboard_status` with the
same composition model as the OpenTelemetry pipeline.

The integration tests use in-memory exporters/transports and prove batching,
propagation, flush idempotence, concurrent Tower request isolation,
matched-route transaction names, redaction, and deduplication without mutating
the process-global subscriber.
