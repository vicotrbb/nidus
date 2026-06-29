# Changelog

## Unreleased

- No unreleased changes yet.

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
