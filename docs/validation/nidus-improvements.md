# Nidus improvements 1–3: implementation and validation

## Scope and resulting interfaces

Baseline: clean `main`, `99642ed0018c6da5c921db92d966f3b52a7e7d77`, version 1.0.17. Changes remain uncommitted; no push, merge, tag, publication, deployment, or production access occurred.

HTTP metrics now own a request-accounting guard from `Service::call` until response, service error, or cancellation. Dropping before polling, during polling, or through an outer timeout releases in-flight accounting exactly once. Cancellation adds `nidus_http_cancelled_requests_total{method,route}` without inventing a response status or histogram sample. Response latency still ends when the response becomes available. Custom hooks remain source compatible through a default `on_cancel`; hooks owning accounting must implement it and must not panic.

`ApplicationPlan` centralizes graph ordering, provider registration, declared async resource initialization, overrides, and cleanup ownership. Shared HTTP composition constructs module controllers and installs the same application container and necessary request scopes. Facade composition preserves configured middleware, OpenAPI, observability, and manual routes. `TestApp::from_application` and `from_managed` transfer the composed application instead of discarding its container. Module tests can use the same plan directly without introducing a facade/testing dependency cycle.

The additive managed supervisor owns startup, HTTP connections/in-memory test requests, workers, draining, cleanup, and a shared shutdown report. It makes `Built → Starting → Running → Draining → Stopping → Stopped` observable. Successfully initialized resources roll back in reverse order after subsequent initializer, router, startup hook, or bind failure. Managed shutdown executes once, continues after individual errors, preserves original error sources, and joins owned async work before releasing dependencies.

```rust,ignore
use nidus::prelude::*;
use nidus::lifecycle::managed::ManagedOptions;

let managed = Nidus::create::<AppModule>()
    .override_provider(test_database)?
    .with_router(manual_routes)
    .build_managed(ManagedOptions::default())
    .await.map_err(|report| format!("startup failed: {report:?}"))?;
let app = nidus_testing::TestApp::from_managed(managed);
app.get("/users").send().await.assert_status(StatusCode::OK);
assert!(app.shutdown_report().await.unwrap().is_success());
```

For production, use `.start_managed(socket_address, options)` and explicitly await `shutdown()`. Worker-only applications use `.start_managed_worker(options)` without HTTP. Implement `Resource`, declare it with `ModuleBuilder::resource::<R>()`, and register workers through `ManagedOptions::worker`. See [the complete contract](../managed-applications.md) and [the executable hello-world example](../../examples/hello-world/src/main.rs).

## Compatibility and limits

- Existing low-level `LifecycleRunner` and application shutdown calls remain repeatable; exactly-once semantics belong to the managed owner. Low-level Axum serving still does not implicitly run application cleanup.
- Existing metrics names, response/error labels, middleware ordering, and response-availability latency remain. Tower readiness delegates to the inner service. Built-in accounting recovers on ordinary inner-service unwind; custom hook panics, process abort, and panicking destructors have no cleanup guarantee.
- Overrides are explicit and applied before registration/initialization/controller construction. A replaced resource remains externally owned and its original initializer/cleanup never run. `async_initializer_for<T>` annotates a legacy initializer's sole output; opaque initializers/factories cannot be inferred or selectively replaced.
- Graph and declared route conflicts are checked before resources start. Opaque routers/factories are checked during assembly; subsequent failures roll back initialized resources. Resources must clean up their own partial initialization failure.
- Module `TestApp::build()` now composes synchronous providers/controllers and rejects async modules with a documented panic; `try_build()` reports the error and `build_started()` supports async composition. Prefer `build_managed()` for caller-cancellation safety. Router-only helpers retain their low-level behavior.
- Module-test `.config(config)` overrides the concrete `nidus_config::Config` provider before initialization. Other typed configuration requires an explicit override. Harness `config()` is not an eager snapshot; use `resolve::<Config>()` for the application provider. Router-only builders retain their separate harness configuration.
- Request scopes are installed only when required, unless explicitly requested. HTTP dependencies remain in the HTTP crate; no unsafe code was introduced. The only dependency version update is the precise `h2 0.4.15 → 0.4.16` security patch; unrelated lockfile resolution was preserved.
- Default drain/cleanup budgets are 30/10 seconds. Cleanup divides remaining time among remaining hooks; timed-out futures are dropped and later hooks still run. Composition rollback separately has a 10-second budget and reports timeouts in nested rollback errors, rather than the supervisor's `deadline_expired` flag.
- Startup caller cancellation transfers cleanup to the supervisor after composition finishes. Initializers must bound their own waits. Blocking/non-yielding code cannot be forcibly terminated; raw detached tasks, upgrades, and adapter workers require explicit ownership. Drop requests shutdown but cannot await it; keep Tokio alive and await shutdown for join evidence.
- Managed HTTP currently supervises HTTP/1 connections. It gates an existing `/health/ready` route and preserves dependency health checks; it does not create a health endpoint.
- Telemetry declared before dependent resources flushes after their cleanup. Ordinary lifecycle hooks start after resources and close before resources in reverse hook order. Avoid double-registering cleanup.

Validation exposed two additional concrete defects, fixed with the main work: the OTLP HTTP exporter lacked Tokio context on the SDK batch thread (covered by an actual loopback POST/flush/shutdown regression), and the Cockroach example decoded `SELECT 1` as `i32` instead of its `INT8`/`i64` type. Live validation now refuses occupied ports instead of killing unrelated processes, builds all binaries from the explicit workspace manifest/target, and verifies nonempty OTLP/Sentry receiver payloads.

The first complete pass also detected locked `h2 0.4.15` via `cargo audit` ([RUSTSEC-2026-0258](https://rustsec.org/advisories/RUSTSEC-2026-0258)). The patch to `0.4.16` changes no dependency declarations or minimum Rust version. No audit exceptions or policy rules were weakened. The entire suite was restarted after this lockfile change; the earlier pass remains under `target/nidus-validation/before-h2-update`. That rerun exposed a MySQL startup race: a socket query could succeed against the image's temporary `--skip-networking` initialization server before the TCP server was ready. The probe now requires an authenticated TCP query. The service suite passed with full cleanup after this correction, independent review found no issue, and the entire acceptance pass was restarted again. The interrupted run is preserved under `target/nidus-validation/mysql-readiness-race`; its incomplete benchmarks and unreached checks are not counted as final passes.

## Acceptance matrix

The test files below are new regression coverage, in addition to the existing workspace suite. Final execution outcomes are recorded separately below.

| Requirement | Concrete evidence |
| --- | --- |
| Success/error, drop before/after poll, completed drop | `crates/nidus-http/tests/metrics_cancellation.rs`: `cancellation_before_and_after_poll_releases_exactly_once`, `completed_futures_do_not_cancel_or_release_twice` |
| Timeout/middleware order, concurrency, exclusion | Same file: `outer_timeout_cancels_inner_metrics_and_inner_timeout_records_service_error`, `concurrent_request_cancellation_releases_all_accounting`, `excluded_requests_never_create_accounting` |
| Hooks, readiness, unwind | Same file: `custom_hooks_receive_cancellation_and_readiness_is_delegated`, `unwinding_inner_poll_releases_built_in_accounting`, `unwinding_inner_call_releases_accounting` |
| Production/test controllers, resource replacement, shared instances/scopes, manual routes | `crates/nidus/tests/managed_composition.rs`: `module_test_and_production_share_overrides_scopes_resources_and_manual_routes` |
| Config and explicit scope semantics | Same file: `configuration_override_replaces_declared_initializer_before_it_runs`, `explicit_scope_opt_in_survives_a_singleton_override` |
| Early metadata validation | Same file: `structural_route_conflicts_fail_before_resource_initialization`; `crates/nidus-core/tests/application_plan.rs`: `registrar_resource_collision_fails_before_any_resource_initializer`, `opaque_registration_is_detected_before_constructing_a_duplicate_resource` |
| Dependency order and rollback source preservation | `application_plan.rs`: `rollback_retains_original_error_and_every_cleanup_failure_in_reverse_order`; `managed_composition.rs`: `initializer_failure_rolls_back_imported_resources_in_production_and_tests` |
| Core async composition retains ownership | `application_plan.rs`: `core_async_bootstrap_retains_declared_resource_ownership` |
| Router and bind failures after initialization | `managed_composition.rs`: `opaque_router_failure_after_initialization_rolls_back`, `listener_bind_failure_closes_initialized_resources` |
| Full real-module/resource/scope/metrics/worker scenario | `managed_composition.rs`: `end_to_end_managed_modules_cancel_metrics_drain_workers_and_close_once` includes real TCP draining, readiness withdrawal, exact cleanup counts and listener rebind |
| HTTP drain/deadline and in-memory ownership | Same file: `managed_http_drains_inflight_and_releases_listener`, `managed_deadline_drops_hanging_http_requests_before_resource_cleanup`, `module_managed_test_waits_for_inmemory_http_before_cleanup` |
| Concurrent shutdown, worker join, reverse cleanup | `crates/nidus-core/tests/managed_lifecycle.rs`: `concurrent_shutdown_cancels_and_joins_worker_before_reverse_cleanup` |
| Startup/cleanup errors and panic | Same file: `failed_start_rolls_back_successes_and_preserves_cleanup_errors`, `panicking_worker_factory_is_joined_and_resources_close`, `panicking_cleanup_does_not_skip_remaining_hooks` |
| Cancelled startup/shutdown waiters | Same file: `cancelled_startup_caller_still_rolls_back_owned_application`, `cancelled_shutdown_waiter_leaves_cleanup_running` |
| Hanging hooks, worker abort, telemetry last | Same file: paused-time `hanging_hook_does_not_prevent_later_cleanup_or_telemetry_flush`, `noncooperative_async_worker_is_aborted_joined_and_dropped_before_cleanup` |
| Actual telemetry transport and explicit flushing | `crates/nidus-opentelemetry/tests/live_http.rs`: `real_http_export_flushes_a_nonempty_batch_on_the_sdk_thread`; live OTLP and Sentry binaries plus loopback receiver |

## Example inventory and evidence

Inventory from repository manifests and Rust binary targets: **15 application packages**, including **2 standalone packages**, plus **11 additional integration binaries** = **26 executable examples**. Workspace tests compile/test all workspace targets. Standalone local-patch checks run each consumer's own formatting, tests, build and HTTP workflows against the unpublished implementation. No verification against published dependencies is claimed for those patched runs.

`W` = workspace test/build; `L` = expanded live-example script; `E` = standalone local-patch script; `S` = disposable service script with `NIDUS_VALIDATE_EXAMPLES=1`. Final command statuses below determine whether each evidence group completed.

| Example | Evidence and representative behavior | Services and cleanup contract |
| --- | --- | --- |
| auth-api | W, L: missing key 401, valid key authorized | Loopback process stopped/joined by script |
| background-jobs | W, L: worker demonstration runs to completion | In-memory; normal process exit |
| cache-app | W, L: cache demonstration | In-memory; normal process exit |
| dashboard-api | W, L: account route and dashboard graph | In-memory SQLite; owned process stopped/joined |
| hello-world | W, L: module-owned root and managed harness test | Managed SIGTERM cleanup completion asserted |
| integrations-production (main) | W, L: configured SQLite/cache composition | In-memory SQLite/cache; normal exit |
| launchpad-api | W, L: auth/validation/create/read/update/workflow/body-limit | In-memory; owned process stopped/joined |
| modular-monolith | W, L: module composition demonstration | In-memory; normal exit |
| openapi | W, L: document and documentation routes | Loopback process stopped/joined |
| production-api | W, L: live/ready/metrics, timeout, domain error | Loopback process stopped/joined |
| realworld-api | W, L: users/projects/tasks/auth/validation/context/workflow | Local SQLite; loopback process stopped/joined |
| rest-api | W, L: typed user route | Loopback process stopped/joined |
| sqlx-app | W, L: SQLite queries | Local in-memory SQLite; normal exit |
| external-support-desk | E: ticket create/assign/comment/close and error paths | Standalone temporary patched copy; stopped/joined; temp removed |
| external-commerce | E: products/cart/idempotent checkout/inventory/metrics | Standalone temporary patched copy; SQLite/cache; stopped/joined; temp removed |
| integrations-production/cockroach | W, S: SQL roundtrip and example | Disposable TLS Cockroach; tested pool closure and owned service cleanup |
| integrations-production/mysql | W, S: SQL roundtrip and example | Disposable MySQL; tested pool closure and owned service cleanup |
| integrations-production/redis | W, S: Redis roundtrip, TTL and example | Disposable Redis; key cleanup and service removal |
| integrations-production/kafka | W, S: admin/delivery/consume/commit and example | Disposable Kafka; owned topic/service cleanup |
| integrations-production/nats | W, S: persistent stream/durable consumer/ack and example | Disposable NATS JetStream; stream/service cleanup |
| integrations-production/rabbitmq | W, S: confirm/consume/ack and example | Disposable RabbitMQ; queue and labeled volume/service cleanup |
| integrations-production/sqs | W, S: DLQ/send/receive/delete and example | Disposable LocalStack; dummy credentials; owned queues/service cleanup |
| integrations-production/durable-jobs | W, L: durable job execution | Local SQLite; normal exit |
| integrations-production/envelope | W, L: integration envelope demonstration | In-memory; normal exit |
| integrations-production/opentelemetry | W, L: nonempty `/v1/traces` POST, explicit flush/shutdown | Owned loopback receiver stopped/joined |
| integrations-production/sentry | W, L: nonempty `/api/1/envelope/` POST, explicit flush/shutdown | Same owned loopback receiver |

The generated CLI consumer is additional evidence: scaffold, compile, launch, representative root response, then owned process/temp cleanup. Existing low-level HTTP examples prove process termination and representative behavior; only managed examples/tests claim application-hook shutdown. They are not presented as automatically gaining managed semantics.

## Independent review

Two independent reviewers checked the entire changed/untracked implementation against fixed baseline, repository standards, and the requested acceptance criteria. Follow-up reviews covered fixes for composition ownership, eager configuration factory behavior, worker factory panic ownership, deadline documentation, example invocation safety, and the actual OTLP/Cockroach fixes. The final targeted review reported no remaining findings and `git diff --check` passed. Review is separate from command validation.

## Final validation

All functional, feature-isolation, documentation, example, benchmark, dependency-policy, security, and applicable library semver checks passed on the finished implementation. Package-list preflight passed; full registry-backed tarball verification remains blocked at the unpublished internal API boundary described below. It is not marked passed.

Execution used Rust/Cargo 1.96.0 on `aarch64-apple-darwin`, with inherited `CARGO_NET_OFFLINE=true` for nested Cargo invocations. Advisory tools fetched the RustSec database. Local service validation used OrbStack's local Unix socket and isolated disposable containers; no production/shared service was used. Service cleanup verified zero run-owned containers, networks, volumes or temporary paths, released ports, unchanged global volume inventory, and unchanged worktree status.

The frozen source fingerprint includes tracked and nonignored untracked files, excluding only this results report and the acceptance checklist:

`62e63fd00ebc5a632b2160b7d6a7e63a7f8da0820c2b863f2da70fb297f73853` (SHA-256; 1249 files).

The fingerprint was recomputed after the entire final pass and matched. No implementation, manifest, lockfile, example, guide, or validation-script edits followed this pass. Only the results report and checklist were completed afterward.

Raw command logs, exact outcomes/timings, the reproducible runner, fingerprints and Criterion estimates are retained under `target/nidus-validation/final/`. Earlier failed/interrupted attempts have separate directories and are not substituted for final results.

| Command | Final result | Wall time (s) |
| --- | --- | --- |
| `cargo fmt --all --check` | PASS (exit 0) | 0 |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS (exit 0) | 1 |
| `cargo test --locked --workspace --all-features` | PASS (exit 0) | 47 |
| `bash scripts/check-integration-feature-matrix.sh` | PASS (exit 0) | 8 |
| `RUSTDOCFLAGS=-Dwarnings cargo doc --locked --workspace --all-features --no-deps` | PASS (exit 0) | 2 |
| `bash scripts/verify-live-examples.sh` | PASS (exit 0) | 24 |
| `NIDUS_EXTERNAL_EXAMPLES_LOCAL_PATCH=1 bash scripts/verify-external-examples.sh` | PASS (exit 0) | 68 |
| `NIDUS_VALIDATE_EXAMPLES=1 bash scripts/test-integration-services.sh` | PASS (exit 0) | 44 |
| `cargo bench --locked -p nidus-workspace --all-features --bench request_lifecycle --bench routing -- --noplot` | PASS (exit 0) | 420 |
| `cargo deny check` | PASS (exit 0) | 1 |
| `bash scripts/audit-dependencies.sh` | PASS (exit 0) | 2 |
| `bash scripts/semver-check-publishable-crates.sh --baseline-rev 99642ed0018c6da5c921db92d966f3b52a7e7d77 --release-type minor` | PASS (exit 0) | 56 |
| `bash scripts/package-publishable-crates.sh --list-only --locked` | PASS (exit 0) | 3 |
| `bash scripts/package-publishable-crates.sh --allow-dirty --locked` | BLOCKED: published core lacks new APIs (exit 101) | 4 |

Workspace harnesses reported **638 passed, zero failed, 23 ignored**. The ignored cases are nine opt-in service tests (all nine subsequently executed and passed by the service script) and 14 existing illustrative macro doctests (still ignored, not represented as passing). Macro UI coverage separately includes 12 compile-pass and 23 compile-fail fixtures, all successful. The five new regression-test files contain 32 focused tests across application plans, managed lifecycle/composition, cancellation accounting and actual OTLP transport.

A final normal/build dependency-tree assertion also confirmed that worker-only `nidus-rs --no-default-features` excludes `axum`, `hyper` and `tower-http`.

All `W`, `L`, `E`, and `S` evidence groups in the example matrix completed successfully on this final fingerprint. Standalone consumer tests/builds use temporary local patches; they do not establish compatibility with already published dependencies. Legacy examples establish representative behavior and owned-process termination; managed cleanup is asserted only where explicitly stated.

Semver checked **23 libraries** against the exact baseline with the intended additive/minor release classification and found no breaking changes. `nidus-macros` (procedural macro) and `cargo-nidus` (binary) have no supported library target for this tool and were explicitly skipped; macro UI tests and generated-consumer tests provide separate evidence. No Nidus version was bumped. Rule-level inapplicable skips remain visible in the raw semver log.

Security passed after the precise `h2` patch. The repository's existing, narrowly checked `RUSTSEC-2023-0071` exception for SQLx/MySQL's public-key-only RSA usage remains in force; no exception was added or broadened. `cargo deny check` uses the repository's existing default graph/policy, while `cargo audit` scans the complete workspace lockfile under that existing exception. This is evidence under the documented policy, not a claim that all dependencies are free of every possible vulnerability.

### Packaging boundary

File-list preflight passed for all **25 publishable crates**. Full tarball verification passed for `nidus-core`, `nidus-macros`, `nidus-config`, `nidus-auth`, `nidus-events`, `nidus-jobs`, and `nidus-validation`, then stopped while compiling the packaged `nidus-http`. Its registry-resolved `nidus-core 1.0.17` does not contain the unpublished `ApplicationPlan` and `lifecycle::managed` APIs. The remaining 17 crates were not reached by the fail-fast full-package script.

This is the cross-crate release-order limitation already identified by the packaging script's preflight contract. A coherent version/dependency release sequence is required before these archives can independently resolve the new APIs from the registry. No archive verification bypass, local-registry substitution, version bump or publication was used to make that check appear successful. Locally patched standalone consumers and the generated consumer validate the unpublished workspace code separately.

### Benchmark results and scope

All **44** existing request-lifecycle and routing benchmarks completed with their unchanged 3-second warmup, 100 samples, and approximately 5-second measurement windows. The table reports current-run sample medians with 95% confidence intervals, taken from Criterion's `new/estimates.json`. Criterion emitted cached-run comparisons, including regressions and improvements; the raw log retains them. Those cached runs include earlier attempts and do not form a controlled before/after baseline, so their percentages are not used as performance-change claims.

Request-path cases exercise the existing in-memory Axum/Tower harness, including runtime entry and router cloning; they are not network throughput measurements. The metrics recording cases isolate the hook; the production-with-metrics case includes the configured middleware stack. Router/controller construction is measured separately by the `app`, `setup` and route-composition cases. Full managed/resource startup latency, allocations and compile-time performance were not benchmarked. Validation wall times above include cache effects and are not compile-time performance claims. There is no controlled before/after baseline, speedup claim, or allocation claim.

| Benchmark (median, 95% confidence interval) | Time |
| --- | --- |
| raw axum baseline request | 504.839 ns [497.837, 510.587] |
| nidus hello-world request | 455.686 ns [452.066, 458.277] |
| nidus hello-world app | 1.001 µs [0.992, 1.011] |
| nidus 32-route controller app | 17.413 µs [17.309, 17.649] |
| nidus controller + service request | 560.494 ns [556.323, 568.307] |
| nidus controller + service app | 1.334 µs [1.324, 1.342] |
| nidus controller setup | 181.160 ns [180.171, 184.618] |
| nidus guarded route | 701.980 ns [693.888, 714.391] |
| nidus module-composed guarded route | 580.378 ns [575.928, 585.910] |
| nidus validation route | 1.269 µs [1.256, 1.291] |
| nidus request-scoped route | 834.429 ns [830.870, 839.567] |
| nidus request-scoped extractor route | 895.240 ns [883.417, 911.183] |
| nidus health readiness with 8 checks | 1.603 µs [1.587, 1.613] |
| nidus middleware security headers request | 648.273 ns [643.034, 652.402] |
| nidus middleware body limit request | 608.548 ns [603.443, 614.400] |
| nidus middleware legacy request id request | 1.721 µs [1.712, 1.729] |
| nidus middleware validated request id request | 1.104 µs [1.099, 1.111] |
| nidus middleware request context request | 975.390 ns [970.492, 979.312] |
| nidus middleware error envelope success request | 776.163 ns [766.764, 784.769] |
| nidus middleware catch panic success request | 606.420 ns [602.381, 608.497] |
| nidus middleware timeout response request | 633.465 ns [627.933, 638.004] |
| nidus middleware rate limit request | 904.217 ns [898.168, 910.145] |
| nidus middleware rate limit rejected request | 854.661 ns [849.394, 860.340] |
| nidus middleware rate limit store error request | 910.639 ns [905.436, 915.464] |
| nidus rate limit store check with 10k identities | 39.746 ns [39.590, 39.940] |
| nidus structured logging span creation | 77.155 ns [76.753, 77.395] |
| nidus logging redaction lowercase lookup | 6.229 ns [6.161, 6.290] |
| nidus logging redaction mixed-case lookup | 5.824 ns [5.800, 5.852] |
| nidus request context clone | 3.838 ns [3.829, 3.848] |
| nidus trusted proxy client ip identity | 110.022 ns [108.552, 111.387] |
| nidus trusted proxy identity extractor clone | 3.556 ns [3.546, 3.567] |
| nidus 100-route openapi json request | 569.469 ns [566.356, 574.553] |
| nidus 64-schema openapi document construction | 15.936 µs [15.826, 15.983] |
| nidus 100-route openapi document render | 321.384 µs [316.869, 323.764] |
| nidus 100-route openapi document construction | 40.585 µs [40.089, 41.032] |
| nidus 100-route openapi metadata construction | 34.329 µs [34.120, 34.615] |
| nidus 8-route openapi document construction | 2.804 µs [2.777, 2.826] |
| nidus api defaults production request | 2.466 µs [2.451, 2.484] |
| nidus api defaults production with metrics request | 2.822 µs [2.809, 2.848] |
| nidus metrics record response | 56.791 ns [56.359, 57.359] |
| nidus metrics record inner error | 58.124 ns [57.152, 59.187] |
| nidus metrics render text | 8.528 µs [8.230, 8.854] |
| raw axum route composition | 1.346 µs [1.341, 1.354] |
| nidus controller route composition | 1.627 µs [1.623, 1.634] |

### Remaining operational limits

The managed API cannot forcibly terminate non-yielding/blocking work or recover asynchronous cleanup after runtime/process termination. Initializers must bound their external waits and own partial-failure cleanup. Detached tasks and upgrades need explicit supervision. Cleanup errors/timeouts are reported, so `Stopped` means cleanup was attempted, not that every external system confirmed closure. These contracts are covered in the guide rather than hidden behind successful happy-path checks.

One early discovery run also observed an unattributed change in global Docker volume inventory. Run-labeled cleanup succeeded, but the global invariant failed; unknown volumes were not deleted. Subsequent complete service runs, including this final run, passed the unchanged global inventory check. The failure was investigated and retained as an evidence limitation, not silently suppressed.

No required validation was silently omitted. The unsupported semver targets, existing ignored doctests, unreached full-package checks, packaging release-order limitation, and unmeasured performance categories are explicitly separated above. Work remains uncommitted and unpublished.
