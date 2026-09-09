# Controlled release measurements

This unpublished standalone tool adds `stats_alloc` only to the optional allocation-instrumentation build. Framework runtime dependencies are unchanged. The Rust harness forbids unsafe code; allocation instrumentation uses the dependency's safe interface.

From the repository root, run:

```sh
python3 scripts/measure-release-performance.py target/release-readiness/performance prepare
# Stop other validation/compilation before measuring.
python3 scripts/measure-release-performance.py target/release-readiness/performance measure
```

Preparation builds the same harness against baseline commit `99642ed0018c6da5c921db92d966f3b52a7e7d77` and the candidate, in separate target directories, with identical third-party versions and compiler flags. It records the candidate source and executable hashes; measurement refuses changed inputs. Ten paired runs alternate baseline/candidate order after warmup. Raw routing is a control; metrics and production middleware measure successful responses through response availability. Allocation counts use separate instrumented binaries and never supply latency estimates. Managed composition, startup and shutdown are measured together as a separate candidate-only operation, outside the request path.

Results are local microbenchmarks, not throughput or tail-latency guarantees. Inspect every paired sample, allocation totals and the raw control before interpreting medians. A materially noisy or regressed result requires investigation; a cached Criterion comparison is not release evidence.
