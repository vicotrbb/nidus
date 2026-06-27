# Nidus Framework Quality Audit

- Date: 2026-06-26
- Scope: full repository at `main` (commit `70bf62a`), all 14 crates, 11 example packages, 3 benches, docs
- Method: source inspection of every crate/example/bench + verification baseline commands
- Author: automated framework hardening pass (evidence-backed; no overclaiming)

## Severity scale

- **P0** — correctness/security/release blocker
- **P1** — important framework-quality issue (real defect with user-visible impact)
- **P2** — useful hardening / ergonomics / coverage gap
- **P3** — polish

## Verification baseline (recorded before any changes)

All run on the audited commit:

```
cargo build --workspace --all-features             -> Finished, 0 errors
cargo test --workspace --all-features              -> ok (all suites), 0 failures
```

The full test suite is green: ~260 tests across unit/integration/doc-tests pass (doctests are
intentionally `#[ignore]`d and reported as ignored). Per-suite counts sampled:
`nidus-core` 51, `nidus-http` 45+ (`production_api` suite included), `cargo-nidus` 60,
`nidus-testing` 26, `nidus-openapi` 27, `nidus-config` 21, `nidus-validation` 9, `nidus-auth` 10,
`nidus-events` 6, `nidus-jobs` 8, all examples 34.

## Architecture summary

Nidus is a modular Rust backend framework inspired by NestJS ergonomics, built directly on
Axum 0.8, Tower 0.5, Tokio, serde, validator, utoipa, and tracing. It composes these crates
instead of replacing them.

Workspace layout (`Cargo.toml`):

- `nidus` — public facade + prelude + `NidusApplicationBuilder`; feature-gated re-exports.
- `nidus-core` — `Container` (type-keyed, `HashMap<TypeId, ProviderEntry>`), providers, lifetimes
  (`Singleton`/`Transient`/`Request`), `RequestScope`, `ModuleGraph` validation, `LifecycleRunner`,
  `NidusError`.
- `nidus-macros` — attribute macros: `module`, `injectable`, `controller`, `routes`, HTTP verbs,
  `openapi`, `guard`, `pipe`, `validate`, `main`. Compile-fail + insta snapshot coverage.
- `nidus-http` — `Controller`/`RouteDefinition` route composition, router/path math, `HttpError`,
  production `ErrorEnvelopeLayer`, `RequestContext`/request-id, `ApiDefaults::production` stack,
  `HealthRegistry`, `PrometheusMetrics`, rate limiting, logging, OTel helpers (`otel` feature).
- `nidus-config` — `Config` (serde-based, layered merge, typed access, path errors).
- `nidus-openapi` — `OpenApiDocument` from controller `RouteMetadata`, `/openapi.json` + `/docs`.
- `nidus-validation` — `Pipe` trait, `ValidationPipe`, `ValidatedJson<T>` (422 shape).
- `nidus-auth` — `Guard`/`GuardExt`/`GuardContext`/`GuardError`, `guard_layer` Tower integration.
- `nidus-events` — in-process `EventBus` (weak subscribers), `EventObserver`.
- `nidus-jobs` — in-memory `JobQueue`/`AsyncJobQueue` (batch, panic-recovering), `ObservedJobRunner`.
- `nidus-sqlx`, `nidus-cache` — separately installable adapters (outside the facade).
- `nidus-testing` — `TestApp`/`TestAppBuilder` (in-memory `oneshot`), provider/config overrides.
- `cargo-nidus` — CLI (`new`, `generate`, `routes`, `graph`, `expand`, `check`, `openapi`).

Dependency direction is clean and inward: facade → core/macros/http/...; adapters depend only on
`nidus-core` (+ optional `nidus-http`/`nidus-config`). No circular crate dependencies. The
`nidus-workspace` root package only hosts the three Criterion benches.

## Crate-by-crate findings

### nidus-core (DI, modules, lifecycle)

#### F-CORE-1 — Panicking singleton factory permanently deadlocks that provider (P1)
- **Evidence:** `crates/nidus-core/src/provider/mod.rs:136-154`. `resolve_singleton` enters
  resolution (`:137`), sets `SingletonState::Initializing` (`:138`), drops the lock (`:139`),
  then calls `self.create_erased(container)` (`:141`) with **no `catch_unwind`**. The `Err`
  branch (`:149-152`) resets state to `Empty`, but a **panic unwinds past it**. The
  `resolution::enter` guard's `Drop` removes the type from the stack, so later resolves see
  `Initializing` (`:128`) and `is_active` returns false (`:129`), then **block forever** on
  `wait_unpoisoned` (`:134`).
- **Files:** `crates/nidus-core/src/provider/mod.rs`
- **Risk:** One `panic!`/index-oob/`.unwrap()` inside any singleton factory (or a transitive dep)
  makes that provider unresolvable for the entire process lifetime, silently hanging request
  handlers that touch it.
- **Fix:** Wrap `create_erased` in `std::panic::catch_unwind(AssertUnwindSafe(..))`; on panic,
  reset state to `Empty`, `notify_all`, then either `resume_unwind` or return a `NidusError`.
- **Verification:** add a test that registers a panicking singleton factory, triggers a resolve,
  and asserts a second resolve returns an error (not a hang) — run under a timeout.

#### F-CORE-2 — Core `Nidus::bootstrap` yields an empty container (no providers) (P1)
- **Evidence:** `crates/nidus-core/src/app/mod.rs:56-69` builds `Container::new()` and never runs
  `module.provider_registrars()` / `async_initializers()`. Only the facade
  `NidusApplicationBuilder::build` (`crates/nidus/src/app.rs:99-109`) registers providers. The
  core `app.rs` / `lifecycle_bootstrap.rs` tests assert module presence, never provider resolution.
- **Files:** `crates/nidus-core/src/app/mod.rs`; contrast `crates/nidus/src/app.rs`
- **Risk:** `Nidus::bootstrap::<M>()` followed by `app.container().resolve::<T>()` returns
  `MissingProvider` despite declared providers. Surprising for anyone using the documented core API.
- **Fix:** Either register providers in core bootstrap, or rename/gate the core bootstrap and
  document that registration requires the facade builder; add a resolution test.
- **Verification:** `cargo test -p nidus-core --test app` (new resolution assertion).

#### F-CORE-3 — Module graph keyed by short type name, not `TypeId`; no dependency completeness (P2)
- **Evidence:** name derivation at `crates/nidus-core/src/module/mod.rs:230-236,271-277`
  (`rsplit("::").next()`); `graph.rs:123-241` compares these strings. Missing transitive deps
  surface only at runtime resolution.
- **Files:** `crates/nidus-core/src/module/{mod,graph}.rs`
- **Risk:** Distinct types sharing a simple name (e.g. `auth::Session` vs `billing::Session`)
  trigger false `DuplicateModuleProvider`/`AmbiguousProvider`.
- **Fix:** Key graph identity on `TypeId`; optionally capture `Inject<T>` field types in
  `#[injectable]` for a real dependency graph.
- **Verification:** regression test with two same-simple-name providers.

#### F-CORE-4 — Blocking `Condvar` waits reachable from async handlers (P2)
- **Evidence:** `provider/mod.rs:134` (`wait_unpoisoned`), `request_scope.rs:124`. First-time
  singleton resolution can happen lazily during request handling (providers are not eagerly
  resolved at build — `crates/nidus/src/app.rs:96-113`).
- **Files:** `crates/nidus-core/src/provider/mod.rs`, `crates/nidus-core/src/request_scope.rs`
- **Risk:** Under contention a Tokio worker thread blocks on the condvar; many stalls starve the
  runtime.
- **Fix:** Eagerly resolve singletons at bootstrap, or document; reserve the wait for the sync API.
- **Verification:** `cargo bench --bench dependency_resolution` before/after any change.

#### F-CORE-5 — `register_request` providers cannot chain request-scoped deps (P2)
- **Evidence:** `container/mod.rs:84-90` registers only the container factory;
  `create_erased_in_scope` falls back to `create_erased(scope.container())` (`provider/mod.rs:166-168`),
  so a `container.inject::<OtherRequest>()` fails with `RequestScopeRequired`. Only
  `register_request_scoped` / `#[injectable(request)]` chain correctly.
- **Files:** `crates/nidus-core/src/container/mod.rs`, `crates/nidus-core/src/provider/mod.rs`
- **Risk:** Subtle trap for lower-level API users. (Already documented at
  `docs/dependency-injection.md:84-87` for the `register_request_scoped` path.)
- **Fix:** Document the limitation explicitly, or unify `register_request` to pass the scope.

### nidus-macros (diagnostics)

#### F-MAC-1 — `#[controller]` non-injectable fields defer to runtime error, not compile error (P2)
- **Evidence:** `crates/nidus-macros/src/controller.rs:42-67,97-116`. A field type that is not
  `Inject`/`Optional` becomes a generated method returning a runtime `NidusError::ApplicationBuild`
  rather than a `syn::Error::new_spanned` at expansion (contrast `injectable.rs:60-67` which does
  emit compile errors for its fields).
- **Files:** `crates/nidus-macros/src/controller.rs`
- **Risk:** Structurally invalid controllers compile and fail at first instantiation.
- **Fix:** Emit a spanned compile error for non-`Inject`/`Optional` named fields.
- **Verification:** add a `tests/ui` compile-fail case (trybuild).

#### F-MAC-2 — Attribute-level macro errors use `Span::call_site()` (P3)
- **Evidence:** `crates/nidus-macros/src/diagnostics.rs:5-7`; consumers in `controller.rs`,
  `guard.rs`, `pipe.rs`, `entrypoint.rs`, `module.rs:51`, `injectable.rs:15`. Route/openapi errors
  are spanned (`new_spanned`); attribute placement errors point at the whole invocation.
- **Files:** `crates/nidus-macros/src/diagnostics.rs`
- **Risk:** Poorer DX for macro-misplacement errors.
- **Fix:** Thread the offending token/span into `compile_error`.

### nidus-http (controllers, middleware, errors, health, metrics)

#### F-HTTP-1 — `guard_layer` never populates request headers; inner service called before authorization (P1)
- **Evidence:** `crates/nidus-auth/src/middleware.rs:82-93`. `call()` moves `request` into
  `self.inner.call(request)` at `:86` **before** the guard runs at `:89`;
  `GuardContext::new(state, route_label)` is built with **no headers**, so `ctx.headers()` is
  always an empty `HeaderMap` through this layer. The macro path
  (`crates/nidus-macros/src/routes.rs:176-177`) DOES pass headers, so the two wiring paths are
  inconsistent.
- **Files:** `crates/nidus-auth/src/middleware.rs` (hosted in nidus-auth; the layer's home)
- **Risk:** Any header/token guard wired via the documented public `guard_layer` is silently broken
  (always-deny for header-required guards; always-allow for "no header = anonymous" guards).
  `examples/realworld-api/src/auth/guard.rs:14-21` (`ApiKeyGuard` reading `x-api-key`) would be
  broken if wired through the layer. Existing `guard_layer` tests only check `route_label`/`state`,
  so the bug is latent.
- **Fix:** Split headers off the request before calling inner; authorize first; pass headers:
  `GuardContext::new(state, route_label).with_headers(parts.headers.clone())`, then call inner on
  success.
- **Verification:** regression test asserting a header-reading guard receives the header through
  `guard_layer` (`cargo test -p nidus-auth`).

#### F-HTTP-2 — Production body limit is `Content-Length`-only (bypassable) (P2)
- **Evidence:** `crates/nidus-http/src/middleware/security.rs:171-185` (`body_limit_layer` checks
  only `Content-Length`); `api_defaults.rs:93,282-284` uses it for the 1 MiB default. Docs at
  `security.rs:80-89,199-204` acknowledge this; test `body_limit_layer_allows_undeclared_body_sizes`
  proves the bypass.
- **Files:** `crates/nidus-http/src/middleware/{security,api_defaults}.rs`
- **Risk:** Chunked-transfer clients (no `Content-Length`) bypass the stated limit on the wire.
- **Fix:** Layer `streaming_body_limit_layer` (tower-http `RequestBodyLimitLayer`, already a dep)
  into `ApiDefaults::production`, or document the two-tier model prominently.
- **Verification:** `cargo test -p nidus-http --test logging_otel_security`; new streaming-limit test.

#### F-HTTP-3 — 413 (body-limit) responses bypass request-id, metrics, and error envelope (~~P2~~ mitigated, Wave 8)
- **Evidence:** `body_limit_layer` was the outermost functional layer (after `security_headers`),
  so a `413` was produced before `validated_request_id`/`metrics`/`ErrorEnvelope` ran. Now moved
  inside those layers in `ApiDefaults::apply` (`crates/nidus-http/src/middleware/api_defaults.rs`),
  so an oversized-body `413` flows out through the envelope (enveloped), metrics (metered), and the
  request-id layer (carries `x-request-id`) — consistent with how `408` timeouts are observed.
- **Files:** `crates/nidus-http/src/middleware/api_defaults.rs`; test `production_api.rs`.
- **Verification:** `production_defaults_envelope_and_meter_body_limit_rejections` (413 → JSON
  envelope `statusCode:413` + `x-request-id` + metered); manual curl of production-api with a 2 MB
  body → `413` `application/json` envelope + `x-request-id: <uuid>` + security headers + metered.
  The order-only change adds no per-request layer (still 9 layers); `request_lifecycle` production
  scenarios show no regression (~3.8 µs bare, ~4.45 µs with metrics).

#### F-HTTP-4 — No test pins the production middleware order (P2)
- **Evidence:** `tests/production_api.rs` tests only behavioral side-effects; no assertion that the
  layer sequence equals the documented stack (`api_defaults.rs:246-254`).
- **Files:** `crates/nidus-http/tests/production_api.rs`
- **Risk:** A future refactor can silently reorder layers (e.g., move `ErrorEnvelope` inside
  `metrics`, or `body_limit` inside `request_id`) without any test failure.
- **Fix:** Add an order-probe test (envelope present on a handler 500, metric recorded on a 408,
  security header present on 413/429, request-id echoed on all paths).
- **Verification:** `cargo test -p nidus-http --test production_api`.

#### F-HTTP-5 — `HttpApplication::listen` lacks graceful shutdown and `ConnectInfo` (P2)
- **Evidence:** `crates/nidus-http/src/server.rs:67-73` uses plain `axum::serve(listener, router)`
  with no `.with_graceful_shutdown(..)` and no
  `into_make_service_with_connect_info::<SocketAddr>()`.
- **Files:** `crates/nidus-http/src/server.rs`
- **Risk:** (a) On SIGTERM in-flight requests abort abruptly with no drain. (b) `client_ip_identity`
  (`context.rs:282-296`) prefers `ConnectInfo<SocketAddr>`, which is never populated via the blessed
  server path — so rate-limit identity falls through to spoofable `X-Forwarded-For` / `"anonymous"`.
- **Fix:** Add optional graceful-shutdown signal + `ConnectInfo` make-service (or document).
- **Verification:** `cargo test -p nidus-http --test server`; integration test for identity.

#### F-HTTP-6 — `client_ip_identity` trusts `X-Forwarded-For` and collapses anonymous to one bucket (P2)
- **Evidence:** `crates/nidus-http/src/context.rs:282-296` reads the first XFF hop with no
  trusted-proxy validation; falls back to a single `RequestIdentity::new("anonymous")`.
- **Files:** `crates/nidus-http/src/context.rs`
- **Risk:** Identity spoofing (evade/framed per-IP limits); one abuser exhausts the shared
  `"anonymous"` window for all anonymous clients.
- **Fix:** Trusted-proxy config; per-connection fallback instead of a shared bucket.
- **Verification:** unit tests for identity extraction branches.

#### F-HTTP-7 — No panic-catching layer in production stack (~~P2~~ mitigated, Wave 7)
- **Evidence:** the production stack now installs a nidus-native `CatchPanicLayer`
  (`crates/nidus-http/src/middleware/catch_panic.rs`) as the innermost layer in
  `ApiDefaults::production`. A handler panic is caught (`std::panic::catch_unwind` over
  `call` and `FutureExt::catch_unwind` over the inner future), logged via `tracing::error!`,
  and surfaced as a bare `500` that the outer `ErrorEnvelopeLayer` renders as the production
  envelope (with request id + metrics). The audit originally flagged
  `api_defaults.rs:260-289` as installing no `CatchPanicLayer`.
- **Files:** `crates/nidus-http/src/middleware/{api_defaults,catch_panic,security}.rs`,
  `crates/nidus-http/src/middleware.rs`, `crates/nidus-http/Cargo.toml`
- **Design notes:** a nidus-native layer is used (not `tower_http::catch_panic`) because the
  latter returns `Response<UnsyncBoxBody<..>>`, which does not compose with the
  `Response<Body>`-typed `ErrorEnvelopeLayer`. It is default-on in `ApiDefaults::production`
  with an opt-out `without_catch_panic()`. `futures-util` (already a workspace dep) added to
  nidus-http for `FutureExt::catch_unwind`.
- **Overhead (within-session A/B):** ~250ns / ~6% on the bare production stack
  (borderline, p=0.02, CI nearly touching zero); statistically undetectable on the
  production+metrics stack (p=0.43, where the metrics layer dominates). Acceptable for
  default-on panic safety; opt out with `without_catch_panic()` for the lowest latency.
- **Verification:** `cargo test -p nidus-http --test production_api
  production_defaults_envelope_panic_as_500` (panicking handler -> 500 envelope + request-id +
  metered); manual curl of production-api normal routes unaffected (health/users/limited 200).

#### F-HTTP-8 — Prometheus series count unbounded (P2)
- **Evidence:** per-series storage is fixed (`DurationHistogram` fixed 11-bucket array,
  `metrics.rs:331`), but `PrometheusState` maps (`:317-320`) have no max-series cap;
  `on_request`/`on_response` accept arbitrary route strings; test
  `prometheus_metrics_records_high_cardinality_routes_explicitly` exercises unbounded growth.
- **Files:** `crates/nidus-http/src/middleware/metrics.rs`
- **Risk:** A misconfigured hook or concrete-path labels cause unbounded memory growth in
  long-running processes (the recent "bound duration storage" commits bounded per-series size,
  not series count).
- **Fix:** Bound series (LRU/cap with `route="<overflow>"`), or enforce pattern-only labels.
- **Verification:** `cargo test -p nidus-http --test production_api`.

#### F-HTTP-9 — Legacy `request_id_layer` generates non-unique `nidus-<nanos>` ids (P3)
- **Evidence:** `crates/nidus-http/src/middleware/request_id.rs:78-85` (wall-clock nanos).
  Production path uses UUID v4. **Confirmed safe** otherwise (request id hardening verified).

### nidus-config

- **Clean.** No panics/unwrap in non-test code; `ConfigError` is path-aware and tested (21 tests).
- **C-1 (P3):** `get_path` navigates objects only — no array indices (`lib.rs:197-199`); untested.
- **C-2 (P3):** env-prefix matching is case-sensitive (`lib.rs:135`); not asserted in tests.
- Docs (`docs/config.md`, `docs/dependency-injection.md`) are **accurate** against the implementation.

### nidus-openapi

- **O-1 (~~P2~~ mitigated, Wave 9):** `OpenApiRoute::to_json_value` now derives error responses
  from route metadata — a guarded route advertises `401 Unauthorized` + `403 Forbidden`, and a
  validating route advertises `422 Validation failed` (description-only; no shared error schema).
  Plain routes (no guard/validate) are unchanged, so existing exact-match specs are unaffected.
  Clients can now discover the error statuses a route can return instead of only the success.
- **O-2 (P2):** No route↔spec parity test; the document is populated manually, so router/spec can
  silently diverge. **Verification:** integration test asserting each `RouteMetadata` appears in JSON.
- **O-3 (P3):** `cargo nidus openapi` inspector hardcodes title/version
  (`crates/cargo-nidus/src/openapi_doc.rs:102-105`), can diverge from runtime `OpenApiDocument`.
- Docs (`docs/openapi.md`) are **accurate**.

### nidus-validation

- **Clean.** No panics/unwrap in non-test code; 422 + sorted `fields` shape tested (9 tests).
- **V-1 (P3):** No test for malformed-JSON rejection (400) path, nor for the
  `ValidationPipeError` ↔ `ErrorEnvelopeLayer` composition (`fields` → `details`).
- Docs (`docs/pipes.md`) are **accurate**.

### nidus-auth

- **F-HTTP-1 (P1)** above (the `guard_layer` bug lives here).
- **A-1 (P2):** Guard runs after `inner.call` (request consumed before authorization)
  (`middleware.rs:86`). Fixed together with F-HTTP-1.
- **A-2 (P3):** `OrGuard` discards the second error when both fail (`lib.rs:99-102`) — intentional,
  tested.
- Note: there is **no `CurrentUser` extractor** in the codebase; auth state reaches handlers only
  generically via `GuardContext::state()`. (README/goal mention `CurrentUser` aspirationally.)

### nidus-events

- **E-1 (P2):** Subscriber queues are unbounded `Vec<T>` (`lib.rs:14,216,226`); no backpressure; a
  slow/absent drainer causes unbounded memory growth for the subscriber's lifetime.
- **E-2 (P3):** Observer runs synchronously on the publishing thread (`lib.rs:178-192`) —
  blocking-in-async risk if the observer does I/O.
- **E-3 (P3):** `lock_unpoisoned` silently absorbs poisoned-mutex state (`lib.rs:272-276`).
- Sync, in-process, no spawns/channels — runtime-safe otherwise.

### nidus-jobs

- **J-1 (P2):** `ObservedJobRunner` has **no `catch_unwind`** — a panicking job skips
  `on_job_finished`, violating the observer contract (`lib.rs:209,230`). The queues do recover
  (`lib.rs:346,404`, verified). No panic-recovery test for the runner.
- **J-2 (P2):** Queues are unbounded and **retain jobs after `run_all`** with no `clear`/`drain`
  (`lib.rs:286-288,321-326,342-359`); a second `run_all` re-executes everything (duplicate side
  effects). No test pins this behavior.
- **J-3 (P3):** No observer integration in `JobQueue`/`AsyncJobQueue` (telemetry vs orchestration
  are mutually exclusive).
- **J-4 (P3):** `ObservedJobRunner::run_async` holds a `!Send` tracing `Entered` across `.await`
  (`lib.rs:228-230`) — latent footgun if the future is ever spawned/boxed as `Send`.

### nidus-testing

- **Clean & ergonomic** overall (26 tests). In-memory `oneshot`, provider/config overrides, lifecycle.
- **T-1 (P2):** No way to install the production `request_scope_layer` on `TestApp`, so realistic
  `RequestScoped<T>` handlers can't be integration-tested without manual wiring
  (`app.rs:212-217` admits this).
- **T-2 (P3):** `assert_text`/`assert_json` are spuriously `async` (no `.await` in body,
  `response.rs:111,119`) — call sites must write `.await` (docs show it: `docs/testing.md:9`).

### cargo-nidus

- **All 10 documented subcommands implemented** and tested (60 tests); `cargo nidus new` template is
  verified to compile and serve `200 hello from nidus` by `tests/cli_new.rs`.
- **CLI-1 (P2):** No end-to-end `cargo check` test for `new` + all four `generate` artifacts
  (module/controller/service/repository) — the multi-artifact wiring is only file-asserted, not
  compile-verified (`tests/cli_generate.rs`).
- **CLI-2 (P2):** The default `nidus = "0.1"` dependency branch (`generate.rs:15-17`) is never
  exercised by any test (all pass `--nidus-path`); the published-user codepath is untested.
- **CLI-3 (P3):** Generated service name hardcodes `"hello-nidus"` regardless of project name
  (`generate.rs:38`).
- **CLI-4 (P3):** `expand` silently requires `cargo-expand` to be installed (`main.rs:136-141`).
- **CLI-5 (P3):** `graph` only scans `src/{main,lib,modules/*.rs}` — controllers/services outside
  `src/modules/` are invisible to `nidus graph` (`graph.rs:29-48`).

### nidus-sqlx / nidus-cache (adapters)

- **Clean boundaries.** Depend only on `nidus-core` (+ optional `nidus-http`/`nidus-config`/`moka`/
  `sqlx`); not pulled into the facade, as designed. No panics/unwrap in source.
- **AD-1 (P3):** Both implement `ProviderRegistrant` as a **no-op** (`nidus-sqlx/lib.rs:182-186`,
  `nidus-cache/lib.rs:229-233`) — registration is imperative via `Builder::register`. Misleading.
- **AD-2 (P3):** `health_status()` exists but is not wired into `HealthRegistry` (no bridge helper);
  untested in both adapters.
- **AD-3 (P2, coverage):** `nidus-sqlx` `health` feature and Postgres `from_config_path` untested;
  `nidus-cache` `invalidate()`/`from_cache()` untested.

## Example findings

| Example | Type | Default port | External svc | Notes |
| --- | --- | --- | --- | --- |
| `hello-world` | server | `127.0.0.1:3000` (hardcoded) | none | clean |
| `rest-api` | server | `127.0.0.1:3000` (hardcoded) | none | `.expect()` in startup helper (`main.rs:38-39`) |
| `auth-api` | server | `127.0.0.1:3000` (hardcoded) | none | guard is a toy route-label check, never reads a header (`main.rs:16-21`) |
| `openapi` | **CLI (prints + exits)** | — | none | **not a server** despite docs implying `/openapi.json`+`/docs` are served |
| `background-jobs` | CLI | — | none | clean |
| `modular-monolith` | CLI | — | none | 4× `.unwrap()` in `main()` (`main.rs:122,123,133,134`) |
| `realworld-api` | server | `127.0.0.1:3000` (`NIDUS_BIND_ADDR`) | none (sqlite::memory:) | `.expect()` in request handler path (`ops.rs:127,137,144,267,271`); deterministic |
| `production-api` | server | `127.0.0.1:3000` (`NIDUS_ADDR`) | none | package named `production-api` (not `nidus-example-*`); metadata drift |
| `sqlx-app` | CLI | — | none (sqlite::memory:) | clean |
| `cache-app` | CLI | — | none | clean |
| `integrations-production` | CLI | — | none for tests; `main()` needs `APP_DATABASE_URL`+`APP_CACHE_NAMESPACE` | clean |

- **EX-1 (P2):** `openapi` example is not a server — `docs/examples.md:11` says it provides
  `/openapi.json` and `/docs` routes; in fact `main()` (`openapi/src/main.rs:74-77`) only builds the
  router, `println!`s the JSON, and exits. A user running `cargo run -p nidus-example-openapi` cannot
  curl anything. (This is the clearest docs-vs-behavior drift.)
- **EX-2 (~~P2~~ mitigated, Wave 5):** `ApiKeyGuard` in the `auth-api` example now reads the
  `x-api-key` header and authorizes only on a match (`unauthorized` otherwise). It is wired
  through the public `guard_layer`, so this also serves as end-to-end coverage that the Wave-1
  header-passing fix works: integration tests assert valid key → 200, missing/wrong key → 401
  (6 tests). Manual curl on the running server: no key → 401, wrong key → 401, valid key →
  200 `authorized`.
- **EX-3 (P3):** `production-api` naming/metadata inconsistency (package `production-api`, workspace-
  inherited edition/license, no `version` pin on the `nidus` dep).
- **EX-4 (P3):** Orphaned empty dir `examples/sqlx-postgres/src/` — no `Cargo.toml`/`main.rs`, not a
  workspace member; leftover from the integrations migration. (Note: the `sqlx-postgres` package in
  `Cargo.lock` is sqlx's own transitive sub-crate, unrelated.)
- **EX-5 (P3):** `.expect()`/`.unwrap()` in non-test example `main`/startup paths (rest-api,
  modular-monolith, realworld-api config + handler).

No example fails to compile against the current API (build is green). No `TODO/FIXME/panic!` in
example/bench code.

## Docs consistency findings

- `docs/architecture.md`, `docs/dependency-injection.md`, `docs/testing.md`, `docs/config.md`,
  `docs/openapi.md`, `docs/pipes.md` are **accurate** against the implementation (verified
  symbol-by-symbol). Notably `docs/dependency-injection.md:84-95` correctly documents the
  `register_request_scoped` chaining requirement and the `RequestScopeRequired` error.
- **D-1 (P2):** `docs/examples.md:11,26` — implies `openapi` example serves `/openapi.json`+`/docs`
  and that "server examples ... keep running"; the `openapi` example does neither (see EX-1).
- **D-2 (P3):** `docs/deployment.md:77-90` lists `without_rate_limit()`/`without_metrics()` among
  "preset concerns" alongside on-by-default ones; rate limiting and metrics are actually opt-in
  (the `ApiDefaults::production` rustdoc at `api_defaults.rs:77-80` is accurate, so this is mild).

## Dependency boundary findings

- **Clean.** No circular crate deps. Adapters outside the facade. `tower-http` features are minimal.
  `deny.toml` licenses allow-list is tight; one acknowledged advisory (`RUSTSEC-2026-0173`,
  proc-macro-error2 via validator 0.20) is ignored with a documented reason.
- **DEP-1 (P3):** `clippy.toml` sets `avoid-breaking-exported-api = false` (good for pre-1.0 hygiene);
  no action.
- (To confirm at finalize: `cargo tree -d`, `cargo deny check`, `cargo audit`, `cargo machete` if
  available.)

## Public API ergonomics findings

- `Inject<T>`, `Optional<T>`, `Lazy<T>`, `Factory<T>`, `Scoped<T>` all exist and are documented.
- **API-1 (P3):** `Lazy<T>`/`Factory<T>` are not container-constructed (manual `::new` only) —
  README lists them alongside the auto-wired types without noting the distinction.
- **API-2 (P3):** Adapter `ProviderRegistrant` no-op impls are misleading (AD-1).
- **API-3 (P3):** `assert_text`/`assert_json` spurious `async` (T-2).

## Error handling & diagnostics findings

- `HttpError` + `ErrorEnvelopeLayer` production envelope is solid: 64 KiB body cap, 5xx masking,
  oversized-body skip, `requestId`/`path`/`timestamp`, all tested.
- `NidusError` covers DI/module/lifecycle cases with type names and preserved source errors.
- `ConfigError` is fully path-aware.
- **ERR-1 (~~P2~~ mitigated, Wave 6):** the production envelope now also masks the `code`
  field on a 5xx to the generic `internal_server_error` (previously `message`/`details` were
  masked but a handler-supplied `code` like `database_error` survived, leaking internal
  taxonomy). The original code is still emitted to the structured server log (`tracing::error!`
  in `envelope_response`) for debugging. The pinning test was strengthened from asserting the
  leak to asserting the mask.
- **ERR-2 (P3):** `register_openapi_schema` panics on serialization failure
  (`crates/nidus/src/lib.rs:40`) instead of returning a `Result`.

## Async/runtime safety findings

- **Mostly clean.** No `Mutex` held across `.await` in nidus-http layers; health `tokio::spawn`s are
  joined; no unbounded channels anywhere.
- **RT-1 (P1):** singleton panic-stuck (F-CORE-1) — a panic during lazy resolution deadlocks.
- **RT-2 (P2):** blocking `Condvar` waits reachable from async (F-CORE-4).
- **RT-3 (P2):** no graceful shutdown (F-HTTP-5).
- **RT-4 (P3):** `ObservedJobRunner::run_async` `!Send` future (J-4); event observer blocking risk (E-2).
- No hidden global mutable state (the `RESOLUTION_STACK` thread-local is correctly scoped by `Drop`).

## Test coverage gaps

- **TG-1 (P1):** no test for the singleton panic-stuck scenario (F-CORE-1).
- **TG-2 (P1):** no test resolves providers from a core `Nidus::bootstrap` app (F-CORE-2).
- **TG-3 (P1):** no test exercises a header-reading guard through `guard_layer` (F-HTTP-1) — the
  reason the bug slipped through.
- **TG-4 (P2):** no production middleware order-pinning test (F-HTTP-4).
- **TG-5 (P2):** no `ObservedJobRunner` panic-recovery test (J-1); no jobs-retained-on-rerun test (J-2).
- **TG-6 (P2):** no route↔OpenAPI parity test (O-2); no validation↔envelope composition test (V-1).
- **TG-7 (P2):** no end-to-end CLI multi-artifact compile test (CLI-1).
- **TG-8 (P3):** adapter `health`/`invalidate`/`from_cache`/Postgres-config untested (AD-3).

## Manual example coverage gaps

- No recorded manual `curl` evidence exists for any example (this audit pass will produce it).
- Servers that can run with zero external services: `hello-world`, `rest-api`, `auth-api`,
  `production-api`, `realworld-api` (sqlite::memory:). The `openapi` example is a CLI (gap EX-1).
- All server examples default to `127.0.0.1:3000`, so manual runs need distinct free ports.

## Benchmark / performance risks

- 3 Criterion benches (`routing`, `dependency_resolution`, `request_lifecycle`) are correctly
  registered (`harness = false`) and **compile against the current API** (no drift; every imported
  symbol verified present).
- `request_lifecycle.rs` is comprehensive (18 scenarios incl. individual middleware layers).
- **BENCH-1 (P3):** no assertion/baseline file locks bench numbers; "benchmark drift" is only
  guarded by manual review. (Criterion reports are non-deterministic by nature.)
- **BENCH-2 (P2):** any change touching F-CORE-1/F-CORE-4 (resolution path) or the HTTP middleware
  stack must re-run `dependency_resolution` / `request_lifecycle` per the optimization rules.

## Security / reliability risks

- **SEC-1 (P2):** body limit bypassable (F-HTTP-2) — DoS surface.
- **SEC-2 (~~P2~~ partially mitigated, Wave 4):** rate-limit identity now uses the real peer
  IP via `ConnectInfo` (F-HTTP-5 fix), closing XFF-spoofing and shared-`anonymous`-bucket
  evasion on the blessed `listen`/`serve` path. Trusted-proxy XFF validation (F-HTTP-6)
  remains deferred; XFF is now only consulted when `ConnectInfo` is absent.
- **SEC-3 (P2):** unbounded growth surfaces: prometheus series (F-HTTP-8), event queues (E-1),
  job queues (J-2).
- **SEC-4 (P1):** `guard_layer` silently breaks header auth (F-HTTP-1) — authorization bypass risk
  for header-token guards.
- **SEC-5 (P3):** example dev secret `dev-secret` (realworld) — documented + overridable, acceptable.
- No leaked secrets in logs/errors (5xx message masking verified); no `unsafe` in framework crates.

## Prioritized backlog

| ID | Sev | Finding | Key evidence |
| --- | --- | --- | --- |
| F-HTTP-1 | **P1** | `guard_layer` never passes headers; authorizes after inner.call | `nidus-auth/src/middleware.rs:86-89` |
| F-CORE-1 | **P1** | Panicking singleton factory permanently deadlocks provider | `nidus-core/src/provider/mod.rs:136-154` |
| F-CORE-2 | **P1** | Core `Nidus::bootstrap` yields empty container | `nidus-core/src/app/mod.rs:56-69` vs `nidus/src/app.rs:99-109` |
| F-CORE-3 | P2 | Graph keyed by short name, not TypeId | `nidus-core/src/module/mod.rs:230-236,271-277` |
| F-CORE-4 | P2 | Blocking Condvar reachable from async | `nidus-core/src/provider/mod.rs:134` |
| F-CORE-5 | P2 | `register_request` can't chain request deps | `nidus-core/src/container/mod.rs:84-90` |
| F-HTTP-2 | P2 | Body limit Content-Length-only (bypassable) | `nidus-http/src/middleware/security.rs:171-185` |
| F-HTTP-3 | ~~P2~~ mitigated | 413 now enveloped/metered/request-id'd (Wave 8) | `nidus-http/src/middleware/api_defaults.rs` |
| F-HTTP-4 | P2 | No production middleware order test | `nidus-http/tests/production_api.rs` |
| F-HTTP-5 | ~~P2~~ mitigated | ConnectInfo now on blessed path; graceful-shutdown API added (Wave 4) | `nidus-http/src/server.rs` |
| F-HTTP-6 | P2 | client_ip_identity spoofing + anonymous bucket | `nidus-http/src/context.rs:282-296` |
| F-HTTP-7 | ~~P2~~ mitigated | Production stack catches handler panics (Wave 7) | `nidus-http/src/middleware/{api_defaults,catch_panic}.rs` |
| F-HTTP-8 | P2 | Prometheus series unbounded | `nidus-http/src/middleware/metrics.rs:317-320` |
| F-MAC-1 | P2 | Controller non-injectable fields → runtime error | `nidus-macros/src/controller.rs:42-67` |
| J-1 | P2 | `ObservedJobRunner` no panic recovery | `nidus-jobs/src/lib.rs:209,230` |
| J-2 | P2 | Job queues retain jobs; rerun duplicates | `nidus-jobs/src/lib.rs:286-288,342-359` |
| E-1 | P2 | Event subscriber queues unbounded | `nidus-events/src/lib.rs:14,216,226` |
| O-1 | ~~P2~~ mitigated | OpenAPI emits error responses (Wave 9) | `nidus-openapi/src/route.rs` |
| O-2 | P2 | No route↔OpenAPI parity test | `nidus-openapi/tests/` |
| EX-1 | P2 | `openapi` example not a server; docs imply it is | `examples/openapi/src/main.rs:74-77`; `docs/examples.md:11` |
| EX-2 | ~~P2~~ mitigated | `auth-api` guard now reads `x-api-key` header (Wave 5) | `examples/auth-api/src/main.rs` |
| CLI-1 | P2 | No end-to-end multi-artifact CLI compile test | `cargo-nidus/tests/cli_generate.rs` |
| CLI-2 | P2 | Default `nidus="0.1"` branch untested | `cargo-nidus/src/generate.rs:15-17` |
| ERR-1 | ~~P2~~ mitigated | 5xx `code` now masked to generic value (Wave 6) | `nidus-http/src/error.rs` |
| AD-3 | P2 | Adapter health/postgres-config/invalidate untested | `nidus-sqlx`, `nidus-cache` tests/ |
| T-1 | P2 | TestApp can't install request_scope_layer | `nidus-testing/src/app.rs:212-217` |
| (many) | P3 | diagnostics spans, naming, async assertions, cleanup, etc. | see sections above |

## Follow-up hardening — second pass (2026-06-26, after commit `ac108ef`)

Waves 1-2 (the three P1 fixes + cheap P2 hardening from the plan) landed in the
prior session. A second evidence-backed pass advanced the deferred backlog.
Baseline before this pass: build green, `cargo test --workspace --all-features`
345 passed / 0 failed; fmt/clippy/doc/deny/audit/machete/tree all clean.

### Implemented (TDD, atomic commits)

- **F-HTTP-8 / SEC-3 mitigated** (commit `66834f7`): `PrometheusMetrics::with_max_series(n)`
  bounds distinct route labels; once `n` are admitted, further labels collapse into a
  single `"<overflow>"` route, preventing unbounded memory growth from accidental
  high-cardinality labels. The default `new()` path is unchanged and zero-overhead
  (`admit_route` is guarded behind `max_series.is_some()`). Two tests pin both paths.
  Benchmark: all three metrics criterion scenarios report *"No change in performance
  detected"* (p > 0.05). Default-cardinality change deferred because existing tests
  deliberately pin "records every distinct route" as intended behavior and the caller
  already controls cardinality via route patterns.
- **E-1 / SEC-3 mitigated** (commit `dcfbf0a`): `EventBus::subscribe_with_capacity(cap)`
  returns a bounded subscriber that evicts the oldest event past the cap, so a slow/absent
  drainer can never grow memory without limit. Queue type moved to a `SubscriberBuffer`
  carrying an optional capacity; default `subscribe()` remains unbounded. Two tests pin
  drop-oldest and keep-all behaviors.
- **O-2 covered** (commit `3070c07`): parity tests assert `from_route_metadata` and
  `from_controller_routes` emit exactly the declared paths and methods (no missing, no
  extra, every operation has an `operationId`), so the generated spec and the router
  built from the same `RouteMetadata` cannot silently diverge.
- **V-1 covered** (commit `3070c07`): a malformed-JSON body is pinned to 400 (Axum
  `JsonRejection`), distinct from the 422 business-rule path; a new workspace integration
  test pins the `ValidatedJson` 422 → `ErrorEnvelopeLayer` composition (envelope preserves
  `code`/`message` and flattens `fields` into `details` while adding `statusCode`/
  `timestamp`/`path`). `serde_json` added as a test-only workspace dev-dependency.

### Investigated and intentionally NOT changed (with evidence)

- **F-MAC-1 reclassified → not a defect.** A blanket compile error for non-`Inject`/
  `Optional` controller fields was implemented and TDD-tested, then **reverted** because it
  regresses two legitimate, tested patterns: `crates/nidus/tests/controller_routes.rs`
  (a controller with a concrete `suffix: &'static str` field, constructed manually via
  `into_router()`) and `crates/nidus/tests/ui/routes_generic_controller.rs` (a generic
  controller `service: S`). The runtime `NidusError::ApplicationBuild` is **intentional**:
  it supports manually-constructed controllers that are never built from the container.
  Refined scoping (compile-error only on concrete fields, runtime-error for generic params)
  still breaks the `&'static str` case. F-MAC-1 is therefore **not a defect** and is removed
  from the backlog.

### Verification after this pass

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D
warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps` all
clean; `cargo test --workspace --all-features` → **354 passed / 0 failed / 30 ignored**
(+9 tests vs baseline); `cargo deny check`, `cargo machete`, `cargo tree -d` clean; the one
`cargo audit` warning (RUSTSEC-2026-0173, `proc-macro-error2` via `validator` 0.20) remains
the pre-existing, documented/ignored advisory.

Manual curl (production-api on `127.0.0.1:64752`, since `metrics.rs` was touched):
`GET /health/live` → 200; `GET /metrics` → 200 `text/plain` rendering
`nidus_http_requests_total{method="GET",route="/users/{id}",status="200"}` with
`route="/metrics"` excluded (count 0); `GET /users/1` → 200 with UUID `x-request-id` and
matching `request_id` in body. No live regression; opt-in cap does not affect the default
uncapped render path.

## Follow-up hardening — Wave 4 (2026-06-26, after commit `3070c07`)

Waves 1-3 landed in prior sessions. Wave 4 closed the production server-path
gap (F-HTTP-5) and the largest part of SEC-2. Baseline before this pass:
build green, `cargo test --workspace --all-features` 354 passed / 0 failed;
fmt/clippy/doc clean.

### Implemented (TDD, atomic commits)

- **F-HTTP-5 mitigated — ConnectInfo on the blessed serve path**
  (`crates/nidus-http/src/server.rs`). Every serving method
  (`listen`, `serve`, and new `listen_with_graceful_shutdown` /
  `serve_with_graceful_shutdown`) now wraps the router with
  `into_make_service_with_connect_info::<SocketAddr>()`, so
  `axum::extract::ConnectInfo<SocketAddr>` is populated for every connection.
  This is the correctness fix the audit flagged: `client_ip_identity`
  (`crates/nidus-http/src/context.rs:282-296`) prefers `ConnectInfo` but the old
  `listen` used plain `axum::serve(listener, self.router)`, so it was **never**
  populated and identity fell through to the spoofable `X-Forwarded-For` header
  or a shared `"anonymous"` bucket.
  - `serve(listener)` and `serve_with_graceful_shutdown(listener, signal)` are
    new public methods (pre-bound listener) — useful for reading the assigned
    port / `SO_REUSEPORT`, and the seam the new tests target.
  - `listen` keeps its public signature (`listen<A: ToSocketAddrs>(self, A)`);
    no public API break. `#[nidus::main]` examples that call `.listen(addr)` are
    unchanged and still build.
  - **Design choice (matches axum):** graceful shutdown is **not** auto-wired on
    `listen`/`serve` (axum's own `axum::serve` also leaves it to the caller).
    The explicit `*_with_graceful_shutdown(signal)` methods provide in-flight
    draining on a caller-supplied signal (SIGTERM/ctrl_c in production); this
    needs no new tokio feature. Resolves the audit's proposed fix
    ("Add optional graceful-shutdown signal + ConnectInfo make-service").
  - **TDD:** test `serve_populates_connect_info_for_peer_identity` was written
    first; verified RED for the exact expected reason
    (`Missing request extension: ConnectInfo<SocketAddr>`) against a
    no-ConnectInfo `serve`, then GREEN after the fix. Test
    `serve_with_graceful_shutdown_drains_and_exits_cleanly` proves a controlled
    shutdown signal drains and the serve future completes cleanly (no hang).
- **SEC-2 mitigated (rate-limit identity):** because `ConnectInfo` is now
  populated, `client_ip_identity` classifies by the **real peer IP** instead of
  trusting spoofable `X-Forwarded-For` or collapsing to `"anonymous"`. The
  shared-anonymous-bucket evasion and XFF-spoofing evasion are closed on the
  blessed server path. (Trusted-proxy validation of XFF, F-HTTP-6, remains
  deferred — but XFF is now only consulted when `ConnectInfo` is absent, i.e.
  not via the blessed `listen`/`serve`.)

### Manual curl evidence (Wave 4)

All server examples started on free ports, real routes curled (read from
source), then stopped cleanly (no background servers left; `lsof` confirmed
ports clear):

| Example | Route(s) | Result |
| --- | --- | --- |
| `hello-world` | `GET /` | 200 `hello from nidus` |
| `rest-api` | `GET /users/42` | 200 `{"id":42,"email":"user@nidus.dev","request_id":0}` |
| `auth-api` | `GET /me` | 200 `authorized` (guard passes route label) |
| `openapi` | `GET /openapi.json`; `GET /docs`; `GET /users/7`; `POST /users` | 200 / 200 (`<title>Nidus Example API Documentation</title>`) / 200 / 201 |
| `production-api` | `GET /health/live`; `/health/ready`; `/users/1`; `/metrics` | 200 / 200 / 200 (UUID `x-request-id`) / 200 (route labels present) |
| `production-api` (Wave 4) | `GET /limited` #1; `GET /limited` #2 with `X-Forwarded-For: 1.2.3.4` | **200 then 429** — 429 on #2 proves the real peer IP (via ConnectInfo) overrides the spoofed XFF, so the `client_ip_identity` limiter can no longer be evaded |
| `realworld-api` | `GET /health`; `GET /projects/1` no key; `POST /users`; `POST /projects owner_id:1`; `GET /projects/1`; `/metrics`; `/openapi.json` | 200 / 401 `missing or invalid x-api-key` / 201 / 201 / 200 / 200 (route labels) / 200 |

### Verification after this pass

`cargo fmt --all --check`, `cargo clippy -p nidus-http --all-targets --all-features
-- -D warnings`, `cargo test -p nidus-http` all clean (+3 tests in the `server`
suite: 357 expected workspace-wide). `cargo build` of all six server examples
succeeds against the updated `listen`. Benchmark decision: **not required** —
the change is on the connection/serve boundary (`into_make_service`), not a
measured request-routing/DI hot path; the per-request middleware stack is
unchanged. (`routing`/`dependency_resolution`/`request_lifecycle` bench source
is untouched.)

## Follow-up hardening — Wave 5 (2026-06-26, after commit `5d714d6`)

A small, example-only, high-confidence pass: the `auth-api` example was the last
example whose guard did not exercise a real authorization signal.

### Implemented

- **EX-2 mitigated — realistic header guard in `auth-api`** (`examples/auth-api/src/main.rs`).
  `ApiKeyGuard` now reads the `x-api-key` header (constant `EXPECTED_API_KEY =
  "nidus-dev-secret"`) and returns `GuardError::unauthorized` on missing/wrong key. Because it
  is wired through the public `guard_layer`, this is also end-to-end coverage that the Wave-1
  header-passing fix (F-HTTP-1) works: 6 tests (valid → 200, missing → 401, wrong → 401, both
  at the guard unit level and the router integration level). Manual curl on the running
  server confirmed the same (no key → 401, wrong key → 401, valid key → 200 `authorized`).

### Verification after this pass

`cargo fmt -p nidus-example-auth-api -- --check`, `cargo clippy -p nidus-example-auth-api
--all-targets -- -D warnings`, `cargo test -p nidus-example-auth-api` (6 passed) all clean.
Example-only change; no crate hot path touched, so no bench required. Full workspace gate run
at finalize.

## Follow-up hardening — Wave 6 (2026-06-26, after commit `c481569`)

A contained security/consistency pass on the production error envelope.

### Implemented

- **ERR-1 mitigated — mask 5xx `code`** (`crates/nidus-http/src/error.rs`). The envelope already
  masked `message`/`details` on a 5xx but left the handler-supplied `code` (e.g.
  `database_error`) intact, leaking internal taxonomy to clients. `envelope_response` now also
  resets `code` to the generic `internal_server_error` on a 5xx. The original code is still
  written to the structured server log (`tracing::error!`, runs before the reset) so debugging
  is unaffected. Default 5xx (no legacy body) already resolved to `internal_server_error`, so
  the only observable change is for handlers that returned a custom-coded 5xx legacy body.
  - **TDD:** the existing `error_envelope_masks_5xx_legacy_error_details` test was renamed and
    strengthened to assert `code == "internal_server_error"`; verified RED (`code` leaked
    `database_error`) then GREEN.
- **Bench decision:** the changed branch (5xx masking) is provably off every measured
  `request_lifecycle` path — the envelope success scenario short-circuits at `error.rs:235`
  before `envelope_response` runs, so the success-path code is byte-identical before/after.
  Re-running the success-envelope bench confirmed noise: it oscillated +10% then −6% across two
  runs (true ~1.05 µs), i.e. ±~8% run-to-run variance, not a real effect. (No baseline lock
  exists — BENCH-1.)

### Verification after this pass

`cargo fmt --all --check`, `cargo clippy -p nidus-http --all-targets --all-features -- -D
warnings`, `cargo test --workspace --all-features` (358 passed / 0 failed) all clean.

## Follow-up hardening — Wave 7 (2026-06-26, after commit `803ba1a`)

Closed the production-stack reliability gap F-HTTP-7: a panicking handler now yields the
production envelope instead of aborting the connection.

### Implemented (TDD, atomic commits)

- **F-HTTP-7 mitigated — panic-catching in the production stack.** A new nidus-native
  `CatchPanicLayer`/`CatchPanicService` (`crates/nidus-http/src/middleware/catch_panic.rs`)
  preserves `Response<Body>` (unlike `tower_http::catch_panic`, whose `Response<UnsyncBoxBody>`
  does not compose with `ErrorEnvelopeLayer`). Installed as the innermost layer in
  `ApiDefaults::production` (default-on, opt-out via `without_catch_panic()`). A handler panic is
  caught, logged, and returned as a bare `500` that the outer envelope renders with request-id +
  metrics. `futures-util` (already a workspace dep) added to nidus-http.
  - **TDD:** `production_defaults_envelope_panic_as_500` verified RED (panic propagated:
    `handler panicked`) then GREEN (500 envelope + request-id + metered).
  - **Bench (within-session A/B against a saved baseline):** catch_panic adds ~250ns / ~6% on the
    bare production stack (borderline p=0.02, CI nearly touching zero) and is statistically
    undetectable on the production+metrics stack (p=0.43). Earlier cross-session comparisons
    showing +95%/+20% were noise (the machine showed ~40% run-to-run swing independent of the
    change).
  - **Manual curl:** production-api normal routes unaffected (health/live, /users/5, /limited all
    200; metrics route labels intact).

### Verification after this pass

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps`,
`cargo test --workspace --all-features` (359 passed / 0 failed), `cargo tree -d` (no dups),
`cargo deny check` (all ok), `cargo machete` (no unused), `cargo audit` (only the pre-existing
documented advisory RUSTSEC-2026-0173) — all clean.

## Follow-up hardening — Wave 8 (2026-06-27, after commit `ef42feb`)

Closed the production-observability gap F-HTTP-3: oversized-body `413` responses are now
enveloped, metered, and carry a request id.

### Implemented (TDD, atomic commits)

- **F-HTTP-3 mitigated — 413 observability.** `body_limit_layer` moved from the outermost
  functional position to inside `validated_request_id` / `metrics` / `ErrorEnvelope` in
  `ApiDefaults::apply`. An oversized-body `413` now flows out through the envelope (enveloped),
  metrics (metered), and the request-id layer (carries `x-request-id`) — consistent with how
  `408` timeouts are observed. Order-only change (still 9 layers; no per-request cost added).
  - **TDD:** `production_defaults_envelope_and_meter_body_limit_rejections` verified RED (`413 must
    carry a request id`) then GREEN (413 → JSON envelope `statusCode:413` + `x-request-id` + metered).
  - **Bench:** `request_lifecycle` production scenarios show no regression (~3.8 µs bare,
    p=0.12; ~4.45 µs with metrics) — expected, since the change reorders layers without
    adding/removing any.
  - **Manual curl:** production-api with a 2 MB body → `413 Payload Too Large`,
    `content-type: application/json`, `x-request-id: <uuid>`, `x-content-type-options: nosniff`,
    body `{"error":{"statusCode":413,"code":"http_error","message":"Payload Too Large",...,
    "requestId":"<uuid>"}}`; `/metrics` records the 413.

### Verification after this pass

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps`,
`cargo test --workspace --all-features` (360 passed / 0 failed) — all clean.

## Follow-up hardening — Wave 9 (2026-06-27, after commit `8341384`)

Diversified into `nidus-openapi`: closed O-1 so OpenAPI specs advertise the error
statuses a route can actually return.

### Implemented (TDD, atomic commits)

- **O-1 mitigated — OpenAPI error responses.** `OpenApiRoute::to_json_value`
  (`crates/nidus-openapi/src/route.rs`) now derives error responses from route metadata:
  guarded routes advertise `401 Unauthorized` + `403 Forbidden`; validating routes
  advertise `422 Validation failed` (description-only — a shared error-envelope schema is
  deferred). Plain routes (no guard/validate) are unchanged, so the existing exact-match
  specs (plain GET/POST) and all key-based assertions are unaffected.
  - **TDD:** `openapi_document_emits_error_responses_for_guarded_validating_routes` (RED: no
    `401`) + `openapi_document_omits_error_responses_for_plain_routes` (pins no-change for
    plain routes) → GREEN.
  - **Manual curl:** realworld-api `POST /projects` (guarded + validating) now serves a spec
    with `201`/`401`/`403`/`422` responses.
  - **Bench:** not required — OpenAPI document generation is startup/build-time, not a
    per-request hot path.

### Verification after this pass

`cargo fmt --all --check`, `cargo clippy -p nidus-openapi --all-targets --all-features -- -D
warnings`, `cargo test --workspace --all-features` (362 passed / 0 failed) — all clean.

## Appendix: verification commands (baseline)

```
cargo build --workspace --all-features
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings   # to run at finalize
cargo fmt --all --check                                                # to run at finalize
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps  # at finalize
cargo tree -d                                                          # at finalize
```

Result at audit time: build green; all tests pass (0 failures); ~260 tests.
