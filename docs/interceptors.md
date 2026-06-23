# Interceptors

Nidus should use Tower layers for middleware and interception behavior wherever practical.

Recommended interceptor concerns:

- request IDs
- tracing spans with stable route labels
- timeouts
- compression
- CORS
- rate limiting
- metrics hooks

```rust
let app = router.layer(route_trace_layer("/users/{id}"));
```

`request_id_layer()` propagates an incoming `x-request-id` response header when
present, preserves a handler-provided response ID, and generates one only when
neither exists.

Rate limiting uses Tower's built-in rate limiter:

```rust
let app = router.layer(rate_limit_layer(100, Duration::from_secs(60)));
```

Metrics hooks are backend-neutral. Implement `HttpMetricsHook` and attach it
with `route_metrics_layer("/users/{id}", metrics)` to record request and
response events without coupling the framework to a metrics backend. The same
hook can implement `on_error` to observe inner service failures that occur
before a response is produced.

Avoid a parallel middleware ecosystem unless Tower cannot express the behavior.
