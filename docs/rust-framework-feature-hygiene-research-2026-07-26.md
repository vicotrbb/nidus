# Rust Framework Feature Hygiene Research - 2026-07-26

This note is a focused follow-up to the completed Nidus architecture and
request/provider lifecycle trace. It covers dependency-feature hygiene and
rejects broader runtime changes that are not justified by repository evidence.
It is research and candidate qualification, not benchmark evidence and not a
claim that compile time, runtime latency, or throughput has improved.

## Scope and source baseline

- Repository snapshot inspected: `a8f2476` on 2026-07-26.
- Workspace: Rust edition 2024, MSRV 1.96, Cargo resolver 3
  (`Cargo.toml:1-43,110-118`).
- Locked dependencies relevant here: SQLx 0.8.6, Tower 0.5.3, tower-http
  0.6.11, Axum 0.8.9, and Tokio 1.53.1.
- Repository evidence inspected: the root workspace manifest, the `nidus-rs`
  facade, `nidus-http`, `nidus-dashboard`, `nidus-sqlx`,
  `nidus-jobs-sqlx`, their feature-gated source, and the workspace examples
  that consume SQLx.
- Only official Cargo/Rust documentation and versioned first-party crate
  documentation/source are used below.

Primary references:

- [Cargo workspace dependency inheritance](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#inheriting-a-dependency-from-a-workspace)
- [Cargo features and feature unification](https://doc.rust-lang.org/cargo/reference/features.html#feature-unification)
- [Cargo resolver feature rules](https://doc.rust-lang.org/cargo/reference/resolver.html#features)
- [Cargo `tree` feature inspection](https://doc.rust-lang.org/cargo/commands/cargo-tree.html#feature-unification)
- [Rust 2024 inherited `default-features` rule](https://doc.rust-lang.org/edition-guide/rust-2024/cargo-inherited-default-features.html)
- [Cargo feature SemVer guidance](https://doc.rust-lang.org/cargo/reference/features.html#semver-compatibility)
- [SQLx 0.8.6 features](https://docs.rs/crate/sqlx/0.8.6/features)
- [SQLx 0.8.6 manifest source](https://docs.rs/crate/sqlx/0.8.6/source/Cargo.toml.orig)
- [SQLx 0.8.6 runtime and TLS support](https://docs.rs/sqlx/0.8.6/sqlx/index.html#runtime-support)
- [tower-http 0.6.11 `trace` module](https://docs.rs/tower-http/0.6.11/tower_http/trace/index.html)
- [Tower 0.5.3 layer order](https://docs.rs/tower/0.5.3/tower/builder/struct.ServiceBuilder.html#order)
- [Tower `Service` readiness and backpressure](https://docs.rs/tower-service/0.3.3/tower_service/trait.Service.html#backpressure)
- [Tower 0.5.3 load shedding](https://docs.rs/tower/0.5.3/tower/load_shed/index.html)
- [Axum 0.8.9 middleware error handling](https://docs.rs/axum/0.8.9/axum/middleware/index.html#error-handling-for-middleware)
- [Tokio 1.53.1 timeout semantics](https://docs.rs/tokio/1.53.1/tokio/time/fn.timeout.html)
- [Tokio 1.53.1 mutex selection](https://docs.rs/tokio/1.53.1/tokio/sync/struct.Mutex.html#which-kind-of-mutex-should-you-use)
- [Tokio 1.53.1 blocking-task constraints](https://docs.rs/tokio/1.53.1/tokio/task/fn.spawn_blocking.html)

## Cargo rules that govern the candidates

The following rules are material to Nidus; treating feature declarations as
local text without applying these rules produces misleading conclusions.

1. A member dependency declared with `workspace = true` inherits the workspace
   dependency declaration. Member-level `features` are **additive** with the
   workspace dependency's features. A member cannot subtract a feature selected
   by `[workspace.dependencies]`.
2. `optional` is intentionally member-local: it cannot be set in
   `[workspace.dependencies]`, but it may be set beside `workspace = true` in a
   member dependency.
3. Rust 2024 rejects a member's attempt to set `default-features = false` unless
   the inherited workspace dependency already sets it to false. Nidus correctly
   sets `default-features = false` for SQLx at the workspace level
   (`Cargo.toml:149`).
4. Cargo builds a dependency with the union of features requested for that
   dependency in the selected graph. Resolver 3 retains resolver 2's relevant
   feature-unification behavior; resolver 3's new default is MSRV-aware version
   fallback, not per-member feature isolation.
5. A whole-workspace build intentionally unifies normal dependency features
   across selected workspace members. Backend-isolation proofs therefore must
   select one adapter package per Cargo invocation. `cargo test --workspace
   --all-features` remains the compatibility proof for the combined graph, not
   the proof that a PostgreSQL-only consumer avoids SQLite.
6. Cargo recommends additive features and warns against mutually exclusive
   feature designs. Nidus should continue to support combined SQLx backend
   features; isolation means that selecting one backend does not silently select
   another, not that multiple backends are forbidden.

## Candidate 1: stop globally injecting SQLite into every inherited SQLx dependency

### Current bottleneck and exact evidence

The root workspace declaration is:

```toml
sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio", "sqlite"] }
```

at `Cargo.toml:149`. Because inherited features are additive, the `sqlite`
feature is present in every member that uses `sqlx.workspace = true`, even when
that member's public feature selection requests only PostgreSQL:

- `nidus-sqlx` inherits SQLx without member-local features
  (`crates/nidus-sqlx/Cargo.toml:26-38`). Its public `postgres` and `sqlite`
  features are declared separately (`crates/nidus-sqlx/Cargo.toml:15-20`), and
  `default = []`, but the workspace has already selected SQLite.
- `nidus-jobs-sqlx` adds SQLx `any` at the member dependency
  (`crates/nidus-jobs-sqlx/Cargo.toml:25-33`). Its `postgres` feature is
  separate from its default `sqlite` feature
  (`crates/nidus-jobs-sqlx/Cargo.toml:15-20`), but
  `--no-default-features --features postgres` still inherits workspace SQLite.
- `nidus-dashboard` makes the SQLx dependency optional and calls its feature
  `sqlite` (`crates/nidus-dashboard/Cargo.toml:14-25`), but that feature currently
  relies on the workspace declaration to select `sqlx/sqlite`.

The current graph was reproduced with:

```bash
cargo tree -p nidus-sqlx --no-default-features --features postgres \
  -e features -i sqlx-sqlite --locked
cargo tree -p nidus-jobs-sqlx --no-default-features --features postgres \
  -e features -i sqlx-sqlite --locked
```

Both commands show `sqlx feature "sqlite"` flowing from the selected Nidus
package and selecting `sqlx-sqlite v0.8.6`; the first command does so even
though `nidus-sqlx` has no default features, and the second does so after
`nidus-jobs-sqlx` defaults were explicitly disabled.

SQLx 0.8.6 does not require SQLite for PostgreSQL. Its official feature graph
maps `postgres` to the optional `sqlx-postgres` dependency, while `sqlite` maps
to the optional `sqlx-sqlite` dependency with its bundled SQLite feature. SQLx
`any` uses weak `?/any` edges for whichever drivers are otherwise selected; it
does not itself require SQLite. Nidus does need `any` in `nidus-jobs-sqlx`
because the public store exposes `AnyPool` and initializes its selected drivers
(`crates/nidus-jobs-sqlx/src/lib.rs:4-8,19,199-210`).

This is a deterministic dependency-graph and compile-surface issue. It is not
evidence of a runtime latency bottleneck: unused backend code may not reach the
final binary, and no runtime claim should be made from a Cargo tree.

### Proposed low-risk change

Remove only `"sqlite"` from the root SQLx feature list, retaining
`default-features = false` and `"runtime-tokio"`. Make the SQLite intent explicit
at each member that owns it:

- retain the existing `sqlx/sqlite` edges in the `sqlite` features of
  `nidus-sqlx` and `nidus-jobs-sqlx`;
- change `nidus-dashboard`'s `sqlite` feature to enable both `dep:sqlx` and
  `sqlx/sqlite`;
- retain the explicit SQLite features already present in workspace examples
  that use SQLx directly.

Do not remove `runtime-tokio` in this change. SQLx 0.8.6 documents that almost
all async APIs require a selected runtime and that `SqlitePool` also needs
runtime support for timeouts and internal management tasks. Nidus is a
Tokio-based framework, so retaining one explicit runtime is a reliability
requirement, not backend leakage.

The Nidus public feature names, default feature sets, public Rust items, pool
types, database behavior, and combined-backend support remain unchanged.
`nidus-jobs-sqlx` must keep `default = ["sqlite"]`; Cargo documents removing a
feature from a default set as potentially SemVer-incompatible. The proposed
change affects only consumers that explicitly disable that default and select a
different backend, aligning their graph with the already documented feature
contract.

### Deterministic proof

Run backend checks as separate package selections so workspace feature
unification does not invalidate the isolation test:

```bash
cargo check --locked -p nidus-sqlx --no-default-features --features sqlite
cargo check --locked -p nidus-sqlx --no-default-features --features postgres
cargo check --locked -p nidus-sqlx --no-default-features --features mysql
cargo check --locked -p nidus-sqlx --no-default-features --features cockroach
cargo check --locked -p nidus-sqlx --all-features

cargo check --locked -p nidus-jobs-sqlx --no-default-features --features sqlite
cargo check --locked -p nidus-jobs-sqlx --no-default-features --features postgres
cargo check --locked -p nidus-jobs-sqlx --no-default-features --features mysql
cargo check --locked -p nidus-jobs-sqlx --no-default-features --features cockroach
cargo check --locked -p nidus-jobs-sqlx --all-features

cargo check --locked -p nidus-dashboard --no-default-features --features sqlite
```

Assert the unwanted package is absent from PostgreSQL-only graphs and present
from SQLite graphs:

```bash
if cargo tree -p nidus-sqlx --no-default-features --features postgres \
  --locked --prefix none | rg -q '^sqlx-sqlite v'; then
  exit 1
fi
if cargo tree -p nidus-jobs-sqlx --no-default-features --features postgres \
  --locked --prefix none | rg -q '^sqlx-sqlite v'; then
  exit 1
fi
cargo tree -p nidus-sqlx --no-default-features --features sqlite \
  --locked --prefix none | rg '^sqlx-sqlite v'
cargo tree -p nidus-dashboard --no-default-features --features sqlite \
  --locked --prefix none | rg '^sqlx-sqlite v'
```

Then run the existing behavioral and combined-feature gates:

```bash
cargo test --locked -p nidus-sqlx --all-features
cargo test --locked -p nidus-jobs-sqlx --all-features
cargo test --locked -p nidus-dashboard --all-features
cargo test --locked --workspace --all-features
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

If compile-time improvement is claimed, compare clean, isolated target
directories for the same PostgreSQL-only command at the pre-change commit and
candidate, in both execution orders, and report wall time plus the package
counts. The deterministic acceptance gate is removal of `sqlx-sqlite` from the
isolated graph; a speed percentage requires repeated measurements and must not
be inferred from that removal alone.

Assessment: **accept for implementation**. The feature leak is reproduced,
SQLx's official graph establishes that it is unnecessary, and the fix is
private-manifest-only with a focused deterministic gate.

## Candidate 2: make tower-http optional in the facade and wire it to observability

### Current bottleneck and exact evidence

The `nidus-rs` facade declares `tower-http.workspace = true` unconditionally
(`crates/nidus/Cargo.toml:34-49`). Its only direct source use is
`tower_http::trace::TraceLayer` in the observability-tracing branch
(`crates/nidus/src/app.rs:285-309`). That code is compiled only when
`observability` is enabled, and `observability` already implies `http`
(`crates/nidus/Cargo.toml:19-24`).

The unconditional edge means a core-only facade consumer still selects
tower-http and all features placed on it by the workspace:

```bash
cargo tree -p nidus-rs --no-default-features --locked --depth 1
cargo tree -p nidus-rs --no-default-features \
  -e features -i tower-http --locked
```

On the inspected snapshot, the first command includes `tower-http v0.6.11`.
The inverted feature tree shows `compression-gzip`, `cors`, `limit`, and
`trace`, inherited from `Cargo.toml:156`. In particular, `compression-gzip`
selects the async-compression/flate2 compression stack even though none of the
facade code compiled by `--no-default-features` can reference tower-http.

This issue is limited in scope. Default facade builds enable `http`, and
`nidus-http` legitimately uses compression, CORS, limits, and tracing in its
public middleware helpers (`crates/nidus-http/src/middleware.rs:3-10,75-88`).
Therefore this candidate reduces the graph for core-only or other non-HTTP
facade configurations; it does not reduce the normal default HTTP graph.

### Proposed low-risk change

Declare the facade dependency as member-local optional:

```toml
tower-http = { workspace = true, optional = true }
```

and add `"dep:tower-http"` to the existing `observability` facade feature.
Cargo explicitly permits `optional` beside `workspace = true`, and the `dep:`
syntax keeps this implementation detail inside the user-facing
`observability` feature instead of creating another public implicit feature.
tower-http documents that `TraceLayer` is available under its `trace` feature;
the workspace already selects it.

No Rust item or signature changes. Every configuration that compiles the direct
`tower_http` path still enables the dependency, while configurations that
cannot compile that path stop paying for it. `nidus-http` remains unchanged.

### Deterministic proof

```bash
cargo check --locked -p nidus-rs --no-default-features
cargo check --locked -p nidus-rs --no-default-features --features http
cargo check --locked -p nidus-rs --no-default-features --features observability
cargo check --locked -p nidus-rs --all-features

if cargo tree -p nidus-rs --no-default-features \
  --locked --prefix none | rg -q '^tower-http v'; then
  exit 1
fi
cargo tree -p nidus-rs --no-default-features --features observability \
  --locked --prefix none | rg '^tower-http v'
```

Run the facade feature and packaging gates:

```bash
cargo test --locked -p nidus-rs --no-default-features
cargo test --locked -p nidus-rs --no-default-features --features observability
cargo test --locked -p nidus-rs --all-features
cargo package --locked -p nidus-rs --allow-dirty
cargo test --locked --workspace --all-features
```

A compile-time claim requires repeated cold builds of the core-only facade
configuration against the same toolchain and lockfile, using isolated target
directories and both comparison orders. The verified graph reduction alone
supports a dependency-hygiene claim, not a compile-time percentage.

Assessment: **accept for implementation**. The dependency is unreachable
without the facade's observability feature, Cargo has a first-class optional
dependency mechanism for this case, and the feature matrix makes the safety
claim deterministic.

## Rejected or deferred candidates

### Do not remove SQLx `any` from the durable job store

`nidus-jobs-sqlx` exposes `sqlx::AnyPool`, constructs it with
`AnyPoolOptions`, and calls `install_default_drivers`
(`crates/nidus-jobs-sqlx/src/lib.rs:4-8,19,199-210,228-243`). Removing the
`any` feature would require a different public pool type or backend-specific
store implementations. That is an API and architecture change, not a small
feature-hygiene optimization. SQLx's weak driver edges mean `any` is not the
cause of the unconditional SQLite selection.

Decision: **reject** for this optimization pass.

### Do not make database backends mutually exclusive

Nidus documents its SQLx features as independent and supports applications that
compile multiple pools. Cargo states that features should be additive and that
mutually exclusive features should be avoided. The existing
`--all-features` gates are valuable compatibility coverage.

Decision: **reject**. Isolate single-backend builds without breaking valid
combined-backend builds.

### Do not remove the `nidus-jobs-sqlx` default SQLite feature

The default is documented (`docs/jobs.md:42-44`) and declared at
`crates/nidus-jobs-sqlx/Cargo.toml:16`. Cargo cautions that removing a feature
from the default set can be SemVer-incompatible.

Decision: **reject**. The correct target is the PostgreSQL-only
`--no-default-features` graph.

### Do not gate `nidus-http` middleware modules merely to shrink features

`nidus-http` directly exposes compression, CORS, body-limit, and tracing helpers
through its current public API (`crates/nidus-http/src/middleware.rs:7-9,25-45`).
Putting those items behind new crate features would move existing public code
behind feature gates, which Cargo identifies as potentially breaking. There is
no isolated evidence that a larger public feature redesign would improve the
default framework build enough to justify that compatibility cost.

Decision: **reject**. Make only the facade's provably unreachable direct edge
optional.

### Do not reorder the production Tower/Axum stack from a microbenchmark

Tower documents that layer order changes which layer receives a request first
and changes the total work admitted by combinations such as buffers and
concurrency limits. Nidus deliberately documents and tests its production
inbound order, including request IDs, metrics, error envelopes, timeouts, body
limits, rate limiting, panic catching, and handlers
(`crates/nidus-http/src/middleware/api_defaults.rs:298-364`). Reordering can
change whether a rejection is enveloped, metered, or assigned a request ID even
when a narrow benchmark gets faster.

Decision: **reject** absent semantic regression tests plus representative
end-to-end latency, throughput, failure-rate, and memory evidence.

### Do not add a global concurrency limit or load shedding by default

Tower's `Service` contract uses `poll_ready` for capacity and permits failure
when a service is called without readiness. `ConcurrencyLimit` changes the
number of in-flight requests, while `LoadShed` converts inner unavailability
into immediate errors. The correct bound and overload response depend on
handler latency, downstream pools, request mix, and deployment concurrency.
Nidus's existing production defaults do not contain enough workload information
to choose one universal limit (`crates/nidus-http/src/middleware/api_defaults.rs:325-364`).

Decision: **reject** as a broad default. Any future admission candidate needs a
mixed healthy/slow workload that proves healthy availability, p95/p99 latency,
throughput, rejection behavior, and bounded CPU/RSS under saturation.

### Do not blanket-rewrite boxed Tower futures

`RateLimitService` currently delegates `poll_ready` to the exact inner service
and boxes its response future
(`crates/nidus-http/src/middleware/rate_limit.rs:241-280`). Tower warns that
cloning an inner service after readiness can call a different, unready clone and
may panic. A concrete future can be a valid local experiment, but a broad
rewrite risks readiness correctness and larger generated state machines.

Decision: **defer**. Qualify one middleware at a time with allowed, rejected,
store-error, cancellation, and allocation/latency measurements; revert it if the
focused result is neutral or regresses.

### Do not treat Tokio primitives as universally interchangeable

Tokio documents that a standard mutex is often preferred when the protected
value is plain data and its guard never crosses an `.await`, while the async
mutex is intended for guards held across asynchronous work. Tokio also documents
that `spawn_blocking` work cannot be aborted once started and can delay runtime
shutdown indefinitely; CPU-bound use should be explicitly bounded. Finally,
Tokio timeouts check the deadline before polling and cannot preempt a future
that does not yield.

Nidus uses blocking boundaries for synchronous exporter/client flush calls
(`crates/nidus-opentelemetry/src/lib.rs:490-507`,
`crates/nidus-kafka/src/lib.rs:399-413`, and
`crates/nidus-sentry/src/lib.rs:379-405`). Their cancellation and shutdown
contracts matter more than a blanket task-spawning rule.

Decision: **reject** blanket mutex swaps, `spawn_blocking` additions/removals,
or stronger timeout guarantees without a call-site lifecycle analysis and a
focused test. A timeout result is not proof that non-yielding work was forcibly
stopped.

### Preserve Axum's infallible router error boundary

Axum requires middleware errors to be converted into responses; otherwise a
connection can terminate without a response. Nidus's raw Tower
`timeout_layer` documentation now states this and points callers to
`HandleErrorLayer`, while `timeout_response_layer` is the response-producing
HTTP helper (`crates/nidus-http/src/middleware.rs:47-57`).

Decision: **no further change justified**. Do not replace the response timeout
with raw Tower timeout middleware merely because the latter is shorter or more
generic.

## Recommended implementation set

Only two changes pass the current evidence gate:

1. remove workspace-global `sqlx/sqlite`, then make each SQLite-owning member
   explicit;
2. make the facade's direct tower-http dependency optional and enable it from
   `observability`.

Both are manifest-only, preserve the public feature names and Rust API, have
deterministic dependency-graph assertions, and can be fully exercised through
the existing combined-feature suite. They should be described as dependency
and compile-surface hygiene. Compile-time savings, binary-size savings, and
runtime effects remain unproven until measured independently.
