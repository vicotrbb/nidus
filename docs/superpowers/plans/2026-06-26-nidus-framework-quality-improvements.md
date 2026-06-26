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
- CLI-1/CLI-2 CLI compile-coverage tests; AD-1/AD-2/AD-3 adapter registration + health wiring + coverage.
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
