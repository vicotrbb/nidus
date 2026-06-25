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

## Round 1 - Dependency Injection Concurrent First Resolution

Hypothesis:

- Singleton and request-scoped provider caches may run factories more than once
  when multiple threads resolve the same provider for the first time.
- A small per-provider/per-scope initialization state should preserve factory
  error behavior while making contenders wait for the first initializer.

Tests added before implementation:

- `singleton_factory_runs_once_under_concurrent_first_resolution`
- `request_factory_runs_once_under_concurrent_first_resolution_in_scope`

Red evidence:

- `cargo test -p nidus-core concurrent_first_resolution` failed as expected for
  the singleton test: the factory ran `8` times instead of `1`.
- `cargo test -p nidus-core --test request_scope_di request_factory_runs_once_under_concurrent_first_resolution_in_scope`
  failed as expected because concurrent request-scope resolutions received
  different instances.

Implementation:

- `ProviderEntry` now tracks singleton cache state as `Empty`, `Initializing`,
  or `Ready` and uses a condition variable so waiters observe the first
  initializer's result.
- `RequestScope` now tracks request instance state as `Initializing` or `Ready`
  and uses a condition variable so concurrent contenders within the same request
  scope share the first initialized instance.
- Failed factories reset the cache state and notify waiters, preserving retry
  behavior and existing provider error wrapping.

Focused verification:

| Command | Result | Notes |
| --- | --- | --- |
| `cargo test -p nidus-core concurrent_first_resolution` | PASS | Both new concurrency tests passed after implementation. |
| `cargo test -p nidus-core` | PASS | All `nidus-core` unit, integration, and doc tests passed. |
| `cargo fmt --all --check` | PASS | Passed after applying `cargo fmt --all`. |
| `cargo clippy -p nidus-core --all-targets --all-features -- -D warnings` | PASS | Focused clippy gate passed. |

Relevant benchmarks after the change:

| Benchmark | Baseline | After Round 1 | Criterion comparison |
| --- | ---: | ---: | --- |
| `nidus singleton dependency resolution` | `23.922 ns` | `27.867 ns` | No statistically significant change detected in Criterion's stored comparison. |
| `raw axum baseline request` | `640.55 ns` | `763.19 ns` | Regressed in this run; not caused by Nidus code path, evidence of noisy environment. |
| `nidus hello-world request` | `603.69 ns` | `607.78 ns` | No change detected. |
| `nidus hello-world app` | `2.8308 us` | `2.8780 us` | No change detected. |
| `nidus controller + service request` | `735.04 ns` | `718.32 ns` | No change detected. |
| `nidus controller + service app` | `3.6480 us` | `3.5968 us` | Improved in Criterion comparison. |
| `nidus controller setup` | `268.43 ns` | `274.22 ns` | No change detected. |
| `nidus guarded route` | `917.87 ns` | `906.44 ns` | Within noise threshold. |
| `nidus validation route` | `1.8485 us` | `1.8509 us` | No change detected. |
| `nidus request-scoped route` | `1.1714 us` | `1.2299 us` | Regressed by about `5.16%` in Criterion comparison. |

Tradeoff:

- The change improves DI correctness under concurrent first resolution.
- The request-scoped route benchmark shows a measured regression for the
  request-scoped initialization path. This is an accepted correctness tradeoff
  for this round, but it should stay visible as a future optimization target.

## Round 2 - Unused Dependency Cleanup

Hypothesis:

- The baseline `cargo machete` findings are real unused direct dependencies.
- Removing confirmed-unused manifest entries should improve dependency hygiene
  without changing public behavior or benchmark-relevant code paths.

Inspection:

- Source search confirmed these direct dependencies were not referenced by their
  owning crates:
  - `crates/nidus-openapi/Cargo.toml`: `serde`
  - `crates/nidus-events/Cargo.toml`: `tokio`
  - `crates/nidus-core/Cargo.toml`: `trybuild`
  - `examples/hello-world/Cargo.toml`: `axum`
  - `examples/production-api/Cargo.toml`: `serde_json`
  - `examples/realworld-api/Cargo.toml`: `nidus-openapi`, `tower-http`,
    `tracing-subscriber`
  - `examples/rest-api/Cargo.toml`: `axum`
- Similar crates that are genuinely used remain declared where needed. For
  example, `examples/production-api` still uses `axum`, `examples/realworld-api`
  still uses `axum`, `serde`, `serde_json`, and `tracing`, and `nidus-core`
  tests still use `tracing-subscriber`.

Implementation:

- Removed only the confirmed-unused direct dependency entries from crate and
  example manifests.
- Let Cargo update `Cargo.lock` through normal verification.

Verification:

| Command | Result | Notes |
| --- | --- | --- |
| `cargo machete` | PASS | No unused dependencies found after cleanup. |
| `git diff --check` | PASS | No whitespace errors. |
| `cargo fmt --all --check` | PASS | No formatting drift. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS | Full workspace clippy passed after cleanup. |
| `cargo test --workspace --all-features` | PASS | Full workspace tests, examples, trybuild tests, and doc tests passed. |
| `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps` | PASS | Workspace docs built without warnings. |
| `cargo deny check` | PASS | `advisories ok, bans ok, licenses ok, sources ok`. |
| `cargo audit` | PASS with warning | Same allowed `RUSTSEC-2026-0173` unmaintained `proc-macro-error2 2.0.1` warning. |

Caveat:

- The first post-cleanup verification attempt failed with `No space left on
  device` while Cargo was creating a target fingerprint. The repo-local
  `target/` directory was `9.8 GiB`; `cargo clean` removed rebuildable artifacts
  (`11.8 GiB`) and verification was rerun successfully.

## Round 3 - Bounded Metrics Duration Histograms

Hypothesis:

- `PrometheusMetrics` stores every observed duration in a per-label `Vec<f64>`,
  which grows without bound in long-lived processes.
- Fixed cumulative Prometheus histogram buckets can preserve useful duration
  telemetry while bounding memory per label set.

Test added before implementation:

- `prometheus_metrics_renders_bounded_duration_histogram_buckets`

Red evidence:

- `cargo test -p nidus-http --test production_api prometheus_metrics_renders_bounded_duration_histogram_buckets`
  failed as expected because the collector rendered only `_count` and `_sum`
  series and did not expose any `_bucket` series.

Implementation:

- Replaced raw duration sample vectors with `DurationHistogram` values storing:
  - total count
  - total sum
  - fixed cumulative bucket counts for `0.005`, `0.01`, `0.025`, `0.05`,
    `0.1`, `0.25`, `0.5`, `1`, `2.5`, `5`, `10`, and `+Inf`
- Kept existing metric names for request totals, in-flight counts, errors,
  duration `_count`, and duration `_sum`.
- Updated the in-memory collector docs to describe bounded duration histograms
  instead of retained duration samples.
- Added metrics recording and rendering coverage to `benches/request_lifecycle.rs`.

Focused verification:

| Command | Result | Notes |
| --- | --- | --- |
| `cargo test -p nidus-http --test production_api prometheus_metrics_renders_bounded_duration_histogram_buckets` | PASS | New bounded histogram regression test passed. |
| `cargo test -p nidus-http` | PASS | All `nidus-http` tests and doc tests passed. |
| `cargo fmt --all --check` | PASS | No formatting drift. |
| `cargo clippy -p nidus-http --all-targets --all-features -- -D warnings` | PASS | Focused clippy gate passed. |
| `cargo clippy --bench request_lifecycle --all-features -- -D warnings` | PASS | New benchmark code passed clippy. |
| `cargo bench --bench request_lifecycle` | PASS | Relevant request lifecycle and metrics benchmarks completed. |

Relevant benchmarks after the change:

| Benchmark | Estimate | Notes |
| --- | ---: | --- |
| `raw axum baseline request` | `631.27 ns` | Close to original baseline. |
| `nidus hello-world request` | `601.98 ns` | Close to original baseline. |
| `nidus controller + service request` | `712.30 ns` | Close to original baseline. |
| `nidus guarded route` | `913.66 ns` | Close to original baseline. |
| `nidus validation route` | `1.8586 us` | Close to original baseline. |
| `nidus request-scoped route` | `1.2211 us` | Still reflects Round 1 correctness overhead. |
| `nidus metrics record response` | `268.28 ns` | New benchmark; no prior baseline. |
| `nidus metrics render text` | `50.262 us` | New benchmark with 10 route labels and 100 observations per label. |

Tradeoff:

- Metrics duration memory is now bounded per method/route/status label set.
- Rendering emits bucket series in addition to count/sum, increasing metrics
  text size by design.
