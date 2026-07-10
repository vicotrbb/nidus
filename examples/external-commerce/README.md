# External Commerce Example

This is a copyable Nidus commerce API that uses published crates.io dependency
declarations, SQLite through `nidus-sqlx`, and a Moka cache through
`nidus-cache`. It is intentionally standalone so external users can copy the
folder out of the Nidus repository and keep the same manifest shape.

## What It Proves

- `use nidus::prelude::*;` is the recommended app-entrypoint import.
- `Nidus::create::<AppModule>()` composes a module graph with async
  infrastructure initialization.
- The facade builder supports `.with_router(router)` and
  `.build_with_router(router)` when composing manual Axum routes.
- `ApplicationHttpExt` remains available for lower-level bootstrapped
  application composition.
- `ApiDefaultsObservabilityExt` enables
  `.observability(&observability)` on `ApiDefaults`.
- `nidus-sqlx` registers a SQLite pool provider during bootstrap.
- `nidus-cache` registers a Moka cache provider and the service invalidates the
  product cache after checkout.
- `nidus-testing` drives HTTP tests without opening a TCP port.

## Dependencies

```toml
nidus = { package = "nidus-rs", version = "1.0.8", features = ["http", "observability"] }
nidus-sqlx = { version = "1.0.8", features = ["sqlite", "health", "observability"] }
nidus-cache = { version = "1.0.8", features = ["health", "observability"] }
nidus-testing = "1.0.8"
```

The manifest has its own `[workspace]` table so Cargo treats it as an external
consumer project, not as a normal member of the Nidus repository workspace.

## Environment

- `NIDUS_ADDR`: bind address, default `127.0.0.1:4302`
- `COMMERCE_DATABASE_URL`: SQLite URL, default `sqlite::memory:`

For a file-backed local database:

```bash
COMMERCE_DATABASE_URL='sqlite://commerce.db?mode=rwc' cargo run
```

## Run

```bash
cd examples/external-commerce
cargo run
```

## Test

```bash
cargo test
```

## Curl Walkthrough

```bash
curl -i http://127.0.0.1:4302/health/live
curl -i http://127.0.0.1:4302/health/ready

curl -i http://127.0.0.1:4302/products

curl -i -X POST http://127.0.0.1:4302/carts \
  -H 'content-type: application/json' \
  -d '{"id":"cart-1"}'

curl -i -X POST http://127.0.0.1:4302/carts/cart-1/items \
  -H 'content-type: application/json' \
  -d '{"product_id":1,"quantity":2}'

curl -i -X POST http://127.0.0.1:4302/carts/cart-1/checkout \
  -H 'idempotency-key: checkout-1' \
  -H 'x-request-id: 018f4ad7-56ce-4f6a-a759-29f4438d8d88'

curl -i -X POST http://127.0.0.1:4302/carts/cart-1/checkout \
  -H 'idempotency-key: checkout-1'

curl -i http://127.0.0.1:4302/products
curl -i http://127.0.0.1:4302/metrics
```

Expected responses:

- `GET /products` returns seeded products and then updated inventory after
  checkout.
- `POST /carts` returns `201 Created`.
- `POST /carts/:id/items` returns the cart total.
- First checkout creates an order. Repeating the same `idempotency-key` returns
  the same order ID.
- `/health/live`, `/health/ready`, and `/metrics` are exposed by API defaults
  and observability wiring.

Error checks:

```bash
curl -i -X POST http://127.0.0.1:4302/carts \
  -H 'content-type: application/json' \
  -d '{"id":""}'

curl -i -X POST http://127.0.0.1:4302/carts/missing/items \
  -H 'content-type: application/json' \
  -d '{"product_id":1,"quantity":1}'

curl -i -X POST http://127.0.0.1:4302/carts/missing/checkout
```

Expected statuses are `400 Bad Request`, `404 Not Found`, and `400 Bad Request`.

## Common Imports And Extension Traits

Application entrypoints should start with:

```rust
use nidus::prelude::*;
```

That import keeps the app-composition methods visible:

- `NidusApplicationExt` for Nidus app creation.
- `ApplicationHttpExt` for lower-level bootstrapped application composition.
- `ApiDefaultsObservabilityExt` for `.observability(&observability)` on
  observability-aware API defaults.
