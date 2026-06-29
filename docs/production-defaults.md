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
        ApiDefaults::production("orders-api").apply(router)
    });
```

## Included HTTP Defaults

- request IDs and request context
- health and readiness routes
- Prometheus-style metrics route when enabled
- CORS, body limits, timeout responses, security headers, and structured logging
- production error envelopes
- unmatched-route fallback returning the Nidus `not_found` JSON envelope
- OpenTelemetry trace-context helpers when the `otel` feature is enabled

The unmatched-route fallback is installed by default in
`ApiDefaults::production(...).apply(router)`. Missing routes therefore receive
the same production envelope as handler-created `HttpError::not_found(...)`
responses, including request ID, path, timestamp, JSON content type, and
security headers. Use `without_not_found_fallback()` when an application
installs its own Axum fallback before applying defaults.

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
