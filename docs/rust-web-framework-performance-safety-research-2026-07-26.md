# Rust Web Framework Performance and Safety Research - 2026-07-26

This note records a read-only architecture scan and a primary-source review for
small, measurable Nidus improvements. It is a candidate-selection document, not
benchmark evidence and not a claim that any proposed optimization will win.

The repository previously kept dated optimization notes under
`docs/performance/` and now keeps accepted measurements in
[`performance.md`](performance.md). This dated note follows the topic-plus-date
convention without changing the accepted-results document before a candidate has
passed its gates.

## Scope and source baseline

- Repository snapshot inspected: `2a9315a` on 2026-07-26.
- Workspace: Rust edition 2024, MSRV 1.96, Cargo feature resolver 2.
- Locked HTTP stack: Axum 0.8.9, Hyper 1.10.1, hyper-util 0.1.20, Tower 0.5.3,
  tower-http 0.6.11, and Tokio 1.53.1.
- Sources were retrieved on 2026-07-26. Versioned documentation links are used
  where behavior can vary by release.
- Scope included the workspace manifests, architecture and performance docs,
  root Criterion benches, module/DI/lifecycle code, request-scope middleware,
  production HTTP defaults, server helpers, metrics/observability code, tests,
  fuzz targets, and CI policy.

Primary references:

- [Cargo resolver versions](https://doc.rust-lang.org/cargo/reference/resolver.html#resolver-versions)
- [Rust 2024 resolver migration](https://doc.rust-lang.org/stable/edition-guide/rust-2024/cargo-resolver.html)
- [Rust `std::fmt::Write`](https://doc.rust-lang.org/std/fmt/trait.Write.html)
- [Rust `Box` and heap allocation](https://doc.rust-lang.org/std/boxed/)
- [Axum 0.8.9 body limits](https://docs.rs/axum/0.8.9/axum/extract/struct.DefaultBodyLimit.html)
- [Axum 0.8.9 server implementation](https://docs.rs/axum/0.8.9/src/axum/serve/mod.rs.html)
- [tower-http 0.6.11 request-body limits](https://docs.rs/tower-http/0.6.11/tower_http/limit/index.html)
- [Tower 0.5.3 layer order](https://docs.rs/tower/0.5.3/tower/builder/struct.ServiceBuilder.html#order)
- [Tower Service readiness contract](https://docs.rs/tower-service/0.3.3/tower_service/trait.Service.html#backpressure)
- [Tower 0.5.3 concurrency limiting](https://docs.rs/tower/0.5.3/tower/limit/concurrency/struct.ConcurrencyLimitLayer.html)
- [Tower 0.5.3 load shedding](https://docs.rs/tower/0.5.3/tower/load_shed/index.html)
- [Tokio 1.53.1 timeout and cancellation](https://docs.rs/tokio/1.53.1/tokio/time/fn.timeout.html)
- [Tokio 1.53.1 mutex selection](https://docs.rs/tokio/1.53.1/tokio/sync/struct.Mutex.html#which-kind-of-mutex-should-you-use)
- [Tokio 1.53.1 blocking work](https://docs.rs/tokio/1.53.1/tokio/task/fn.spawn_blocking.html)
- [Hyper 1.10.1 HTTP/1 connection settings](https://docs.rs/hyper/1.10.1/hyper/server/conn/http1/struct.Builder.html)
- [Hyper 1.10.1 HTTP/2 connection settings](https://docs.rs/hyper/1.10.1/hyper/server/conn/http2/struct.Builder.html)
- [hyper-util 0.1.20 graceful-server example](https://docs.rs/crate/hyper-util/0.1.20/source/examples/server_graceful.rs)

## Architecture and request/provider lifecycle

The relevant runtime contract is:

1. `NidusApplicationBuilder::build` validates `ModuleGraph`, runs every module's
   synchronous provider registrar, then awaits module async initializers before
   constructing the router
   (`crates/nidus/src/app.rs:159-203`).
2. `Container` owns a `TypeId`-keyed provider map using a private identity
   hasher (`crates/nidus-core/src/container/mod.rs:18-52`). Singleton resolution
   uses a guarded state machine and shares one `Arc` instance; transient
   resolution invokes the factory each time
   (`crates/nidus-core/src/provider/mod.rs:102-184`).
3. A request scope is not global. `request_scope_layer` creates one shared
   `RequestScope` and inserts it into that request's extensions
   (`crates/nidus-http/src/middleware/request_scope.rs:46-67`). Request-lifetime
   providers are cached in that scope; singleton/transient requests delegate to
   the container (`crates/nidus-core/src/container/request_scope.rs:57-76`).
4. `ApiDefaults::apply` composes explicit Axum/Tower layers. Its documented
   inbound order is security headers, validated request ID, request context,
   optional metrics, error envelope, timeout, optional streaming body limit,
   declared-length body limit, optional rate limit, panic catcher, then the
   handler (`crates/nidus-http/src/middleware/api_defaults.rs:298-364`).
5. `HttpApplication` serves the router through Axum with connected peer address
   information. Graceful variants stop accepting when the signal completes and
   await active connections (`crates/nidus-http/src/server.rs:75-160`).

This means module-graph work is startup work, not per-request work. Per-request
cost is introduced by the selected middleware, request-scope creation, and
providers actually resolved by the handler.

## Candidate 1: remove temporary allocations from observability exposition

### Current bottleneck

`render_observability_metrics` repeatedly builds temporary `String` values with
`format!` and then copies them into the final output
(`crates/nidus-observability/src/lib.rs:727-803`). Each histogram entry also:

- allocates a `Vec<&str>` for its labels
  (`crates/nidus-observability/src/lib.rs:754-800`);
- rebuilds the same rendered label set for every bucket, count, and sum line
  (`crates/nidus-observability/src/lib.rs:805-837`);
- creates one `Vec<String>` and joined `String` per label rendering
  (`crates/nidus-observability/src/lib.rs:839-846`);
- creates bucket and escaped-label strings
  (`crates/nidus-observability/src/lib.rs:848-856`).

`Observability::render_prometheus` additionally creates an empty output, obtains
an independently allocated HTTP metrics string, and appends a second
independently allocated observability string
(`crates/nidus-observability/src/lib.rs:315-327`).

The existing integration benchmark measures recording but has no exposition row
(`benches/integration_hot_paths.rs:87-120`). The existing tests assert selected
substrings rather than byte-for-byte output
(`crates/nidus-observability/tests/observability.rs:39-76`).

### Proposed change

Write directly into one `String` through `std::fmt::Write`, reserve from a
conservative size estimate, and render each histogram's escaped label set once.
Use fixed-size label slices/tuples rather than per-series vectors. Write escaped
label characters directly so the common no-escape path does not allocate a
second string.

This changes only private helpers. The public return type, metric names, ordering,
escaping, bucket boundaries, numeric formatting, and configured-series behavior
must remain byte-identical. `std::fmt::Write` is the standard formatting
interface for appending formatted data to a `String`; unlike
`push_str(&format!(...))`, it does not require an intermediate owned formatted
string.

### Proof required

Add a byte-for-byte fixture covering empty state, one and many series, all three
histograms, every status value, and labels containing backslash, quote, and
newline. Then add Criterion rows with 1, 10, and 100 admitted series.

Suggested commands:

```bash
cargo test -p nidus-observability --all-features
cargo bench --bench integration_hot_paths --all-features -- 'observability prometheus render' --warm-up-time 2 --measurement-time 5 --sample-size 150
```

For an optimization claim, run an identical benchmark harness against a detached
pre-change commit and the candidate in both execution orders with separate target
directories. Report output size, latency intervals, and allocations if the
benchmark harness has an allocation counter. Accept only byte equality, no
changed public API, and a repeatable improvement outside the suite's documented
5% noise threshold. Otherwise revert the candidate.

Assessment: **best immediate performance candidate**. The source contains
specific avoidable allocations and the changed boundary has a focused,
representative benchmark target.

## Candidate 2: align the edition-2024 workspace with Cargo resolver 3

### Current risk

The workspace declares edition 2024 and `rust-version = "1.96"` but explicitly
selects `resolver = "2"` (`Cargo.toml:1-43`). Cargo documents resolver 3 as the
edition-2024 resolver. Its material difference is MSRV-aware fallback when
selecting dependency versions. Because the workspace-level explicit resolver
wins, Nidus currently opts out of that edition-2024 dependency-selection
behavior.

### Proposed change

Change the workspace resolver from `"2"` to `"3"`. This is dependency-policy
hardening, not a runtime optimization. It helps future lockfile updates prefer
versions compatible with the declared MSRV. Resolver 3 requires Rust 1.84 or
newer; Nidus already requires and verifies Rust 1.96.

### Proof required

```bash
cargo metadata --locked --format-version 1
cargo tree --locked --workspace
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --all-features --no-deps
```

The locked graph must remain unchanged. Also create a temporary lockfile from
the candidate manifest on Rust 1.96 and confirm every selected package honors
its declared `rust-version`. No compile-time or runtime speed claim should be
made without separate measurements.

Assessment: **best immediate code-quality candidate**. It is deterministic,
public-API-compatible, aligned with the declared edition/MSRV, and easy to
rollback.

## Candidate 3: qualify a hard streaming cap for production defaults

### Current risk

`ApiDefaults::production` advertises a 1 MiB body limit but enables only the
declared `Content-Length` check; `streaming_body_limit` defaults to `None`
(`crates/nidus-http/src/middleware/api_defaults.rs:68-105`). The lightweight
layer passes bodies with absent or invalid `Content-Length` unchanged
(`crates/nidus-http/src/middleware/security.rs:84-99,161-188`). A hard streaming
wrapper already exists and is public
(`crates/nidus-http/src/middleware/security.rs:101-114`), but callers must opt in
through `ApiDefaults::streaming_body_limit`
(`crates/nidus-http/src/middleware/api_defaults.rs:220-233`).

Axum documents that its default 2 MiB extractor limit applies only to extractors
that opt into it. tower-http documents that `RequestBodyLimitLayer` applies to
all request bodies, rejects known oversized lengths early, and reports a length
error when an unknown-length body is consumed beyond the bound. Hyper handles
connection resynchronization for an oversized payload.

### Conditional change

Consider setting the production streaming cap to the same 1 MiB value as the
declared-length cap. This would make the documented production limit real for
chunked/headerless bodies without changing method signatures. It does change
observable behavior for existing applications that intentionally stream more
than 1 MiB through production defaults, so it is a reliability/security
candidate rather than a mechanically safe performance patch.

### Proof required

- Exact 1 MiB succeeds; 1 MiB + 1 byte fails.
- Known-length oversize is rejected before the handler.
- Headerless/chunked oversize fails as the handler consumes past the cap.
- An unread body does not get misreported as rejected.
- Error-envelope, request-ID, metrics, and connection reuse behavior remain
  correct after rejection.
- Add a `streaming body limit request` Criterion row beside the existing
  declared-length row and compare the full production-default stack with and
  without the wrapper.
- Run a representative mixed small-body/large-body workload and report p95/p99,
  throughput, failures, and RSS.

Assessment: **conditional**. Do not enable by default from a microbenchmark
alone; require application and connection-recovery evidence.

## Candidate 4: one allocation-free middleware future experiment only

### Current bottleneck

The rate-limit service returns
`Pin<Box<dyn Future<...>>>` and calls `Box::pin` on both the rejected and allowed
paths (`crates/nidus-http/src/middleware/rate_limit.rs:241-279`). A `Box`
uniquely owns a heap allocation, so this shape is evidence of one potential
per-call allocation. The repository already has a focused allowed-request row
(`benches/request_lifecycle.rs:248-255,561-572`).

### Conditional change and proof

A private concrete response-future enum could hold either an immediately ready
429 or the inner future plus decision metadata. Preserve `poll_ready`, error
type, headers, fail-open/fail-closed behavior, and cancellation.

Run the existing rate-limit middleware row plus new allowed, rejected, and store
error rows against an identical detached baseline. Add an allocation count if
practical. Require all focused tests and a repeatable improvement in both
execution orders.

Assessment: **experiment only**. Do not sweep all boxed middleware futures.
Concrete-future rewrites can increase state-machine size and code generation;
the repository already records an error-envelope concrete-future experiment
that regressed by 7.67%-9.62% (`docs/performance.md:655-659`). One local source
shape is not evidence for a blanket rule.

## Safety practices to retain

- Keep Tower readiness exact. `Service::call` may panic if readiness was not
  obtained, and a clone may not share the ready reservation. The guard
  middleware correctly replaces the ready inner service before moving it into
  its boxed future (`crates/nidus-auth/src/middleware.rs:67-104`).
- Keep standard mutexes for short, data-only critical sections that never cross
  `.await`. Tokio explicitly says this is often preferable to its more expensive
  async mutex. Do not interpret that guidance as approval for blocking I/O or
  condition-variable waits on a runtime worker.
- Keep blocking adapter work behind an explicit async boundary.
  `spawn_blocking` is appropriate for blocking work, but Tokio documents that a
  started blocking task cannot be aborted. A request timeout therefore must not
  be described as canceling such side effects.
- Preserve timeout semantics honestly. `tokio::time::timeout` cancels by dropping
  the inner future and checks the timeout only when the future yields. Nidus's
  timeout response layer has those semantics
  (`crates/nidus-http/src/middleware/security.rs:202-272`).
- Preserve explicit middleware order. Tower documents that layer order changes
  how many requests can be admitted or buffered; order is part of the resource
  and observability contract, not cosmetic composition.
- Keep bounded error-body aggregation
  (`crates/nidus-http/src/error.rs:324-328`) and metrics cardinality controls
  (`crates/nidus-http/src/middleware/metrics.rs:124-135`).

## Rejected or deferred ideas

### Custom Hyper server, header timer, and transport tuning

Nidus delegates all four serve methods to `axum::serve`
(`crates/nidus-http/src/server.rs:83-160`). Axum 0.8.9 constructs a default
hyper-util builder in each connection task. Hyper documents that its nominal
HTTP/1 header-read timeout requires a configured timer, that changing
`max_headers` moves header storage to the heap with an estimated performance
cost, that `pipeline_flush` is experimental, and that forced `writev` is
transport-dependent.

Replacing `axum::serve` merely to expose these knobs would add direct
Hyper/hyper-util coupling and duplicate accept, upgrade, connect-info, and
graceful-shutdown behavior. Reject it as a low-risk optimization. Revisit only
with HTTP/1 and HTTP/2, plain and TLS, slow-header, healthy-traffic-under-attack,
recovery, RSS, and graceful-drain qualification. Do not promote a timer or
connection cap merely because it bounds one counter.

### Global concurrency limit, buffer, or load shed

Tower provides these mechanisms, but its documentation shows that layer order
changes the total admitted work and that load shedding converts unready state
into errors. Nidus has no declared capacity value, overload response contract,
or representative availability benchmark. Adding a default would change
behavior and could move queuing to the socket backlog rather than preserve
healthy traffic. Reject until workload-specific gates exist.

### Mutex replacement and sharding

The in-memory rate limiter and metrics collector use short synchronous critical
sections (`crates/nidus-http/src/middleware/rate_limit.rs:59-140`;
`crates/nidus-http/src/middleware/metrics.rs:259-323`). Replacing them with
Tokio mutexes is not supported by Tokio's own guidance for data-only locks.
Sharding or atomics may help contention but require concurrent request/RSS/fault
evidence; single-thread lookup timing is insufficient.

The DI singleton and request-scope caches additionally use condition variables
to serialize first construction
(`crates/nidus-core/src/provider/mod.rs:130-184`;
`crates/nidus-core/src/container/request_scope.rs:109-163`). The container is a
synchronous public API, and it already offers eager singleton resolution to
avoid first-use waits (`crates/nidus-core/src/container/mod.rs:178-197`).
Changing these to async synchronization would alter the public resolution model.

### Blanket allocation and compiler tuning

Reject allocator swaps, blanket `#[inline]`, broad `String` to `Bytes`
migrations, release-profile changes, PGO, and native-CPU flags without a
repository workload and deployment contract. They can trade compile time,
binary size, portability, memory, or one workload against another. The existing
benchmark policy correctly requires comparison at the changed boundary and
forbids extrapolating a microbenchmark to server throughput
(`docs/performance.md:67-71,106-115,958-967`).

## Recommended implementation order

1. Add the observability-render fixture and benchmark, then attempt only the
   private allocation reduction.
2. Change resolver 2 to resolver 3 and run the locked full-workspace gates.
3. Evaluate the streaming production cap separately as a reliability campaign.
4. Attempt the rate-limit concrete future only if the first three leave budget
   and a byte-identical, allocation-aware A/B harness is available.

Stop and revert any performance candidate that does not reproduce outside the
configured noise threshold. Tests establish compatibility and correctness;
only measured comparisons establish performance.
