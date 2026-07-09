# Release 1.0.7

Nidus 1.0.7 is a measured performance, trace-correctness, and runtime-hardening
patch release. Public APIs remain compatible with 1.0.6; the changes are
internal data-structure improvements, stricter standards validation, and fixes
that preserve existing recovery semantics.

## Bounded Event Queues

- Bounded subscribers now use a ring buffer, so evicting the oldest event is
  constant-time instead of shifting the retained queue on every publication.
- Unbounded subscribers retain their existing `Vec` storage and O(1) drain
  path, and declaring a bounded subscription still allocates lazily.
- Zero-capacity, FIFO eviction, second-batch, unbounded, poisoned-lock recovery,
  and multi-subscriber behavior remain covered by tests.

At a full 10,000-event bound, the saved-baseline Criterion row moved from
1.0719-1.1939 us to 67.098-78.214 ns. Criterion reported a 93.9%-94.7%
improvement for this local 100-sample run.

## Trace Context And Structured Logging

- Request context, structured logging, and OpenTelemetry helpers now share one
  allocation-free W3C `traceparent` parser.
- Version `ff`, uppercase or malformed identifiers, all-zero trace/span IDs,
  and extensions on version `00` are rejected. Future-version extensions are
  accepted and ignored as required for forward compatibility.
- Request context now populates the existing parent span ID field instead of
  leaving it empty.
- Structured HTTP spans borrow request ID, trace ID, and route fields rather
  than allocating owned strings before recording them.

The structured-span Criterion row moved from 158.54-159.89 ns to
83.327-86.991 ns. Criterion reported a 37.9%-42.0% improvement for this local
100-sample run.

## Runtime Hardening

- Security-header, declared body-limit, and timeout-response middleware no
  longer allocate boxed futures on their straightforward request paths.
- Request-scoped provider resolution clears in-progress cache state and wakes
  waiters after a factory panic, matching singleton recovery behavior.
- `crossbeam-epoch` was updated to 0.9.20 to address RUSTSEC-2026-0204 in the
  cache adapter and benchmark dependency graph.
- Standalone-example release verification now rejects occupied smoke-test
  ports, starts built binaries directly so cleanup owns the actual server
  process, and prints captured logs when a runtime check fails.

## Rejected Experiment

A borrowed-key rate-limit lookup removed an owned key allocation but regressed
the measured 10,000-identity row. The experiment was reverted; the rate-limit
implementation is unchanged in 1.0.7.

## Release Evidence

The release candidate was validated with:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps
cargo deny check
cargo audit --deny warnings
bash scripts/semver-check-publishable-crates.sh
bash scripts/package-publishable-crates.sh --list-only
npm --prefix website run verify
```

After publishing, verify every crate, docs.rs page, and standalone example:

```bash
bash scripts/verify-published-release.sh 1.0.7
```
