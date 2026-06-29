# External Support Desk Example

This is a copyable Nidus application that uses published crates.io dependencies
instead of workspace path dependencies. It is intended for external users who
want a real app shape before adding their own database.

## What It Proves

- `use nidus::prelude::*;` brings application, HTTP, controller, and extension
  traits into scope.
- `Nidus::create::<AppModule>()` builds a module-driven app.
- `ApplicationHttpExt` enables `.with_router(...)` when attaching routers to a
  bootstrapped application.
- `NidusApplicationExt` enables `.listen(...)` and `.into_router()`.
- `ApiDefaults` installs request IDs, request context, health routes, and error
  envelopes.
- Nidus DI wires `TicketStore`, `TicketRepository`, `TicketService`, and
  `TicketsController`.
- Tests use `nidus-testing` for in-memory HTTP checks.

## Dependencies

```toml
nidus = { package = "nidus-rs", version = "1.0.2", features = ["http"] }
nidus-testing = "1.0.2"
```

The manifest has its own `[workspace]` table so this folder remains a standalone
external-consumer project even when it lives inside the Nidus repository.

## Run

```bash
cd examples/external-support-desk
cargo run
```

Set a different address when needed:

```bash
NIDUS_ADDR=127.0.0.1:4301 cargo run
```

## Test

```bash
cargo test
```

## Curl Walkthrough

```bash
curl -i http://127.0.0.1:4301/health/live

curl -i -X POST http://127.0.0.1:4301/tickets \
  -H 'content-type: application/json' \
  -H 'x-api-key: support-secret' \
  -H 'x-request-id: 018f4ad7-56ce-4f6a-a759-29f4438d8d78' \
  -d '{"subject":"Cannot deploy","description":"Pipeline is blocked","priority":"urgent"}'

curl -i -X POST http://127.0.0.1:4301/tickets/1/assign \
  -H 'content-type: application/json' \
  -H 'x-api-key: support-secret' \
  -H 'x-request-id: 018f4ad7-56ce-4f6a-a759-29f4438d8d79' \
  -d '{"assignee":"Ada"}'

curl -i -X POST http://127.0.0.1:4301/tickets/1/comments \
  -H 'content-type: application/json' \
  -H 'x-api-key: support-secret' \
  -H 'x-request-id: 018f4ad7-56ce-4f6a-a759-29f4438d8d7a' \
  -d '{"author":"Grace","body":"Looking now"}'

curl -i -X POST http://127.0.0.1:4301/tickets/1/close \
  -H 'x-api-key: support-secret' \
  -H 'x-request-id: 018f4ad7-56ce-4f6a-a759-29f4438d8d7b'

curl -i -H 'x-api-key: support-secret' http://127.0.0.1:4301/tickets/1
```

Expected responses include `201 Created` for ticket creation, `200 OK` for
assignment/comment/close/read, a response `x-request-id` header, and a JSON
`request_id` field matching the incoming request ID.

Error checks:

```bash
curl -i http://127.0.0.1:4301/tickets

curl -i -X POST http://127.0.0.1:4301/tickets \
  -H 'content-type: application/json' \
  -H 'x-api-key: support-secret' \
  -d '{"subject":"","description":"missing subject","priority":"normal"}'

curl -i -H 'x-api-key: support-secret' http://127.0.0.1:4301/tickets/404
```

Expected statuses are `401 Unauthorized`, `400 Bad Request`, and `404 Not Found`.

## Common Imports And Extension Traits

Use the prelude at application entrypoints:

```rust
use nidus::prelude::*;
```

This imports the extension traits users most often miss:

- `ApplicationHttpExt` for `.with_router(...)`.
- `NidusApplicationExt` for `Nidus::create::<AppModule>()`, `.listen(...)`, and
  `.into_router()`.
- `ApiDefaultsObservabilityExt` when the `observability` feature is enabled and
  `.observability(&observability)` is used with API defaults.
