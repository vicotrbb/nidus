# Release 1.0.3

Nidus 1.0.3 is a production-DX patch release focused on app composition,
production error behavior, guard ergonomics, observed jobs/events, and the
generated starter.

## Highlights

- `Nidus::create::<AppModule>().with_router(router).build().await` and
  `build_with_router(router)` attach manual Axum routers through the facade
  builder without requiring application code to import `ApplicationHttpExt`.
- `ApiDefaults::production(...).apply(router)` installs the Nidus
  `not_found_fallback` by default, so unmatched routes return the production
  JSON error envelope with `statusCode: 404`, `code: "not_found"`, request ID,
  path, timestamp, and default security headers.
- `GuardContext` now includes `header_str`, `bearer_token`, and `api_key`
  helpers for common explicit auth guards.
- `nidus-jobs` includes `job_observer_channel`, `JobObserverChannel`, and
  `ObservedJobEvent` for off-thread job telemetry export.
- `EventBus::observed(...)`, `Observability::observed_event_bus::<T>()`, and
  `Observability::job_runner()` reduce observer boilerplate while keeping the
  existing explicit primitives available.
- `cargo nidus new` now generates a `src/lib.rs` and `src/main.rs` split,
  first-party `TestApp` HTTP tests, health/readiness coverage through
  `ApiDefaults`, and no explicit metrics disabling in the starter.

## Deferred

SQLx migrations and schema setup remain application-owned. This release keeps
`nidus-sqlx` focused on clean pool registration, health, and observability
integration.

## Verification

Before publishing, run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --all-features --no-deps
cargo test -p cargo-nidus --test cli_new -- --nocapture
bash scripts/package-publishable-crates.sh --list-only
```

After publishing, verify crates.io visibility and regenerate a starter from the
published `cargo-nidus` package before creating the `v1.0.3` tag.
