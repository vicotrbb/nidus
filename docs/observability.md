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
nidus = { package = "nidus-rs", version = "1.0.1", features = ["observability", "events", "jobs", "otel"] }
```

Official adapters expose observability hooks behind their own feature flags:

```toml
nidus-sqlx = { version = "1.0.1", features = ["sqlite", "health", "observability"] }
nidus-cache = { version = "1.0.1", features = ["health", "observability"] }
```

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

OpenTelemetry setup remains explicit. `otel_from_env()` builds resource config
from Nidus metadata and `OTEL_EXPORTER_OTLP_ENDPOINT`; it does not install a
process-global exporter or subscriber.

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
let observed = ObservedEventBus::new(
    EventBus::<UserCreated>::new(),
    observability.event_observer(),
);
observed.publish_named("user.created", UserCreated { id: 42 });

let runner = ObservedJobRunner::new(observability.job_observer());
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
