# Nidus 1.2.0 release validation

The coordinated 1.2.0 minor release publishes all 25 framework packages from
commit `005e06a3fc504fc4cf97d98f88fb5c6be2f12de2`, identified by the annotated
`v1.2.0` tag. The source includes all remote main history through
[PR #41](https://github.com/vicotrbb/nidus/pull/41).

## Compatibility and pre-publication checks

The release retains the existing Provider and Pipe traits, facade prelude
exports, HTTP response module, concrete middleware types, and HTTP route error
definition. Shared route helpers preserve HTTP, OpenAPI, and CLI behavior.

The final source passed:

- Formatting, strict Clippy, rustdoc, and integration feature-matrix checks.
- 647 workspace tests and doctests, with zero failures. The 23 ignored cases
  include illustrative macro doctests and service tests covered separately.
- Compatibility checks against 1.1.0 for all 23 supported library targets.
  Proc-macro and binary-only targets are excluded by the compatibility tool;
  package verification and generated consumer tests cover those targets.
- Verification of all 25 package archives with all features enabled.
- Fresh archive consumers: all-adapter application, installed CLI and generated
  project, and both standalone applications.
- Dependency policy and vulnerability audit after updating locked rustls to
  0.23.45 for RUSTSEC-2026-0285. Four candidate consumer graphs and 539
  third-party package versions/checksums were audited against the official
  sparse registry.
- Website build, links, assets, and SEO checks.

[Pre-merge CI](https://github.com/vicotrbb/nidus/actions/runs/37873338038)
also passed its live service integration checks. Routing and request-lifecycle
benchmarks completed; these runs are not a controlled cross-version performance
comparison.

## Published artifacts and source consumers

All 25 package versions are published and unyanked. Every downloaded archive
matches its official registry checksum and is byte-for-byte identical to the
clean tagged source archive. Each archive's Git provenance identifies the
release commit without a dirty flag. The
[checksum manifest](https://github.com/vicotrbb/nidus/releases/download/v1.2.0/nidus-1.2.0-artifacts.json)
records package identity, SHA-256, and source provenance. All 25 versioned
docs.rs package pages were independently verified available.

Both tracked standalone example lockfiles were refreshed against the published
registry. Only Nidus versions and checksums changed, leaving third-party
packages and dependency edges unchanged. Their `cargo test --locked` runs
passed four tests total. Exact-version registry checks verified six Nidus
packages for support desk and twelve for commerce at 1.2.0, with no workspace
path replacements.

The
[website deployment](https://github.com/vicotrbb/nidus/actions/runs/37874714543)
succeeded, and the live
[release page](https://rustnidus.com/docs/release-1-2-0/) was checked separately.
The [GitHub release](https://github.com/vicotrbb/nidus/releases/tag/v1.2.0)
contains the release notes and checksum evidence. The tag remains fixed at the
published source commit; the subsequent main follow-up contains only these
consumer lockfiles and this validation record.

## Verification scope

The [publication workflow](https://github.com/vicotrbb/nidus/actions/runs/37874802243)
runs the release checks, uploads the full cohort in dependency order, then
verifies package availability, versioned docs.rs pages, and fresh public-registry
consumer applications. The publication and post-publication verification steps
completed successfully, including fresh public-registry consumer workflows.
The host checks use macOS/aarch64; GitHub CI uses Linux.
These checks establish the stated compatibility and release evidence, rather
than a guarantee that all software defects or production workload risks have
been eliminated.
