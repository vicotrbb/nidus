# Nidus Launchpad API

`launchpad-api` is the compact 1.0 tour of Nidus in one runnable service. It uses:

- module composition and dependency injection
- controller macros and typed extractors
- request validation with `ValidatedJson`
- route guards with injected auth state
- OpenAPI route metadata and generated `/openapi.json`
- SQLx SQLite and Moka cache adapters
- request IDs, request context, health, metrics, timeout, body limit, security headers, CORS, and error envelopes
- events, sync jobs, async jobs, and observed workflow hooks
- `nidus-testing` integration tests

Run it locally:

```bash
cargo run -p nidus-example-launchpad-api
```

Exercise the main flow:

```bash
curl -s http://127.0.0.1:4100/health
curl -s -X POST http://127.0.0.1:4100/launches \
  -H 'content-type: application/json' \
  -H 'x-api-key: launch-secret' \
  -d '{"name":"Nidus 1.0","owner_email":"owner@nidus.dev"}'
curl -s http://127.0.0.1:4100/launches/1 -H 'x-api-key: launch-secret'
curl -s http://127.0.0.1:4100/ops/workflow -H 'x-api-key: launch-secret'
curl -s http://127.0.0.1:4100/openapi.json
```

Proof commands:

```bash
cargo test -p nidus-example-launchpad-api --all-targets
cargo run -p nidus-example-launchpad-api
```

