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

Rate limiting uses Tower's built-in rate limiter:

```rust
let app = router.layer(rate_limit_layer(100, Duration::from_secs(60)));
```

Metrics hooks are backend-neutral. Implement `HttpMetricsHook` and attach it
with `route_metrics_layer("/users/{id}", metrics)` to record request and
response events without coupling the framework to a metrics backend.

Avoid a parallel middleware ecosystem unless Tower cannot express the behavior.
