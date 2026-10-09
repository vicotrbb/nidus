# Release 1.2.0

Version 1.2.0 coordinates all 25 framework crates, including `nidus-rs`,
`cargo-nidus`, and the optional integrations. It includes the complete remote
main history through the compatible simplification change in PR #41.

## Shared route implementation

HTTP routing, runtime OpenAPI generation, and CLI source inspection share
normalization, parameter extraction, and operation identifier helpers in
`nidus-core`. HTTP and CLI normalization trims surrounding whitespace and
ensures a leading slash. OpenAPI conversion preserves separators and whitespace.
Existing route errors and output formats remain unchanged.

SQLite and Postgres use one adapter telemetry helper. `CacheConfig` derives
`Default` with the same values as before.

## Compatibility

The minor release retains `Provider`, `Pipe<Input>`, their facade prelude
re-exports, the HTTP response module, and the concrete middleware service and
future types. Applications require no API migration for these simplifications.
The Rust 1.96 minimum and edition 2024 remain unchanged.

## Dependency update

The lockfile updates rustls to 0.23.45 for RUSTSEC-2026-0285, together with the
required aws-lc and rustls-webpki updates. Publication validates the locked
cohort and resolves clean public-registry consumers after uploading all crates.

## Install

```toml
nidus = { package = "nidus-rs", version = "1.2.0", features = ["http"] }
```

Install the matching CLI with `cargo install cargo-nidus --version 1.2.0` and
select integration crates at version 1.2.0 as needed.
