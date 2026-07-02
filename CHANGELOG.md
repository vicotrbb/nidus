# Changelog

## Unreleased

- No unreleased changes yet.

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
