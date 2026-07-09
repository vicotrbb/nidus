# Changelog

## Unreleased

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
