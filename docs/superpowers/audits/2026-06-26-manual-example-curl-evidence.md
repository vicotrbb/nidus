# Manual Example Curl Evidence — 2026-06-26

Captured during the framework hardening pass. Each server was run on a free port,
curled, then stopped cleanly (`kill` + `wait`); no servers left running. Bodies
and headers captured from real `curl -i` output. Run against the post-Wave-1/Wave-2
working tree (commits up to `1e8d6c8`).

## hello-world  (`cargo run -p nidus-example-hello-world`, 127.0.0.1:3000)

- `GET /` → `HTTP/1.1 200 OK`, `content-type: text/plain; charset=utf-8`, body `hello from nidus`.

## rest-api  (`cargo run -p nidus-example-rest-api`, 127.0.0.1:3000)

- `GET /users/1` → `HTTP/1.1 200 OK`, `content-type: application/json`,
  body `{"id":1,"email":"user@nidus.dev","request_id":1}`.

## auth-api  (`cargo run -p nidus-example-auth-api`, 127.0.0.1:3000)

- `GET /me` → `HTTP/1.1 200 OK`, `content-type: text/plain; charset=utf-8`, body `authorized`.
- Validates the `guard_layer` fix (Wave 1.1) in a live server.

## openapi  (`cargo run -p nidus-example-openapi`, 127.0.0.1:3000)

Now a runnable server (Wave 2.4 fix; previously it printed JSON and exited).
- `GET /openapi.json` → `HTTP/1.1 200 OK`, `content-type: application/json`;
  `openapi: 3.1.0`, title `Nidus Example API`, paths `['/users','/users/{id}']`,
  schemas `['CreateUserDto','UserDto']`.
- `GET /docs` → `HTTP/1.1 200 OK`, `content-type: text/html; charset=utf-8`,
  `<title>Nidus Example API Documentation</title>`.
- `GET /users/42` → `HTTP/1.1 200 OK`, body `{"id":42,"email":"user@nidus.dev"}`.

## production-api  (`NIDUS_ADDR=127.0.0.1:3001 cargo run -p production-api`)

- `GET /health/live` → `200`.
- `GET /health/ready` → `200`, header `x-request-id: <uuid v4>`,
  body `{"status":"up","checks":{"cache":{"status":"up"},"database":{"status":"up"}}}`.
- `GET /metrics` → `200` (Prometheus text; health/metrics routes excluded from counters).
- `GET /users/1` → `200`, `x-request-id: <uuid>`,
  body `{"id":1,"email":"user@nidus.dev","request_id":"<same uuid>"}` (request-id propagation).
- `GET /users/domain-error` → `404 Not Found`, production envelope:
  `{"error":{"statusCode":404,"code":"not_found","message":"user not found","details":null,"timestamp":"...","path":"/users/domain-error","requestId":"..."}}`.
- `GET /slow` → `408` (50ms timeout layer).
- `GET /limited` twice within 60s → first `200`, second `429` (1 req/window rate limit).

## realworld-api  (`NIDUS_BIND_ADDR=127.0.0.1:3002 cargo run -p nidus-example-realworld-api`)

SQLite `sqlite::memory:` (no external services). Header `x-api-key: dev-secret` for guarded routes.
- `GET /health/live` → `200`.
- `GET /health/ready` → `{"status":"up","checks":{"database":{"status":"up"}}}` (DB health check live).
- `GET /metrics` → `200`.
- `POST /users` invalid (`{"email":"bad"}`) → `422` (validation rejects; `display_name` required).
- `POST /users` valid (`{"email":"ada@nidus.dev","display_name":"Ada"}`) → `201 Created`,
  body `{"id":1,"email":"ada@nidus.dev","display_name":"Ada"}`.
- `GET /users/1` → `200`.
- `POST /projects` without api key → rejected (guard); with `x-api-key: dev-secret` +
  `{"owner_id":1,"name":"Proj"}` → `201 Created`, body `{"id":1,"owner_id":1,"name":"Proj"}`
  (macro `#[guard]` path receives the header correctly — consistent with the Wave 1.1 fix).
- `GET /openapi.json` → paths include `/health`, `/users`, `/users/{id}`, `/projects`,
  `/projects/{id}`, `/projects/{project_id}/tasks`, `/tasks/{id}/complete`.

## Non-server examples (CLI/library)

`background-jobs`, `modular-monolith`, `sqlx-app`, `cache-app`, `integrations-production`
are not HTTP servers; their runtime behavior is covered by their inline unit tests, all of
which pass under `cargo test --workspace --all-features` (see verification baseline).

## Cleanup confirmation

After every run the server process was killed and `kill -0 $SRV` confirmed "no server running".
No background servers were left running.
