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

Avoid a parallel middleware ecosystem unless Tower cannot express the behavior.
