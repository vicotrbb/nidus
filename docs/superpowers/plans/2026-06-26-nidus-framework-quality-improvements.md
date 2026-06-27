# Nidus Framework Quality Improvements — Implementation Plan

- Date: 2026-06-26
- Derived from: `docs/superpowers/audits/2026-06-26-nidus-framework-quality-audit.md`
- Baseline: green (build + `cargo test --workspace --all-features`, ~260 tests, 0 failures)

## Goal

Reduce the highest-severity correctness risk in the framework with small, high-confidence,
TDD-driven changes. Do not change public APIs without a documented benefit. No regressions.

## Phase selection rationale

The audit found **three P1 correctness defects**, all latent (each slipped past existing tests
because no test exercises the failing path). They form the smallest coherent first phase: each is
isolated, each has an obvious failing-test-first path, and none requires a speculative rewrite.
A small set of cheap, test-first P2 hardening items is bundled in a second wave because they are
low-risk and directly convert latent risk into locked-in coverage.

Out of scope for this plan (deferred — see "Deferred items"): body-limit/streaming changes,
graceful-shutdown, rate-limit identity hardening, panic-catching layer, prometheus series bounds,
graph TypeId re-keying, OpenAPI error-response modeling. These are larger or touch public
behavior/defaults and deserve their own scoped phases.

---

## Wave 1 — P1 correctness fixes (TDD)

### 1.1 `guard_layer` must populate request headers and authorize before calling inner (F-HTTP-1)

- **Files:** `crates/nidus-auth/src/middleware.rs`; test `crates/nidus-auth/tests/guards.rs`
- **Behavior change:** internal layer wiring (no public-API signature change).
- **TDD steps:**
  1. Add a failing test `guard_layer_passes_request_headers_to_guard` in `tests/guards.rs`: a guard
     that reads `ctx.headers().get("x-api-key")` and denies when missing/invalid; wire via
     `guard_layer(..)`; assert a request **without** the header → 401 and one **with** it → 200.
  2. Run `cargo test -p nidus-auth guard_layer_passes_request_headers` → **fails** (guard sees an
     empty `HeaderMap`, so the "with header" case is denied).
  3. Implement: in `GuardService::call`, split `request.into_parts()`, build
     `GuardContext::new(state, route_label).with_headers(parts.headers.clone())`, run `guard.check`
     **first**, and only on `Ok` reassemble `Request::from_parts(parts, body)` and call `inner`.
     On `Err`, return `error.into_response()` without calling inner (also resolves A-1 ordering).
  4. Re-run the new test → passes. Re-run the full `nidus-auth` suite → all pass.
- **Verification:** `cargo test -p nidus-auth`; `cargo clippy -p nidus-auth -- -D warnings`.
- **Bench:** not required (auth layer is not a measured hot path in any current bench; no
  `guard_layer`-using scenario exists in the criterion targets).
- **Rollback:** `git revert <commit>`.

### 1.2 Singleton factory panic must not permanently deadlock the provider (F-CORE-1)

- **Files:** `crates/nidus-core/src/provider/mod.rs`; test `crates/nidus-core/tests/core_di.rs`
- **Behavior change:** panic during singleton construction becomes a recoverable error instead of a
  permanent hang. (Public API unchanged; `NidusError` may gain/extend a variant.)
- **TDD steps:**
  1. Add a failing test `singleton_factory_recovers_after_panic` in `tests/core_di.rs`: register a
     singleton factory that `panic!`s on first call and succeeds thereafter; spawn a resolve in a
     thread with a timeout; assert the second resolve (after the panic) either returns an error or
     succeeds — **not a hang**.
  2. Run under a timeout → **fails/hangs** (current code deadlocks).
  3. Implement: wrap the create call in `std::panic::catch_unwind(AssertUnwindSafe(|| self.create_erased(container)))`.
     On `Err(panic_payload)`: under the lock reset state to `Empty`, `notify_all`, then
     `std::panic::resume_unwind(panic_payload)` (preserve the original panic). This keeps the
     "panic propagates" contract while leaving the provider re-resolvable.
  4. Re-run the test (with the factory succeeding after one panic) → the panic propagates and the
     provider is re-resolvable afterwards. Confirm no hang.
- **Verification:** `cargo test -p nidus-core`; `cargo clippy -p nidus-core -- -D warnings`.
- **Bench:** run `cargo bench --bench dependency_resolution` before/after — the change is on the
  singleton resolution hot path; assert no regression.
- **Rollback:** `git revert <commit>`.

### 1.3 Core `Nidus::bootstrap` must register declared providers (F-CORE-2)

- **Files:** `crates/nidus-core/src/app/mod.rs`; test `crates/nidus-core/tests/app.rs`
- **Behavior change:** core bootstrap registers module-declared providers so a bootstrapped
  `Application` can resolve them (matches the facade builder's behavior).
- **TDD steps:**
  1. Add a failing test in `tests/app.rs`: build a module with one provider, call
     `Nidus::bootstrap::<M>()?`, then `app.container().resolve::<TheProvider>()?` and assert the
     value.
  2. Run → **fails** with `MissingProvider`.
  3. Implement: in core bootstrap, after building the `Container`, iterate
     `module.provider_registrars()` and run each against the container (mirroring
     `crates/nidus/src/app.rs:99-109`). If async initializers exist, document/run them with the
     same approach as the facade.
  4. Re-run → passes. Ensure facade builder path is unchanged (it still works).
- **Verification:** `cargo test -p nidus-core`; `cargo test -p nidus` (facade); full workspace.
- **Bench:** not required (bootstrap is startup-only, not a measured hot path).
- **Rollback:** `git revert <commit>`.
- **Caveat / decision point:** if running registrars in core bootstrap duplicates or conflicts with
  the facade builder, prefer the **documented** option (mark core bootstrap as graph-only and point
  users at the facade builder) rather than changing behavior. Decide based on what the facade
  builder actually does (read `crates/nidus/src/app.rs:99-113` before implementing).

---

## Wave 2 — cheap, test-first hardening (P2)

### 2.1 Lock in the production middleware order (F-HTTP-4)

- **Files:** `crates/nidus-http/tests/production_api.rs`
- **Behavior change:** none (test-only).
- **TDD steps:** add a test `production_defaults_apply_expected_middleware_order` that builds
  `ApiDefaults::production` over a probe router and asserts observable ordering: a handler `500`
  is enveloped AND metered; a `408` (timeout) is metered and carries `x-request-id`; a `413`
  carries security headers; a `429` (rate-limited) is enveloped. (Order-sensitive properties only.)
- **Verification:** `cargo test -p nidus-http --test production_api`.

### 2.2 `ObservedJobRunner` panic recovery (J-1)

- **Files:** `crates/nidus-jobs/src/lib.rs`; test `crates/nidus-jobs/tests/observed_jobs.rs`
- **TDD steps:** add `observed_job_runner_emits_finished_and_continues_after_panic` → fails (panic
  propagates, no `on_job_finished`). Mirror the queue's `catch_unwind`/`FutureExt::catch_unwind`
  recovery; emit `on_job_finished` with `Failure`; resume/return the failure. Re-run → passes.
- **Verification:** `cargo test -p nidus-jobs`.

### 2.3 Job queue `drain`/`clear` + document re-run semantics (J-2)

- **Files:** `crates/nidus-jobs/src/lib.rs`; test `crates/nidus-jobs/tests/jobs.rs`
- **TDD steps:** add `job_queue_drain_empties_after_run_all` and a test pinning that a second
  `run_all` without `drain` re-runs (documents existing behavior). Add `JobQueue::drain` /
  `AsyncJobQueue::drain` (move jobs out). Re-run → passes.
- **Verification:** `cargo test -p nidus-jobs`.

### 2.4 Fix `openapi` example/docs drift (EX-1, D-1)

- **Files:** `examples/openapi/src/main.rs`; `docs/examples.md`
- **Behavior change:** make the example a real server (matching docs) OR correct the docs. Preferred:
  convert `main()` to `#[nidus::main]` serving the router with `.listen("127.0.0.1:3000")` so the
  documented `/openapi.json` + `/docs` routes are live and curl-able.
- **TDD steps:** update the example's inline tests; manually curl `/openapi.json` + `/docs`.
- **Verification:** `cargo test -p nidus-example-openapi`; manual curl (below).

---

## Manual example testing steps (read from source — do not guess routes)

All server examples default to `127.0.0.1:3000`; run each on a distinct free port via its env var
where supported, otherwise pick a free port. Capture command + HTTP status + body/header, then stop.

- `hello-world`: `cargo run -p nidus-example-hello-world` → `curl -i http://127.0.0.1:3000/`
- `rest-api`: `cargo run -p nidus-example-rest-api` → `curl -i http://127.0.0.1:3000/users/1`
- `auth-api`: `cargo run -p nidus-example-auth-api` → `curl -i http://127.0.0.1:3000/me`
- `openapi` (after 2.4): `cargo run -p nidus-example-openapi` → `curl -i http://127.0.0.1:3000/openapi.json` and `/docs`
- `production-api`: `NIDUS_ADDR=127.0.0.1:<port> cargo run -p production-api` → curl `/health/live`,
  `/health/ready`, `/metrics`, `/users/1`
- `realworld-api`: `NIDUS_BIND_ADDR=127.0.0.1:<port> cargo run -p nidus-example-realworld-api` → curl
  `/health/live`, `/health/ready`, `/metrics`, `/openapi.json`, `/users/1`,
  `POST /projects` with `x-api-key: dev-secret`
- (If 2.4 is deferred, `openapi` manual curl is skipped and noted as a limitation.)

Never leave servers running: capture output then stop the process.

## Validation gates (run after each wave, and at finalize)

```
git diff --check
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
cargo test --workspace --all-features
cargo test -p <each changed package>
cargo bench --bench dependency_resolution      # only if a resolution-path change landed
cargo bench --bench request_lifecycle          # only if an HTTP/middleware change landed
cargo tree -d
cargo deny check   # if available
cargo audit        # if available
cargo machete      # if available
```

## Rollback strategy

Each wave is a separate atomic commit. Any wave can be reverted in isolation with
`git revert <commit>` without affecting the others or the audit/plan documents. The audit (commit
`a63a2b9`) is independent and stays regardless.

## Deferred items (intentionally out of scope for this plan)

- F-CORE-3 graph TypeId re-keying; F-CORE-4 eager singleton resolution; F-CORE-5 request-dep docs.
- F-HTTP-2 streaming body limit; F-HTTP-3 413 ordering; F-HTTP-5 graceful shutdown + ConnectInfo;
  F-HTTP-6 client_ip_identity hardening; F-HTTP-7 panic-catching layer; F-HTTP-8 prometheus series cap.
- F-MAC-2 spanned diagnostics; O-1 OpenAPI error-response modeling; O-2 parity test (covered
  partially by 2.1's probe); ERR-1 5xx code masking.
- E-1/E-2/E-3 bounded event queues + observer offloading.
- EX-2 auth-api guard realism; EX-3 production-api naming; EX-4 orphan `sqlx-postgres` dir cleanup;
  EX-5 example `.expect()` cleanup.
- CLI coverage beyond CLI-1/CLI-2; AD-1/AD-2/AD-3 adapter registration + health wiring + coverage.
- T-1 TestApp request-scope helper; T-2 spurious-async assertions.
- BENCH-1 baseline locking.

These are tracked in the audit backlog and will be addressed in follow-up phases.

---

## Wave 3 — reliability hardening + parity coverage (second session, after `ac108ef`)

Status: **implemented** (commits `66834f7`, `dcfbf0a`, `3070c07`). See the audit's
"Follow-up hardening — second pass" section for full evidence.

- **3a — F-HTTP-8 / SEC-3**: opt-in `PrometheusMetrics::with_max_series(n)` cardinality
  cap (overflow bucket). Default uncapped path unchanged and zero-overhead. Metrics
  criterion benches: no change (p > 0.05). (Default-cap change deferred.)
- **3b — E-1 / SEC-3**: opt-in `EventBus::subscribe_with_capacity(cap)` bounded
  subscriber (drop-oldest). Default unbounded path unchanged.
- **3c — O-2**: route↔spec parity tests (`from_route_metadata`, `from_controller_routes`).
- **3d — V-1**: malformed-JSON 400 test + `ValidatedJson` 422 ↔ `ErrorEnvelopeLayer`
  composition integration test (workspace-level).

### Reclassified out of the backlog (evidence in audit follow-up section)

- **F-MAC-1** is **not a defect**: the runtime `ApplicationBuild` is intentional to support
  manually-constructed controllers (`controller_routes.rs`, `routes_generic_controller.rs`).
  A compile-error enforcement was implemented, TDD-tested, and reverted to avoid regressing
  those patterns. Removed from the deferred backlog.

### Verification after Wave 3

fmt/clippy/doc clean; `cargo test --workspace --all-features` → 354 passed / 0 failed
(+9); deny/machete/tree clean; production-api manual curl confirms metrics recording and
exclusion still work. Benchmark decision: `request_lifecycle` metrics scenarios re-run
because `metrics.rs` (hot path) was touched — no regression (p > 0.05). `dependency_resolution`
and `routing` benches not re-run: no DI or routing hot-path code changed.

---

## Wave 4 — production server-path hardening: ConnectInfo + graceful shutdown (F-HTTP-5)

Status: **implemented** (commits below). See the audit's "Follow-up hardening —
Wave 4" section for full evidence.

### 4a — F-HTTP-5: `ConnectInfo<SocketAddr>` on the blessed serve path

- **Files:** `crates/nidus-http/src/server.rs`; test `crates/nidus-http/tests/server.rs`
- **Behavior change:** every serving method wraps the router with
  `into_make_service_with_connect_info::<SocketAddr>()`. `listen` keeps its public
  signature (no break); `serve`/`serve_with_graceful_shutdown`/`listen_with_graceful_shutdown`
  are new. `client_ip_identity` now classifies by real peer IP, not spoofable XFF / `"anonymous"`.
- **TDD steps:** (1) wrote `serve_populates_connect_info_for_peer_identity` +
  `serve_with_graceful_shutdown_drains_and_exits_cleanly`; (2) confirmed RED on a
  no-ConnectInfo `serve` (`Missing request extension: ConnectInfo<SocketAddr>`); (3) added
  ConnectInfo + the graceful-shutdown API; (4) GREEN.
- **Verification:** `cargo test -p nidus-http --test server` (3 passed);
  `cargo clippy -p nidus-http --all-targets --all-features -- -D warnings`; `cargo fmt -p nidus-http -- --check`.
- **Design note:** graceful shutdown is opt-in via the explicit `*_with_graceful_shutdown`
  methods (matches axum's `axum::serve`); no tokio `signal` feature needed.
- **Bench:** not required (connection/serve boundary, not a measured request/DI hot path).
- **Manual curl:** production-api `GET /limited` #1 → 200, #2 with spoofed
  `X-Forwarded-For: 1.2.3.4` → 429 (proves real-peer-IP identity overrides XFF).
  All six server examples curled green (see audit table).
- **Rollback:** `git revert <commit>`.

### Verification after Wave 4

`cargo fmt --all --check`, `cargo clippy -p nidus-http --all-targets --all-features -- -D
warnings`, `cargo test -p nidus-http` clean; six server examples build against the updated
`listen`; full workspace gate run at finalize (see audit). Benchmark decision: not required
(connection/serve boundary, not a measured request/DI hot path).

---

## Wave 5 — example realism: header-based guard in `auth-api` (EX-2)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 5" section.

- **Files:** `examples/auth-api/src/main.rs` (guard + tests).
- **Behavior change:** example-only. `ApiKeyGuard` reads `x-api-key` instead of checking the
  route label, returning `unauthorized` on failure. End-to-end proof of the Wave-1 `guard_layer`
  header-passing fix.
- **TDD:** 6 tests cover valid/missing/wrong key at both the guard unit level and the router
  integration level; manual curl confirmed on the running server.
- **Verification:** `cargo test -p nidus-example-auth-api` (6 passed); fmt/clippy clean.
- **Bench:** not required (example-only, no hot path).

---

## Wave 6 — production envelope: mask 5xx `code` (ERR-1)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 6" section.

- **Files:** `crates/nidus-http/src/error.rs`; test `crates/nidus-http/tests/production_api.rs`.
- **Behavior change:** the production error envelope now masks `code` to the generic
  `internal_server_error` on a 5xx (previously a handler-supplied code like `database_error`
  leaked while `message`/`details` were masked). Server logs retain the original code.
- **TDD:** strengthened the pinning test to assert the mask; verified RED (`database_error`
  leaked) then GREEN.
- **Verification:** `cargo test -p nidus-http --test production_api` (23 passed);
  `cargo test --workspace --all-features` (358 passed); fmt/clippy clean.
- **Bench:** not required — the changed 5xx branch is off every measured `request_lifecycle`
  path (success short-circuits before `envelope_response`); confirmed via two noisy re-runs.

---

## Wave 7 — production reliability: panic-catching layer (F-HTTP-7)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 7" section.

- **Files:** `crates/nidus-http/src/middleware/catch_panic.rs` (new), `api_defaults.rs`,
  `security.rs`, `middleware.rs`, `Cargo.toml`; test `production_api.rs`.
- **Behavior change:** `ApiDefaults::production` now catches handler panics (default-on) and
  renders them as the production `500` envelope (request-id + metrics), instead of aborting the
  connection. Opt out with `without_catch_panic()`.
- **Design:** nidus-native layer (not `tower_http::catch_panic`, whose `UnsyncBoxBody` response
  doesn't compose with `ErrorEnvelopeLayer`'s `Response<Body>`). Preserves the body type.
- **TDD:** `production_defaults_envelope_panic_as_500` RED (panic propagated) → GREEN.
- **Verification:** `cargo test -p nidus-http` (24 in production_api); `cargo test --workspace
  --all-features` (359 passed); fmt/clippy/doc/deny/machete/tree/audit clean.
- **Bench:** within-session A/B vs a saved baseline — ~250ns/~6% on the bare production stack
  (borderline p=0.02), undetectable on production+metrics (p=0.43). Earlier cross-session
  "regressions" were noise (~40% run-to-run swing). Acceptable for default-on panic safety.
- **Manual curl:** production-api normal routes unaffected.

---

## Wave 8 — production observability: enveloped/metered 413 (F-HTTP-3)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 8" section.

- **Files:** `crates/nidus-http/src/middleware/api_defaults.rs`; test `production_api.rs`.
- **Behavior change:** `body_limit` moved inside `validated_request_id`/`metrics`/`ErrorEnvelope`,
  so a `413` is enveloped, metered, and carries a request id (consistent with `408`). Order-only.
- **TDD:** `production_defaults_envelope_and_meter_body_limit_rejections` RED (`413 must carry a
  request id`) → GREEN.
- **Verification:** `cargo test -p nidus-http --test production_api` (25 passed); `cargo test
  --workspace --all-features` (360 passed); fmt/clippy/doc clean.
- **Bench:** no regression — order-only change (still 9 layers); production scenarios ~3.8 µs
  (bare, p=0.12) / ~4.45 µs (with metrics).
- **Manual curl:** production-api 2 MB body → 413 JSON envelope + `x-request-id` + metered.

---

## Wave 9 — OpenAPI completeness: error responses (O-1)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 9" section.

- **Files:** `crates/nidus-openapi/src/route.rs`; tests `route_metadata.rs`.
- **Behavior change:** OpenAPI operations now advertise `401`/`403` (guarded routes) and `422`
  (validating routes), description-only. Plain routes unchanged.
- **TDD:** emits-error-responses (RED) + omits-for-plain-routes (no-change pin) → GREEN.
- **Verification:** `cargo test -p nidus-openapi`; `cargo test --workspace --all-features`
  (362 passed); fmt/clippy clean.
- **Bench:** not required (build-time doc generation, not a request hot path).
- **Manual curl:** realworld-api `POST /projects` spec now lists 201/401/403/422.

---

## Wave 10 — testing ergonomics: TestApp request scope (T-1)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 10" section.

- **Files:** `crates/nidus-testing/src/app.rs`; test `tests/request_scope.rs`.
- **Behavior change:** `TestAppBuilder::with_request_scope()` installs the request scope layer so
  `RequestScoped<T>` extractors resolve in HTTP integration tests (additive; off by default).
- **TDD:** enables-extractor (RED: method missing → GREEN: 200) + rejects-without (pins 500).
- **Verification:** `cargo test -p nidus-testing`; `cargo test --workspace --all-features`
  (364 passed); fmt/clippy/doc clean.
- **Bench:** not required (test infrastructure, not a request hot path).

---

## Wave 11 — CLI coverage: all-artifact compile test (CLI-1)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 11" section.

- **Files:** `crates/cargo-nidus/tests/cli_generate.rs` (test-only).
- **Behavior change:** none (test-only). Generates all four artifacts and runs `cargo check
  -Dwarnings` to verify the module wiring compiles end-to-end.
- **Verification:** `cargo test -p cargo-nidus --test cli_generate` (7 passed); `cargo test
  --workspace --all-features` (365 passed); fmt/clippy clean.
- **Bench:** not required (code-generation CLI, not a request hot path).

---

## Wave 12 — adapter coverage + F-CORE-3 deferral (AD-3 partial; F-CORE-3)

Status: **implemented** (AD-3 cache) / **deferred with rationale** (F-CORE-3). See the audit's
"Follow-up hardening — Wave 12" section.

- **Files (AD-3):** `crates/nidus-cache/tests/moka_cache.rs` (test-only).
- **AD-3 change:** added `invalidate` (removes only the targeted key) and `from_cache` (wraps a
  caller-owned Moka cache + namespace) tests. `nidus-sqlx` health/Postgres-config stay out of scope
  (need a live DB).
- **F-CORE-3:** investigated and deferred — proper fix is a public API change; DI resolution is
  already `TypeId`-safe, so only graph validation can false-positive (rare). Documented workaround.
- **Verification:** `cargo test -p nidus-cache`; `cargo test --workspace --all-features`
  (367 passed); fmt/clippy/doc clean.
- **Bench:** not required (adapter unit coverage, not a request hot path).

---

## Wave 13 — body-limit hardening: opt-in streaming cap (F-HTTP-2 / SEC-1)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 13" section.

- **Files:** `crates/nidus-http/src/middleware/api_defaults.rs`; tests `production_api.rs`.
- **Behavior change:** additive only — new `ApiDefaults::streaming_body_limit(max_bytes)` opt-in
  layers `streaming_body_limit_layer` to cap headerless/chunked bodies as they are read. Default
  stack unchanged (`Content-Length`-only); two-tier model documented.
- **TDD:** documents-bypass (headerless body → 200) + caps-with-streaming (→ 413).
- **Verification:** `cargo test -p nidus-http --test production_api` (27 passed); `cargo test
  --workspace --all-features` (369 passed); fmt/clippy/doc clean.
- **Bench:** not required — opt-in (default off); default production stack unchanged.

---

## Wave 14 — async safety: opt-in eager singleton resolution (F-CORE-4 / RT-2)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 14" section.

- **Files:** `crates/nidus-core/src/container/mod.rs`; tests `tests/core_di.rs`.
- **Behavior change:** additive only — new `Container::eagerly_resolve_singletons()` pre-constructs
  every singleton at startup so the lazy `Condvar` wait is never reached from async handlers. Default
  lazy resolution unchanged.
- **TDD:** constructs-once-and-caches (RED: method missing → GREEN) + skips-transient/request +
  propagates-factory-errors.
- **Verification:** `cargo test -p nidus-core`; `cargo test --workspace --all-features`
  (372 passed); fmt/clippy/doc clean.
- **Bench:** not required (opt-in, default unchanged; runs at startup, not request hot path).

## Wave 15 — CLI coverage: default published dependency branch (CLI-2)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 15" section.

- **Files:** `crates/cargo-nidus/tests/cli_new.rs`; audit status table.
- **Behavior change:** none (test-only).
- **Coverage:** `cargo_nidus_new_defaults_to_published_nidus_dependency` runs `cargo nidus new`
  without `--nidus-path` and asserts the generated manifest uses `nidus = "0.1"` instead of a local
  `path` dependency.
- **Verification:** focused test (1 passed) and full `cli_new` suite (4 passed).
- **Bench:** not required (CLI manifest generation, not a framework hot path).

## Wave 16 — CLI ergonomics: generated service name follows project name (CLI-3)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 16" section.

- **Files:** `crates/cargo-nidus/src/generate.rs`, `crates/cargo-nidus/tests/cli_new.rs`; audit.
- **Behavior change:** generated `cargo nidus new <name>` projects use `<name>` in
  `ApiDefaults::production(...)` instead of always using `"hello-nidus"`.
- **TDD:** `cargo_nidus_new_uses_project_name_for_service_name` RED on the hardcoded template, GREEN
  after replacing the template marker with the requested project name.
- **Verification:** focused test (1 passed) and full `cli_new` suite (5 passed).
- **Bench:** not required (CLI template generation, not a framework hot path).

## Wave 17 — config ergonomics: array indexes in path helpers (C-1)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 17" section.

- **Files:** `crates/nidus-config/src/lib.rs`, `crates/nidus-config/tests/env_paths.rs`,
  `docs/config.md`; audit.
- **Behavior change:** additive — `get_path` and typed path helpers traverse arrays when a path
  segment is a zero-based numeric index.
- **TDD:** raw and typed array path tests RED (`None` at array boundary), GREEN after array traversal.
- **Verification:** `cargo test -p nidus-config --test env_paths`; `cargo test -p nidus-config`.
- **Bench:** not required (config startup/test helper, not request hot path).

## Wave 18 — config docs/tests: env prefix case sensitivity (C-2)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 18" section.

- **Files:** `crates/nidus-config/tests/env_paths.rs`, `docs/config.md`; audit.
- **Behavior change:** none (test/docs-only).
- **Coverage:** `config_matches_env_prefix_case_sensitively` pins that `APP_*` matches prefix `APP`
  while lowercase `app_*` is ignored.
- **Verification:** `cargo test -p nidus-config --test env_paths`; `cargo test -p nidus-config`.
- **Bench:** not required (test/docs-only).

## Wave 19 — async job observation: no span guard across await (J-4)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 19" section.

- **Files:** `crates/nidus-jobs/src/lib.rs`, `crates/nidus-jobs/tests/observed_jobs.rs`; audit.
- **Behavior change:** internal instrumentation hygiene only — observer callbacks and job outcomes
  are unchanged.
- **Coverage:** `observed_job_runner_async_future_is_send` asserts the async runner future is `Send`.
  The assertion already passed before the refactor, refining the audit finding; the implementation
  still removes the tracing `Entered` guard across `.await`.
- **Verification:** `cargo test -p nidus-jobs --test observed_jobs`; `cargo test -p nidus-jobs`;
  `cargo clippy -p nidus-jobs --all-targets --all-features -- -D warnings`.
- **Bench:** not required (job observation, not HTTP/DI/routing/request lifecycle).

## Wave 20 — deployment docs: default-on vs opt-in API defaults (D-2)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 20" section.

- **Files:** `docs/deployment.md`; audit.
- **Behavior change:** none (docs-only).
- **Docs change:** default-on `ApiDefaults::production` middleware concerns are listed separately
  from opt-in metrics and rate limiting.
- **Verification:** `git diff --check`; `cargo fmt --all --check`.
- **Bench:** not required (docs-only).

## Wave 21 — CLI diagnostics: missing cargo-expand guidance (CLI-4)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 21" section.

- **Files:** `crates/cargo-nidus/src/main.rs`, `crates/cargo-nidus/tests/cli.rs`,
  `docs/getting-started.md`; audit.
- **Behavior change:** failed `cargo nidus expand` now recognizes Cargo's missing `expand`
  subcommand output and tells users to install `cargo-expand`.
- **TDD:** fake `cargo` in `PATH` emits `no such command: expand`; test was RED with generic
  failure, GREEN with install guidance.
- **Verification:** focused expand test; full `cli` test target; `cargo test -p cargo-nidus`;
  `cargo clippy -p cargo-nidus --all-targets --all-features -- -D warnings`.
- **Bench:** not required (CLI diagnostics only).

## Wave 22 — CLI graph discovery: recursive src scanning (CLI-5)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 22" section.

- **Files:** `crates/cargo-nidus/src/graph.rs`, `crates/cargo-nidus/tests/cli_graph.rs`; audit.
- **Behavior change:** `cargo nidus graph` now scans all `.rs` files below `src/`, so generated
  controllers, services, and repositories outside `src/modules/` are visible.
- **TDD:** generated feature-directories test RED when graph omitted `UsersController` /
  `UsersService` / `UsersRepository`, GREEN after recursive source discovery.
- **Verification:** focused graph test; full `cli_graph` target; `cargo test -p cargo-nidus`;
  `cargo clippy -p cargo-nidus --all-targets --all-features -- -D warnings`.
- **Bench:** not required (CLI source inspection only).

## Wave 23 — testkit response assertions: synchronous helpers (T-2 / API-3)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 23" section.

- **Files:** `crates/nidus-testing/src/response.rs`, nidus-testing tests, example tests,
  `docs/testing.md`; audit.
- **Behavior change:** `TestResponse::assert_text` and `TestResponse::assert_json` no longer return
  immediately-ready futures; callers assert synchronously after awaiting `send()`.
- **TDD:** one `assert_json(...)` call was changed to the synchronous form first. RED under
  `cargo clippy -p nidus-testing --test http_testing --all-features -- -D warnings` because the
  async helper returned an unused `Future`, GREEN after making the helpers synchronous.
- **Verification:** focused http_testing case; `cargo test -p nidus-testing`;
  `cargo clippy -p nidus-testing --all-targets --all-features -- -D warnings`; affected
  `nidus-openapi` and example package tests/clippy.
- **Bench:** not required (test-helper ergonomics only).

## Wave 24 — README DI wrapper clarity (API-1)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 24" section.

- **Files:** `README.md`; audit.
- **Behavior change:** none (docs-only).
- **Docs change:** top-level features now distinguish auto-wired `Inject<T>` / `Optional<T>`,
  request-scope `Scoped<T>`, and manual `Lazy<T>` / `Factory<T>` helpers.
- **Verification:** `git diff --check`; `cargo fmt --all --check`.
- **Bench:** not required (docs-only).

## Wave 25 — adapter health registry bridges (AD-2)

Status: **partially implemented**. See the audit's "Follow-up hardening — Wave 25" section.

- **Files:** `crates/nidus-sqlx/src/lib.rs`, `crates/nidus-cache/src/lib.rs`, adapter tests,
  `examples/integrations-production/src/main.rs`, `docs/deployment.md`, `docs/integrations.md`;
  audit.
- **Behavior change:** providers built with the `health` feature can attach themselves as
  readiness checks through `register_ready_check(...)`.
- **TDD:** cache and SQLite tests called the new helper first. RED on missing method; GREEN after
  adding helpers.
- **Verification:** focused cache health-feature test; focused SQLx health-feature test with
  `sqlite postgres nidus-config health` enabled; full changed adapter/example package checks.
- **Bench:** not required (additive health-registration helper only).
- **Deferred:** live Postgres health execution still needs an external Postgres service and remains
  intentionally out of deterministic local tests.

## Wave 26 — audit consistency: validation coverage status (V-1)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 26" section.

- **Files:** audit only.
- **Behavior change:** none.
- **Docs change:** V-1 is no longer listed as open in the crate summary because Wave 3 already
  added malformed-JSON 400 coverage and validation-envelope composition coverage.
- **Verification:** focused `nidus-validation` malformed JSON test; focused workspace
  `validation_envelope` composition test.
- **Bench:** not required (audit/status-only correction).

## Wave 27 — CLI OpenAPI document metadata flags (O-3)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 27" section.

- **Files:** `crates/cargo-nidus/src/main.rs`, `crates/cargo-nidus/src/openapi_doc.rs`,
  `crates/cargo-nidus/tests/cli_openapi.rs`, `docs/getting-started.md`, `docs/openapi.md`; audit.
- **Behavior change:** `cargo nidus openapi` accepts `--title` and `--version` while keeping the
  existing defaults when flags are omitted.
- **TDD:** new CLI test failed before the flags existed, then passed once the OpenAPI generator
  accepted document metadata options.
- **Verification:** focused metadata flag test; full `cli_openapi` target; `cargo test -p
  cargo-nidus`; cargo-nidus clippy.
- **Bench:** not required (CLI metadata only).

## Wave 28 — legacy request ID UUID generation (F-HTTP-9)

Status: **implemented**. See the audit's "Follow-up hardening — Wave 28" section.

- **Files:** `crates/nidus-http/src/middleware/request_id.rs`,
  `crates/nidus-http/tests/middleware.rs`, `benches/request_lifecycle.rs`, `docs/interceptors.md`;
  audit.
- **Behavior change:** `request_id_layer()` now generates UUID v4 values instead of
  `nidus-<nanos>` when no request or response ID is present. It still propagates arbitrary incoming
  IDs and preserves handler response IDs.
- **TDD:** new middleware test failed while generated IDs used the `nidus-` timestamp format, then
  passed after switching to UUID v4 generation.
- **Verification:** focused middleware test; full `middleware` test target; `cargo test -p
  nidus-http`; nidus-http clippy; request-lifecycle bench clippy.
- **Bench:** `cargo bench --bench request_lifecycle` with a new legacy request-id scenario. First
  legacy scenario measurement has no prior baseline; broader run was mixed, so no performance
  improvement is claimed.
