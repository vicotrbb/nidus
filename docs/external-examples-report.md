# External Examples And Extension Trait DX Report

## Summary

This change hardens Nidus 1.0.2 developer experience around extension traits and
adds two copyable external-user examples that use crates.io-style dependencies:

- `examples/external-support-desk`
- `examples/external-commerce`

The examples are standalone Cargo packages with their own `[workspace]` tables
so they are verified separately from the repository workspace and do not use
local path dependencies.

## Files Changed

- `README.md`
- `docs/getting-started.md`
- `docs/deployment.md`
- `docs/observability.md`
- `docs/examples.md`
- `docs/external-examples-report.md`
- `crates/cargo-nidus/src/generate.rs`
- `crates/cargo-nidus/tests/cli_new.rs`
- `examples/external-support-desk/.gitignore`
- `examples/external-support-desk/Cargo.lock`
- `examples/external-support-desk/Cargo.toml`
- `examples/external-support-desk/README.md`
- `examples/external-support-desk/src/main.rs`
- `examples/external-commerce/.gitignore`
- `examples/external-commerce/Cargo.lock`
- `examples/external-commerce/Cargo.toml`
- `examples/external-commerce/README.md`
- `examples/external-commerce/src/main.rs`
- `scripts/verify-external-examples.sh`

## Extension Trait DX Improvements

The docs and generated starter README now recommend:

```rust
use nidus::prelude::*;
```

They also explain the extension traits users most often miss:

- `ApplicationHttpExt` enables `.with_router(...)`.
- `NidusApplicationExt` enables `Nidus::create::<AppModule>()`, `.listen(...)`,
  and `.into_router()`.
- `ApiDefaultsObservabilityExt` enables `.observability(&observability)` and
  observability-aware API defaults.

The generated starter regression in `crates/cargo-nidus/tests/cli_new.rs`
asserts that new projects include the prelude import and generated README
sections for common imports and compile errors.

## Examples Added

### `examples/external-support-desk`

Demonstrates DI/container usage through `TicketStore`, `TicketRepository`,
`TicketService`, and `TicketsController`; ticket creation; comments; priorities;
statuses; assignment; close transition; validation failures; `x-api-key` auth;
request IDs; not-found behavior; `nidus-testing`; and live curl instructions.

### `examples/external-commerce`

Demonstrates `nidus-sqlx` SQLite wiring, `nidus-cache` Moka wiring, products,
carts, inventory, idempotent checkout, env configuration, health/readiness,
metrics, database/cache tests, and live curl instructions.

## Live-Tested Endpoints

`scripts/verify-external-examples.sh` starts both examples on deterministic
localhost ports and curls:

- Support desk: `/health/live`, `POST /tickets`, `POST /tickets/1/assign`,
  `POST /tickets/1/comments`, `POST /tickets/1/close`, `GET /tickets`,
  `POST /tickets` validation failure, and `GET /tickets/404`.
- Commerce: `/health/live`, `/health/ready`, `/products`, `POST /carts`,
  `POST /carts/cart-1/items`, `POST /carts/cart-1/checkout` twice with the same
  idempotency key, `/metrics`, validation failure, not-found behavior, and
  missing idempotency-key behavior.

## Verification Command Results

Fresh results recorded during implementation:

- `git diff --check`: pass.
- `cargo fmt --all --check`: pass.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: pass.
- `cargo test --workspace --all-features`: pass.
- `cargo test -p cargo-nidus --all-features`: pass.
- `cargo test --manifest-path examples/external-support-desk/Cargo.toml`: pass,
  2 tests.
- `cargo test --manifest-path examples/external-commerce/Cargo.toml`: pass,
  2 tests.
- `cargo test -p cargo-nidus cargo_nidus_new_generates_compilable_nidus_project --all-features`:
  pass, generated starter compiles.
- `bash scripts/verify-external-examples.sh`: pass, including formatting,
  starter regression, standalone example tests, live server startup, curl checks,
  and external path dependency scan.
- `NIDUS_EXTERNAL_EXAMPLES_LOCAL_PATCH=1 bash scripts/verify-external-examples.sh`:
  pre-publish proof mode for release candidates whose crates are not on
  crates.io yet. The script copies both examples to a temp directory and adds
  temporary `[patch.crates-io]` entries there only.
- `rg -n "path *=.*nidus|/Users/victorbona/Daedalus/nidus" examples scripts docs README.md crates/cargo-nidus || true`:
  reports existing internal workspace examples and the verification script's
  guard pattern. It does not report the new `examples/external-support-desk` or
  `examples/external-commerce` manifests.
- `rg -n "path *=.*nidus|/Users/victorbona/Daedalus/nidus" examples/external-support-desk examples/external-commerce || true`:
  no output.

## Known Limitations

- The external examples are intentionally standalone and are not normal
  repository workspace members.
- `external-support-desk` uses an in-memory store to keep the example
  copyable without external services.
- `external-commerce` defaults to `sqlite::memory:`; use
  `COMMERCE_DATABASE_URL='sqlite://commerce.db?mode=rwc'` for a file-backed
  local database.

## Completion Statement

Implementation is complete. The required docs, generated starter DX hardening,
external examples, verification script, live curl proof, path-dependency checks,
and strict local gates have all been run with passing results. The only
path-dependency matches left by the broad scan are pre-existing internal
workspace examples or documentation/script guard text, not the new external
examples.
