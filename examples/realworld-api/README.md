# Nidus Real-World API Example

This example shows a production-shaped "team tasks" API built with Nidus. It
keeps the framework wiring visible while splitting the application into modules,
controllers, services, repositories, DTOs, auth, database, operations, and
observability files.

The example demonstrates:

- `#[nidus::main]` and `Nidus::create::<AppModule>()`
- Nidus controllers and route macros
- Nidus module metadata for app, database, auth, users, and projects modules
- Manual `ModuleBuilder` usage with an async SQLx database initializer
- Dependency injection through `#[injectable]`, `Inject<T>`, singleton config,
  and request-scoped extraction with `RequestScoped<T>`
- Request validation with `ValidatedJson<T>` and `garde`
- SQLite-backed persistence with local SQLx SQLite dependencies
- Repository, service, and controller layering
- OpenAPI route metadata and served docs at `/openapi.json` and `/docs`
- `ApiDefaults::production` with strict request IDs, request context, production
  error envelopes, health, metrics, security headers, body limits, and timeouts
- `/health/live` and `/health/ready` through `HealthRegistry`, including async
  readiness checks
- Prometheus-format HTTP metrics at `/metrics`
- Deterministic production-style rate limiting on `/ops/limited`
- Explicit CORS origin configuration for `https://console.nidus.dev`
- Webhook/raw-body body limit helper on `/ops/webhook`
- `RequestContext` extraction through `/context`
- `LoggingConfig` for JSON/development logging and redaction metadata
- `nidus_config::Config`-backed application configuration
- OTel trace-context helper usage when the Nidus `otel` feature is enabled
- `EventBus`, `ObservedEventBus`, `JobQueue`, `AsyncJobQueue`, and
  `ObservedJobRunner` in the observed workflow route
- Executable guards for `x-api-key` protected project and task routes
- Tests with `nidus_testing::TestApp`

## Running

```bash
cargo run -p nidus-example-realworld-api
```

The server listens on `127.0.0.1:3000` by default and uses an in-memory SQLite
database unless `NIDUS_DATABASE_URL` is set. The development API key defaults to
`dev-secret` and can be changed with `NIDUS_API_KEY`. Configuration is loaded
through `nidus_config::Config` using the `NIDUS_` prefix, for example
`NIDUS_ALLOWED_ORIGIN`, `NIDUS_ENVIRONMENT`, and `NIDUS_LOG_FORMAT`.

```bash
curl http://127.0.0.1:3000/health
curl http://127.0.0.1:3000/health/live
curl http://127.0.0.1:3000/health/ready
curl http://127.0.0.1:3000/metrics

curl -X POST http://127.0.0.1:3000/users \
  -H 'content-type: application/json' \
  -d '{"email":"owner@nidus.dev","display_name":"Owner"}'

curl -X POST http://127.0.0.1:3000/projects \
  -H 'content-type: application/json' \
  -H 'x-api-key: dev-secret' \
  -d '{"owner_id":1,"name":"Launch API"}'

curl http://127.0.0.1:3000/context \
  -H 'x-request-id: 018f4ad7-56ce-4f6a-a759-29f4438d8d78'

curl http://127.0.0.1:3000/ops/limited -H 'x-api-key: dev-secret'
curl -X POST http://127.0.0.1:3000/ops/workflows/observed \
  -H 'x-request-id: 018f4ad7-56ce-4f6a-a759-29f4438d8d78'
```
