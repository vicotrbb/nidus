# Release 1.0.17

Nidus 1.0.17 is a public-API-compatible correctness and dependency-policy patch
containing every change on `main` since 1.0.16.

## Application bootstrap correctness

- The README quickstart now builds its module-owned controller exactly once.
  This removes an overlapping `GET /users/{id}` route panic without weakening
  the framework's duplicate-route detection.
- Focused regression coverage builds the documented application and exercises
  the route through the resulting router.

## Precise CLI graph discovery

- `cargo nidus graph` no longer treats ordinary public structs as modules when
  a source file contains no module metadata.
- Macro-declared modules and explicit `ModuleBuilder` definitions remain
  source-driven and deterministically ordered. Typed builder calls for imports,
  providers, controllers, and exports are included in the reported metadata.
- Malformed or unknown `#[module]` metadata now fails with the relevant source
  path and parser diagnostic rather than silently becoming an empty module.
- The real-world example graph contains only `AppModule`, `AuthModule`,
  `DatabaseModule`, `ProjectsModule`, and `UsersModule` with their actual
  imports, providers, controllers, and exports.

## Dependency-policy health

- `event-listener` is updated from 5.4.1 to 5.4.2, resolving
  RUSTSEC-2026-0221 across the SQLx, Moka, Redis, and Lapin dependency paths.
- Direct Sentry dependencies are pinned to the same 0.48.5 release, and the
  dependency-policy check requires all resolved `sentry` and `sentry-*` crates
  to form one exact version cohort.
- Dependabot now proposes Sentry-family updates independently from the broad
  Rust dependency group, preventing an unrelated update set from obscuring a
  Sentry compatibility failure.
- A weekly CI schedule runs the dependency-policy job against `main` without
  unnecessarily scheduling Rust validation, live integrations, or the website
  job. The existing narrow SQLx MySQL RSA advisory exception is unchanged.

The release candidate is checked with workspace formatting, warnings-denied
Clippy, all-feature tests and doctests, isolated locked feature combinations,
rustdoc warnings as errors, dependency and RustSec policies, semver checks for
every publishable library, package file-list preflights, website verification,
and standalone external examples.

After publication, verify all 25 registry artifacts, docs.rs pages, and the two
standalone external examples against crates.io with:

```bash
bash scripts/verify-published-release.sh 1.0.17
```
