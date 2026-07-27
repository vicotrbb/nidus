# Release 1.0.16

Nidus 1.0.16 is a public-API-compatible reliability, performance, and
compile-surface patch containing every change merged to `main` since 1.0.15.

## Runtime reliability

- Bounded event queues now remove the oldest payload under the queue lock but
  destroy it only after releasing the lock. Slow, reentrant, and panicking
  destructors can no longer unnecessarily extend or poison the mutation
  critical section.
- Provider registrars and async initializers now run in validated
  dependency-first order. Imported providers are available before their
  importers without changing the deterministic name order returned by
  `ModuleGraph::modules()`.
- Explicit feature modules may share one recursively typed dependency without
  producing a false `DuplicateModule` error. The shared provider and initializer
  execute once; caller-supplied duplicate definitions still fail.
- Framework-owned fallback rate-limit identities now borrow their static value
  instead of allocating a temporary `String`. Opposite-order benchmark results
  did not establish a stable request-latency percentage, so the claim is limited
  to the deterministic allocation removal.

## Focused performance work

- Prometheus exposition now writes directly into one output buffer, streams
  label escaping, and reuses fixed label storage. Confirmation benchmarks for
  1, 10, and 100 admitted lifecycle series improved by 83.29%-87.28%.
- Health readiness borrows registered check names and callbacks through
  serialization instead of cloning them for every response. The final
  eight-check confirmation improved by 15.84%-17.22%; an earlier noisy repeat
  remains documented, so no production latency guarantee is made.
- Typed configuration lookup now traverses path segments while constructing the
  same diagnostic label once. Paired opposite-order comparisons improved a
  six-segment typed lookup by 31.17%-37.61%.

These are focused in-process benchmarks. They do not establish application
throughput, process RSS, availability, or p95/p99 latency.

## Feature and safety hygiene

- SQLite is no longer selected by PostgreSQL-only SQLx adapter builds. The
  normal dependency graph drops from 149 to 142 unique packages.
- `tower-http` and its compression dependencies are no longer selected by the
  core-only facade. That graph drops from 53 to 33 unique packages.
- Clean-build timing changed with execution order and does not support a
  compile-time speed claim; the retained result is the deterministic dependency
  and compile-surface reduction.
- Cargo resolver 3 now aligns dependency selection with the edition-2024/MSRV
  workspace policy.
- All framework, workspace-harness, and `cargo-nidus` crate roots forbid unsafe
  Rust, turning the existing safe-only implementation into a compiler-enforced
  invariant.
- Feature-matrix checks use `--locked`, and controller/injectable macro
  expansion shares one private parser while preserving the complete trybuild
  diagnostic contract.

## Documentation and proof

- Native Tower timeout layers are documented as returning raw timeout errors;
  applications can map them with `HandleErrorLayer`, while
  `timeout_response_layer` remains the Nidus HTTP 408 option.
- Async observed-job examples now show the observer, job, queue, runner, and
  execution lifecycle together.
- New focused tests cover destructor reentrancy and panic recovery, typed-path
  edge cases, dependency-first startup, shared typed subgraphs, explicit
  duplicate diagnostics, feature isolation, and request-scoped extraction.

The release candidate is checked with workspace formatting, warnings-denied
Clippy, all-feature tests and doctests, isolated locked feature combinations,
rustdoc warnings as errors, dependency and RustSec policies, semver checks for
every publishable library, package file-list preflights, website verification,
benchmark-harness compilation, fuzz-target compilation, and standalone
external examples.

After publication, verify all 25 registry artifacts, docs.rs pages, and the two
standalone external examples against crates.io with:

```bash
bash scripts/verify-published-release.sh 1.0.16
```
