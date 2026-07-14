# Changelog

## Unreleased

- Shared immutable guard route labels across guard contexts, layers, services,
  and generated module routes instead of cloning owned labels per request. The
  existing guarded-route benchmark improved by 18.33%-22.87% in the immediate
  comparison, with a repeated run also classed as an improvement.
- Removed one redundant header-map clone from every macro-generated guarded
  request, added explicit direct-versus-module guard documentation, and added a
  two-guard runtime regression test that preserves ordered header enforcement.
- Deferred error-envelope path and request-ID string creation until a response
  is known to be a 4xx/5xx, improving the measured successful-response path by
  4.98%-7.21% without changing the envelope contract.
- Replaced the panic-catching middleware's per-request boxed future with a
  concrete future composition, improving its measured non-panicking path by
  6.66%-8.19% while preserving synchronous-call and future-poll panic handling.
- Added isolated panic-middleware benchmark coverage and a regression test for
  synchronous `Service::call` panics.
- Interned in-process Prometheus route labels and retained HTTP methods as
  typed keys, eliminating fresh label-string allocations on every recorded
  request and response while preserving exposition output and series caps.
- Made lifecycle tracing async-safe so a startup or shutdown span is active
  only while its future is being polled, preventing unrelated work on the same
  executor thread from inheriting the lifecycle span.
- Added focused allocation-reuse and tracing-scope regression coverage.

## 1.0.11 - 2026-07-13

- Built OpenAPI paths and operation IDs directly into pre-sized strings instead
  of allocating temporary segment vectors and strings. A repeated local
  100-route document-build benchmark improved by 18.40%-20.56% while focused
  path and operation-ID behavior tests remained unchanged.
- Made application shutdown attempt every lifecycle hook in reverse order even
  after a hook fails, while preserving the existing `Result` API by returning
  the first error after all cleanup callbacks have run.
- Added focused lifecycle regression coverage, OpenAPI construction benchmark
  coverage, and documentation for the measured performance and shutdown
  semantics.

## 1.0.10 - 2026-07-11

- Added separately installable Redis, Kafka, NATS/JetStream, RabbitMQ, SQS,
  OpenTelemetry, Sentry, shared-integration, and SQLx durable-job crates while
  keeping the `nidus-rs` facade dependency surface unchanged.
- Added SQLx MySQL and explicitly tested CockroachDB support with verify-full
  TLS and bounded SQLSTATE `40001` transaction retries that exclude ambiguous
  results and external side effects.
- Added an at-least-once durable job runtime with scheduling, idempotent
  enqueue, attempt-fenced multi-worker leases, indexed ready/recovery/DLQ
  paths, heartbeats, full-jitter retries, crash recovery, cancellation,
  graceful draining, and dead-letter inspection.
- Added bounded lifecycle admission and shutdown for adapter-owned operations,
  parsed-host enforcement for local plaintext escape hatches, redacted durable
  diagnostics, and panic isolation for custom telemetry observers.
- Added a pinned real-service suite with container, network, volume, schema,
  broker-resource, temporary-file, port, and worktree cleanup proof, plus an
  isolated feature matrix and integration hot-path benchmarks.

## 1.0.9 - 2026-07-10

- Normalized controller mount prefixes once and converted route parameters into
  one pre-sized output buffer instead of allocating per segment and
  re-normalizing every stored route. Local Criterion measurements improved
  32-route controller construction by 34.1%-35.8%; existing routing behavior
  and focused path-invariant tests remain green.
- Registered OpenAPI schemas directly into the document's owned map instead of
  cloning every previously registered schema for each addition. A new
  64-schema construction benchmark improved by 92.0%-92.1% locally, with a
  regression test preserving first-registration-wins behavior for duplicate
  schema names.
- Rejected and reverted a request-context refresh experiment after repeated
  composed-stack measurements ranged from a small improvement to no change and
  a metrics-stack regression.

## 1.0.8 - 2026-07-10

- Mounted controller routes directly into their destination router instead of
  constructing and merging a temporary router per route. Local Criterion
  measurements improved 32-route controller construction by 55.1%-56.2%; a
  focused regression test preserves same-path, different-method routing.
- Made `RequestContext` clones share immutable string metadata with
  copy-on-write enrichment while preserving all public methods and their
  behavior. Clone time improved by 96.0%-96.1% locally, and composed request
  stack benchmarks improved by 3.8%-17.2% across the measured configurations.
- Pre-rendered immutable OpenAPI JSON and Swagger UI responses at router
  construction time. A 100-route JSON response improved from 188.42-190.58 us
  to 519.09-520.22 ns locally while preserving the response media types and
  parsed document content.
- Added focused Criterion rows for multi-route controller construction,
  request-context cloning, and OpenAPI document serving, plus copy-on-write,
  routing-composition, and content-type regression coverage.

## 1.0.7 - 2026-07-09

- Replaced front-draining `Vec` storage for bounded event subscribers with a
  ring buffer while retaining the existing `Vec` fast path for unbounded
  subscribers. At a full 10,000-event bound, local Criterion publication time
  moved from 1.129 us to 72.47 ns (94.3% lower mean estimate).
- Consolidated W3C `traceparent` validation across request context,
  OpenTelemetry helpers, and structured logging; request context now populates
  the parent span ID, version `ff` and malformed version-00 extensions are
  rejected, and future-version extensions are ignored. Structured spans now
  borrow request, trace, and route fields instead of allocating owned strings
  (159.2 ns to 84.99 ns locally; 40.0% lower mean estimate).
- Removed one boxed-future allocation per request from the security-header,
  declared body-limit, and timeout-response middleware. In repeated local
  Criterion comparisons, the isolated middleware rows improved by 29% to 53%;
  the final composed production-default sample was too noisy to establish a
  statistically significant change.
- Fixed request-scoped provider resolution after a factory panic: the
  in-progress cache entry is now cleared and waiters are notified, matching
  singleton-provider recovery instead of leaving the scope permanently stuck.
- Updated the transitive `crossbeam-epoch` dependency used by the cache adapter
  and benchmark tooling from 0.9.18 to 0.9.20 to address RUSTSEC-2026-0204.
- Hardened standalone-example release verification against occupied ports and
  orphaned `cargo run` children; the verifier now owns the real server PIDs and
  prints captured server logs when a runtime smoke fails.

## 1.0.6 - 2026-07-04

- Sped up dependency resolution: constructed singletons resolve through a
  lock-free cache and provider maps hash `TypeId` keys with an identity
  hasher (singleton resolution 11.78 ns -> 3.96 ns locally).
- Cut per-request allocations in the validated request ID middleware, request
  context construction, and the error envelope layer (production default
  stack 2.109 us -> 1.962 us locally).
- Merged the in-process Prometheus collector's per-status maps into one
  series map (record response 126.7 ns -> 81.3 ns) and rewrote text
  exposition to write directly into the output (render 30.1 us -> 7.2 us),
  with unchanged output.
- Amortized in-memory rate limit pruning to at most one sweep per window
  (check with 10k tracked identities 27.5 us -> 32 ns) and switched rate
  limit headers to infallible integer conversions.
- Fixed the guard service to honor the Tower readiness contract by moving
  the polled-ready inner service into the response future.
- Added Criterion rows for the rate limit layer and a 10k-identity store
  check, plus focused tests for singleton reuse/retry, `TypeId` hashing, and
  Prometheus bucket label sync.

## 1.0.5 - 2026-07-02

- Added the optional `nidus-dashboard` crate and facade `dashboard` feature for
  an embedded, protected runtime cockpit.
- Added Home, Atlas, Routes, Timeline, Adapters, and Settings dashboard
  surfaces, with Events and Jobs consolidated into Timeline filters while their
  APIs remain available.
- Added branded dashboard logo and favicon asset routes behind dashboard auth.
- Hardened dashboard UI mode rendering, operation timing copy, keyboard state,
  active navigation state, and mobile or desktop layout proof.
- Refreshed docs, examples, website, generated starter defaults, and version
  references for the 1.0.5 release.

## 1.0.4 - 2026-06-29

- Updated `anyhow` to 1.0.103 to address RUSTSEC-2026-0190.
- Pinned all GitHub Actions workflow actions to full commit SHAs.
- Expanded the security policy with direct private reporting links, fallback
  email, response timelines, disclosure guidance, and scope.
- Added cargo-fuzz integration and CI fuzz target compilation.
- Refreshed docs, examples, website, generated starter defaults, and version
  references for the 1.0.4 patch release.

## 1.0.3 - 2026-06-29

- Added facade builder router ergonomics with `Nidus::create::<AppModule>().with_router(router)` and `build_with_router(router)`.
- Added a first-class Nidus unmatched-route fallback and wired production defaults to return the JSON `not_found` envelope for missing routes.
- Added `GuardContext` helpers for UTF-8 headers, bearer tokens, and API-key headers.
- Added channel-backed job observation with structured `ObservedJobEvent` values plus small event/job observability factory helpers.
- Improved the `cargo nidus new` starter with a `src/lib.rs` and `src/main.rs` split, generated HTTP tests, readiness checks, and cleaner production defaults.
- Refreshed docs, examples, and version references for the 1.0.3 patch release.

## 1.0.2 - 2026-06-29

- Public website refresh for standalone Nidus positioning, docs-first navigation, and 1.0.2 launch evaluation.
- GitHub Pages build alignment for the `rustnidus.com` custom-domain root while preserving project-base verification.
- Documentation expansion for installation, CLI, concepts, runtime surfaces, production boundaries, official adapters, examples, API reference, and release proof.
- Version alignment across package metadata, snippets, generated starter defaults, examples, and website docs.

## 1.0.1 - 2026-06-29

- 1.0.1 release hygiene and package proof improvements.
- Public documentation updates for post-1.0 installation, crate selection, starter workflow, and supported versions.
- Starter project improvements for a small module, controller, and service learning path.
- CI package verification alignment with the publish workflow.

## 1.0.0

- Initial stable release of the Nidus workspace crate set.
- Public facade crate `nidus-rs` with explicit feature groups for HTTP, config, OpenAPI, validation, auth, events, jobs, observability, and testing.
- Core dependency injection, module graph, lifecycle, provider lifetimes, request scope, and controller registration.
- HTTP integration over Axum and Tower with production defaults for request IDs, context, health, metrics, CORS, body limits, timeouts, security headers, logging, and error envelopes.
- CLI project generation and source inspection commands.
- Official SQLx and cache adapter crates kept separate from the facade.
- Workspace examples for hello world, REST, auth, OpenAPI, production defaults, background jobs, modular monolith, SQLx, cache, integrations, and production-shaped APIs.
- Replaced the validation derive dependency with `garde`, removing the `proc-macro-error2` RustSec advisory path without suppressing the advisory.
