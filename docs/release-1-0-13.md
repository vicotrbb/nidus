# Release 1.0.13

Nidus 1.0.13 is a public-API-compatible patch release focused on reducing
framework-owned work in dependency injection, readiness checks, event and job
dispatch, middleware, and structured logging while tightening HTTP and CLI
behavior.

## DI and startup efficiency

- Module validation now specializes empty and single-name metadata, moves owned
  names into graph indexes once, borrows graph-owned names during cycle checks,
  and allocates ambiguity lists only when multiple exporters exist.
- Initialized singleton state no longer retains a second strong `Arc`; the
  existing `OnceLock` remains the authoritative fast-path cache.
- Exact duplicate, cycle, ambiguity, nested-resolution, teardown, retry, panic,
  and concurrent-initialization behavior remains covered by focused tests.

## Runtime hot paths

- Readiness checks are polled concurrently in the request future instead of
  spawning one Tokio task per check. Timeouts, panic-to-down mapping,
  cancellation, and deterministic response ordering are preserved.
- One-subscriber event publication no longer allocates a temporary collector or
  clones the event. Additional subscribers still receive ordered clones.
- Observed event and job attributes now share immutable storage until explicit
  enrichment requires copy-on-write ownership.
- Logging redaction lookup normalizes configured names once and performs
  allocation-free case-insensitive binary search at request time.
- Request-ID and rate-limit middleware borrow configuration during synchronous
  policy work instead of cloning the complete configuration per request.

The retained Criterion results are isolated microbenchmarks documented in
`docs/performance.md`; they are not end-to-end throughput guarantees. Repeated
comparisons classified the changed rows as improvements, while experiments that
regressed controls or failed to reproduce were reverted.

## HTTP, security, and CLI correctness

- Request-scoped extraction failures no longer expose provider type names or
  factory details to clients; full diagnostics remain server-side.
- Bearer authentication schemes are matched case-insensitively.
- `without_body_limit` disables both declared-length and streaming body caps.
- Error-envelope body replacement removes stale content length, encoding, and
  range headers while retaining unrelated response metadata.
- Integration-envelope `traceparent` validation accepts valid future-version
  extensions while keeping version `00` exact and rejecting malformed or
  oversized values.
- `cargo nidus routes` and `cargo nidus openapi` can associate uniquely named
  controllers and route implementations split across source files, while
  ambiguous short names fail with an actionable diagnostic.
- `#[nidus::main]` documentation now matches the macro's existing support for
  named async functions with arguments and explains the nested-runtime boundary.

## Verification boundary

The release candidate is checked with workspace formatting, warnings-denied
Clippy, all-feature tests, rustdoc warnings as errors, dependency policy and
RustSec audits, semver checks for all publishable crates, package file-list
preflights, website verification, benchmark harness compilation, and standalone
external examples using temporary local patches.

After publication, verify all 25 registry artifacts, docs.rs pages, and the two
standalone external examples against crates.io with:

```bash
bash scripts/verify-published-release.sh 1.0.13
```
