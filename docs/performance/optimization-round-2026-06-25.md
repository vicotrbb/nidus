# Optimization Round Baseline - 2026-06-25

This document records the baseline for the evidence-driven Nidus optimization
round started on 2026-06-25. It is a local measurement report, not a universal
performance claim.

## Environment

- Machine: `Victors-MBP.localdomain`
- OS: `macOS 14.5 (23F79)`
- Kernel: `Darwin 23.5.0 arm64`
- Rust: `rustc 1.96.0 (ac68faa20 2026-05-25)`
- Cargo: `cargo 1.96.0 (30a34c682 2026-05-25)`
- Git state before changes: `## main...origin/main`
- Recent HEAD: `d416c40 docs: improve framework API rustdocs`
- CI/config caveat: no `.github` directory is present in this checkout.

## Workspace Inventory

Workspace crates and examples are declared in the root `Cargo.toml`:

- Root crate: `nidus-workspace`
- Framework crates: `nidus`, `nidus-core`, `nidus-http`, `nidus-macros`,
  `nidus-config`, `nidus-openapi`, `nidus-validation`, `nidus-auth`,
  `nidus-events`, `nidus-jobs`, `nidus-testing`, `cargo-nidus`
- Examples: `hello-world`, `rest-api`, `auth-api`, `sqlx-postgres`, `openapi`,
  `background-jobs`, `modular-monolith`, `realworld-api`, `production-api`

Benchmark targets present:

- `cargo bench --bench dependency_resolution`
- `cargo bench --bench routing`
- `cargo bench --bench request_lifecycle`

## Baseline Commands

| Command | Result | Notes |
| --- | --- | --- |
| `git status --short --branch` | PASS | Reported `## main...origin/main`. |
| `cargo fmt --all --check` | PASS | No formatting drift. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS | Completed in `35.68s`. |
| `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps` | PASS | Generated workspace docs without rustdoc warnings. |
| `cargo test --workspace --all-features` | PASS | Full workspace tests, examples, trybuild tests, and doc tests passed. |
| `cargo bench --bench dependency_resolution` | PASS | Criterion completed. |
| `cargo bench --bench routing` | PASS | Criterion completed. |
| `cargo bench --bench request_lifecycle` | PASS | Criterion completed. |
| `cargo deny check` | PASS | `advisories ok, bans ok, licenses ok, sources ok`. |
| `cargo audit` | PASS with warning | Exited successfully; reported allowed unmaintained warning `RUSTSEC-2026-0173` for `proc-macro-error2 2.0.1`. |
| `cargo machete` | FAIL | Reported unused dependencies listed below. |

Test caveats:

- CLI integration tests create temporary projects under `/private/var/...` and
  may fetch/check dependencies from crates.io during a cold run.
- `cargo test --workspace --all-features` covers the listed workspace examples
  through their unit/integration tests, including `hello-world`, `rest-api`,
  `auth-api`, `openapi`, `background-jobs`, `modular-monolith`,
  `realworld-api`, and `production-api`.

Optional tool caveats:

- `cargo audit` refreshed the RustSec advisory database and crates.io index.
- `cargo machete` exited with code `1`; these are baseline findings, not fixed
  in this baseline commit:
  - `crates/nidus-openapi/Cargo.toml`: `serde`
  - `crates/nidus-events/Cargo.toml`: `tokio`
  - `examples/hello-world/Cargo.toml`: `axum`
  - `examples/production-api/Cargo.toml`: `serde_json`
  - `examples/realworld-api/Cargo.toml`: `nidus-openapi`, `tower-http`,
    `tracing-subscriber`
  - `examples/rest-api/Cargo.toml`: `axum`

## Benchmark Baseline

Criterion used the default warm-up and measurement settings. `Gnuplot` was not
available, so Criterion used the plotters backend.

| Benchmark | Estimate | Notes |
| --- | ---: | --- |
| `nidus singleton dependency resolution` | `23.922 ns` | 13 high outliers reported. |
| `raw axum route composition` | `1.7736 us` | 1 high severe outlier reported. |
| `nidus controller route composition` | `5.5902 us` | 1 high mild outlier reported. |
| `raw axum baseline request` | `640.55 ns` | 5 high outliers reported. |
| `nidus hello-world request` | `603.69 ns` | 1 high mild outlier reported. |
| `nidus hello-world app` | `2.8308 us` | 8 high outliers reported. |
| `nidus controller + service request` | `735.04 ns` | 9 high outliers reported. |
| `nidus controller + service app` | `3.6480 us` | 6 high outliers reported. |
| `nidus controller setup` | `268.43 ns` | 10 mixed outliers reported. |
| `nidus guarded route` | `917.87 ns` | 9 mixed outliers reported. |
| `nidus validation route` | `1.8485 us` | 1 high severe outlier reported. |
| `nidus request-scoped route` | `1.1714 us` | 1 high mild outlier reported. |

## Initial Improvement Candidates

These candidates come from the requested scope and baseline evidence. They still
need focused tests and code inspection before implementation claims.

1. Dependency injection concurrency: existing tests cover singleton reuse and
   poisoned-cache recovery, but the baseline does not yet prove concurrent first
   resolution executes provider factories at most once.
2. Request-scoped dependency caching: existing tests cover reuse within a
   request scope, but the baseline does not yet prove concurrent request-scope
   first resolution behavior.
3. Metrics storage and rendering: existing tests cover recording and exclusions,
   but high-volume memory-growth behavior and bounded duration storage still
   need inspection.
4. Error envelope body buffering: existing tests cover envelope shape and
   request IDs, but oversized error body behavior needs explicit proof.
5. `cargo machete` findings: unused dependency cleanup is a low-risk developer
   experience/maintenance target if each dependency is confirmed truly unused.

## Next Round Target

The first implementation round should target dependency injection concurrency
because it is central to framework correctness and has a clear missing proof:
write concurrent first-resolution tests, verify the current behavior, then only
change provider caching if the test exposes duplicate factory execution or
another correctness issue.
