# Production Defaults

Nidus production defaults are opt-in composition helpers over Axum and Tower.
They return normal routers and layers so applications can inspect, replace, or
reorder the boundary.

```rust
use nidus::prelude::*;

let app = Nidus::create::<AppModule>()
    .build()
    .await?
    .map_router(|router| {
        ApiDefaults::production("orders-api")
            .without_metrics()
            .apply(router)
    });
```

## Included HTTP Defaults

- request IDs and request context
- health and readiness routes
- Prometheus-style metrics route when enabled
- CORS, body limits, timeout responses, security headers, and structured logging
- production error envelopes
- OpenTelemetry trace-context helpers when the `otel` feature is enabled

## Observability Defaults

```rust
let observability = Observability::production("orders-api")
    .version(env!("CARGO_PKG_VERSION"))
    .environment("prod")
    .prometheus()
    .tracing()
    .otel_from_env();
```

Automatic instrumentation applies where Nidus owns the integration point: HTTP
middleware, `ObservedEventBus`, `ObservedJobRunner`, module validation, and
official adapter builders. Raw SQLx queries, raw cache clients, ORMs, queues,
and HTTP clients remain explicit application instrumentation.
