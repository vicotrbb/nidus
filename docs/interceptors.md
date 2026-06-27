# Interceptors

Nidus should use Tower layers for middleware and interception behavior wherever practical.

Recommended interceptor concerns:

- request IDs
- request dependency scopes
- tracing spans with stable route labels
- structured logging spans
- timeouts
- body limits
- security headers
- compression
- CORS
- rate limiting
- metrics hooks

```rust
let app = router.layer(route_trace_layer("/users/{id}"));
```

`request_id_layer()` propagates an incoming `x-request-id` response header when
present, preserves a handler-provided response ID, and generates one only when
neither exists. Generated IDs are UUID v4 values, but this legacy layer does
not validate incoming IDs or populate `RequestContext`.

Production APIs should prefer `validated_request_id_layer(...)`, which validates
incoming IDs, generates UUID v4 values by default, stores the final ID in
request extensions, and writes it to the response header.

```rust
let app = router.layer(validated_request_id_layer(
    RequestIdConfig::production().mode(RequestIdMode::Strict),
));
```

Use `RequestIdMode::Permissive` to replace malformed incoming values instead of
rejecting them. `RequestIdConfig::header_name(...)` and
`RequestIdConfig::generator(...)` customize the boundary. Custom generators must
return values that can be stored in HTTP headers. If a generator returns an
invalid header value, the validated middleware returns a structured
`500 Internal Server Error` with code `invalid_generated_request_id` before the
request reaches the handler.

`request_context_layer()` attaches `RequestContext` to request extensions and
makes it extractable by handlers. The context carries request ID, correlation
ID, method, matched route when Axum provides it, raw path, trace fields where
available, client kind, and optional application user, tenant, and session
fields.

```rust
async fn handler(context: RequestContext) -> String {
    context.request_id().to_owned()
}
```

`request_scope_layer(container)` creates one `RequestScope` per HTTP request and
stores it in request extensions. Handlers can use `RequestScoped<T>` to resolve
request-lifetime providers without sharing them across requests:

```rust
async fn handler(context: RequestScoped<RequestContext>) -> &'static str {
    "ok"
}

let app = router.route("/me", get(handler)).layer(request_scope_layer(container));
```

Rate limiting uses Tower's built-in rate limiter:

```rust
let app = router.layer(rate_limit_layer(100, Duration::from_secs(60)));
```

For production-shaped boundaries, use `RateLimitConfig` with an identity
extractor and store adapter. Nidus ships `InMemoryRateLimitStore` for local
development and single-process apps; distributed stores can implement
`RateLimitStore`. The in-memory store prunes expired identity windows when it is
checked, but it is process-local, resets on restart, and is not a distributed
production rate-limit backend.

```rust
let app = router.layer(
    RateLimitConfig::new(100, Duration::from_secs(60), InMemoryRateLimitStore::new())
        .identity(client_ip_identity())
        .fail_closed()
        .layer(),
);
```

`client_ip_identity()` uses the connected peer IP from Axum `ConnectInfo` and
does not trust `X-Forwarded-For`. The Nidus `listen` and `serve` helpers
populate `ConnectInfo` for normal TCP serving. When an app intentionally runs
behind a known reverse proxy that owns `X-Forwarded-For`, use
`trusted_proxy_client_ip_identity([...])` and pass the trusted proxy IPs
explicitly:

```rust
let proxy = "127.0.0.1".parse::<std::net::IpAddr>()?;
let app = router.layer(
    RateLimitConfig::new(100, Duration::from_secs(60), InMemoryRateLimitStore::new())
        .identity(trusted_proxy_client_ip_identity([proxy]))
        .fail_closed()
        .layer(),
);
# Ok::<(), std::net::AddrParseError>(())
```

Requests from untrusted peers ignore `X-Forwarded-For` and use the direct peer
IP. Requests without peer information fall back to the shared `"anonymous"`
identity, which is suitable for in-memory tests but should not be treated as a
multi-client production boundary.

The layer emits `RateLimit-Limit`, `RateLimit-Remaining`, `RateLimit-Reset`,
and `Retry-After` headers when a request is rejected.

## Security Boundary

`security_headers_layer()` adds conservative API response headers:
`X-Content-Type-Options`, `X-Frame-Options`, and `Referrer-Policy`.
`body_limit_layer(max_bytes)` rejects requests with a declared oversized
`Content-Length`; it does not count streamed bytes. Use
`streaming_body_limit_layer(max_bytes)` when you need the request body wrapped
and capped as downstream extractors or handlers read it. `webhook_body_limit_layer(max_bytes)`
uses the declared `Content-Length` boundary with an explicit response marker for
webhook/raw-body routes.

```rust
let app = router
    .layer(security_headers_layer())
    .layer(body_limit_layer(1024 * 1024));
```

Use `timeout_response_layer(duration)` when the application wants elapsed work
mapped to an HTTP `408 Request Timeout` response instead of Tower's raw timeout
error.

`cors_layer()` remains a permissive development helper. Use
`cors_origin_layer(origin)` when an API should allow one explicit origin while
keeping methods and headers configured through Tower HTTP.

Metrics hooks are backend-neutral. Implement `HttpMetricsHook` and attach it
with `route_metrics_layer("/users/{id}", metrics)` to record request and
response events without coupling the framework to a metrics backend. The same
hook can implement `on_error` to observe inner service failures that occur
before a response is produced.

`PrometheusMetrics` is an in-process implementation for examples, tests, and
simple deployments. Metrics use Axum matched route patterns when available and
skip `/health/live`, `/health/ready`, and `/metrics` by default.

```rust
let metrics = PrometheusMetrics::new();
let app = router.merge(metrics.routes()).layer(metrics.layer());
```

Avoid a parallel middleware ecosystem unless Tower cannot express the behavior.
