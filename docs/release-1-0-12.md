# Release 1.0.12

Nidus 1.0.12 is a public-API-compatible patch release focused on faster request
paths, stricter runtime behavior, and stronger reproducible performance proof.

## Request-path efficiency

- Successful responses no longer allocate error-envelope path and request-ID
  strings, and panic isolation uses a concrete future instead of boxing every
  request.
- Guard route labels, trusted-proxy lists, and Prometheus labels share immutable
  storage instead of cloning repeated strings. Request context also reuses the
  request ID as its default correlation ID.
- Health routes prebuild immutable state and abort unfinished checks when a
  request is cancelled. OpenAPI fixed methods avoid owned-string allocation,
  and unnamespaced Moka reads and invalidations avoid temporary cache keys.

## Runtime hardening

- Trusted-proxy identity now walks all X-Forwarded-For fields from the connected
  peer toward the first untrusted hop, preventing attacker-controlled leftmost
  values from selecting a rate-limit identity.
- SQLite and PostgreSQL readiness failures no longer expose driver details in
  public health responses.
- Lifecycle tracing spans are active only while their asynchronous hook future
  is being polled, preventing unrelated work from inheriting lifecycle context.

## CLI discovery

The routes, graph, and OpenAPI inspection commands now use one deterministic,
recursive source walker under src. This matches generated and feature-oriented
layouts and avoids treating valid OpenAPI examples in source text as malformed
attributes.

## Homelab benchmark

The release includes a new paced end-to-end campaign comparing the exact
v1.0.4 tag with the 1.0.12 candidate across ping, users, projects, events, and
mixed workloads. Throughput remained effectively flat or slightly higher in
all five profiles. The mixed profile improved average latency by 16.89% and p95
latency by 8.89%.

All Nidus candidate and control groups passed the declared repeatability gates,
with 0% HTTP failures and 100% checks. The overall cross-framework campaign is
published as qualified rather than strictly accepted because Spring ping
average-latency CV reached 15.58% against its 15% threshold. No samples were
discarded and no threshold changed.

The benchmarked source is commit c0b6d82e96494850a553a1fdc5589e26d71ca4f0;
subsequent changes before release are documentation and release metadata only.

## Verification boundary

The release candidate is checked with workspace formatting, strict Clippy,
all-feature tests, rustdoc warnings as errors, dependency policy and RustSec
audits, semver checks, package file-list preflights, website verification, and
standalone external examples using temporary local patches.

After publication, verify all 25 registry artifacts, docs.rs pages, and the two
standalone external examples against crates.io with:

    bash scripts/verify-published-release.sh 1.0.12
