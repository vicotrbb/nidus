# Interceptors

Nidus should use Tower layers for middleware and interception behavior wherever practical.

Recommended interceptor concerns:

- request IDs
- tracing spans
- timeouts
- compression
- CORS
- rate limiting
- metrics hooks

Avoid a parallel middleware ecosystem unless Tower cannot express the behavior.

