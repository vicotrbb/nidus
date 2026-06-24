# Nidus Real-World API Example

This example shows a small production-shaped "team tasks" API built with Nidus.
It keeps the framework wiring visible while splitting the application into
modules, controllers, services, repositories, DTOs, auth, database, and
observability files.

The example demonstrates:

- Nidus controllers and route macros
- Nidus module metadata for app, database, auth, users, and projects modules
- Dependency injection through `#[injectable]` providers and `Container`
- Request validation with `ValidatedJson<T>` and `validator`
- SQLite-backed persistence with local SQLx SQLite dependencies
- Repository, service, and controller layering
- OpenAPI route metadata and served docs at `/openapi.json` and `/docs`
- Tracing middleware and structured application logs
- Tests with `nidus_testing::TestApp`

The Nidus `#[guard]` macro currently records route metadata. Executable guard
middleware does not receive HTTP request headers, so this example uses the guard
metadata on protected routes and an Axum middleware for real `x-api-key`
enforcement. Tests cover both the metadata and the executable failure response.

## Running

```bash
cargo run -p nidus-example-realworld-api
```

The server listens on `127.0.0.1:3000` by default and uses an in-memory SQLite
database unless `NIDUS_DATABASE_URL` is set. The development API key defaults to
`dev-secret` and can be changed with `NIDUS_API_KEY`.

```bash
curl http://127.0.0.1:3000/health

curl -X POST http://127.0.0.1:3000/users \
  -H 'content-type: application/json' \
  -d '{"email":"owner@nidus.dev","display_name":"Owner"}'

curl -X POST http://127.0.0.1:3000/projects \
  -H 'content-type: application/json' \
  -H 'x-api-key: dev-secret' \
  -d '{"owner_id":1,"name":"Launch API"}'
```
