# Rust Framework Performance and Safety Research - 2026-07-27

This note narrows the next Nidus optimization and reliability candidates after
the broader 2026-07-26 review. It is a candidate-selection document, not
benchmark evidence. A source-level allocation observation does not establish a
latency, throughput, RSS, or compile-time improvement until the repository's
proof gates pass.

## Inspected baseline

- Committed source inspected: `ab1c89d66e61`, plus the uncommitted focused
  proof-harness additions present during this review.
- Local toolchain: `rustc 1.96.0` and `cargo 1.96.0`.
- Workspace: Rust edition 2024, MSRV 1.96, Cargo resolver 3.
- `cargo metadata --locked --format-version 1` confirms Axum 0.8.9, Tower
  0.5.3, tower-http 0.6.11, tower-service 0.3.3, Hyper 1.10.1, hyper-util
  0.1.20, Tokio 1.53.1, Serde 1.0.228, serde_json 1.0.151, and futures-util
  0.3.32.
- Sources were checked on 2026-07-27. Versioned crate documentation is linked
  where behavior is stack-version-sensitive.

The previous note already evaluated transport tuning, production body limits,
Tower readiness, blocking work, mutex selection, concurrency limiting, and
blanket compiler/allocation tuning
(`docs/rust-web-framework-performance-safety-research-2026-07-26.md:353-431`).
Those conclusions are retained unless this note identifies new repository
evidence.

## Relevant runtime boundary

`Nidus::bootstrap_with_lifecycle` validates a module graph, registers every
synchronous provider, awaits async provider initializers, and only then starts
lifecycle hooks (`crates/nidus-core/src/app/mod.rs:88-120`). The facade builder
does the same registration and initialization work before constructing the Axum
router (`crates/nidus/src/app.rs:164-206`). These are startup paths.

Ordinary HTTP requests do not traverse the module graph. The request-path work
considered here is limited to selected Axum/Tower middleware, specifically
rate-limit identity creation and the already measured rate-limit service.

This distinction matters: module ordering can be a deterministic reliability
fix without being called a request-performance optimization, while
`RequestIdentity` construction can be evaluated as a request-path allocation
change.

## Selection summary

| Candidate | Boundary | Decision |
| --- | --- | --- |
| Borrow framework-owned static request identities | Per rate-limited request and missing-peer fallback | Implement only with content/API tests and paired measurement |
| Fuse nested typed config traversal and error-label construction | Typed config read | Implement with exact error-parity tests and a focused benchmark |
| Initialize imported modules before their importers | Application startup | Implement as reliability hardening; do not claim request performance |
| Replace the public rate-limit service's boxed future | Per rate-limited request | Reject for this pass |

## Candidate 1: borrow framework-owned static request identities

### Current bottleneck

`RequestIdentity` is a public tuple struct with one private `String`, and its
public constructor always converts through `Into<String>`
(`crates/nidus-http/src/context.rs:278-292`). Dynamic identities need ownership,
but Nidus also constructs the literal `"anonymous"` on framework-owned paths:

- the default `RateLimitConfig` identity extractor
  (`crates/nidus-http/src/middleware/rate_limit.rs:181-190`);
- the service fallback when a custom extractor returns `None`
  (`crates/nidus-http/src/middleware/rate_limit.rs:255-268`);
- missing connected-peer information in `client_ip_identity`
  (`crates/nidus-http/src/context.rs:326-337`);
- missing connected-peer information in
  `trusted_proxy_client_ip_identity`
  (`crates/nidus-http/src/context.rs:340-364`).

Each `RequestIdentity::new("anonymous")` converts a static string slice into an
owned `String`. The default rate-limit identity path therefore creates a
temporary heap-backed identity on every request before the in-memory store
performs its lookup. The focused allowed-request benchmark uses exactly that
default identity (`benches/request_lifecycle.rs:263-270,584-595`).

This observation is narrow. The in-memory store intentionally owns a `String`
when it creates a new identity window
(`crates/nidus-http/src/middleware/rate_limit.rs:127-139`), so borrowing the
temporary identity does not remove the one-time map-key allocation for a new
identity.

### Proposed change

Change only the private representation to `Cow<'static, str>`. Keep the public
constructor signature exactly as:

```rust
pub fn new(value: impl Into<String>) -> Self
```

and store its result as `Cow::Owned`. Add a private constructor for
framework-owned `&'static str` values and use it only at the four static fallback
sites above. Continue to own header values, user/tenant values, and formatted IP
addresses.

Do **not** generalize the public constructor to `impl Into<Cow<'static, str>>`.
Although attractive, that changes a public generic bound and allows callers to
observe different ownership behavior. The private constructor gets the desired
framework optimization without changing source compatibility.

Rust's [`Cow` documentation][rust-cow] defines borrowed and owned variants,
immutable access through the borrowed type, and implementations for `Clone`,
`Eq`, and `Hash`. That makes it suitable for a content-key identity: borrowed
and owned `"anonymous"` values compare and hash by string content. The
[`Cow` implementation list][rust-cow] and a direct content-parity test are both
required evidence; the type choice alone is not proof of unchanged behavior.

The tuple field is already private and `RequestIdentity` has default Rust
representation. Cargo's [SemVer compatibility guidance][cargo-semver-private]
states that private fields of default-representation types may change without a
layout compatibility promise, subject to preserving the rest of the public
contract. The constructor, `as_str`, derives, and type name remain unchanged.

### Required proof

Add focused tests that prove:

1. `RequestIdentity::new(String)` remains owned and returns identical bytes.
2. The private static constructor is borrowed and cloning it does not turn it
   into an owned string.
3. Borrowed and owned identities with the same text are equal, hash identically,
   and address the same in-memory rate-limit window.
4. Default, missing-extractor, missing-peer, and trusted-proxy missing-peer
   fallbacks still yield exactly `"anonymous"`.
5. Existing custom dynamic identity extractors remain source-compatible and
   behavior-compatible.

Use the existing allowed rate-limit row as the primary latency control. A
deterministic allocation counter, if available in the repository harness, should
show that repeated default-identity requests remove one temporary allocation.
Run an untouched detached baseline and the candidate in both execution orders
with separate target directories:

```bash
cargo test -p nidus-http --all-features

CARGO_TARGET_DIR=/tmp/nidus-identity-baseline \
  cargo bench --bench request_lifecycle --all-features -- \
  'nidus middleware rate limit request' \
  --warm-up-time 3 --measurement-time 8 --sample-size 200 --noplot

CARGO_TARGET_DIR=/tmp/nidus-identity-candidate \
  cargo bench --bench request_lifecycle --all-features -- \
  'nidus middleware rate limit request' \
  --warm-up-time 3 --measurement-time 8 --sample-size 200 --noplot
```

Use the repository's 5% Criterion noise threshold
(`docs/performance.md:115-118`). If allocation removal is not counted
deterministically and the latency result is not repeatable, report only the
representation change and do not claim a performance win.

Assessment: **best narrow request-path candidate**. It targets literal-owned
data under framework control and leaves dynamic identities owned.

## Candidate 2: fuse typed config traversal and error-label construction

### Current bottleneck

Raw `Config::get_path` traverses an arbitrary path iterator without collecting
it (`crates/nidus-config/src/lib.rs:188-211`). Both typed variants instead:

1. clone every segment into a `String`;
2. collect the strings into a `Vec`;
3. join them into a second owned error label;
4. iterate the collected path again to locate the value.

The duplicated shape is in
`Config::get_path_typed` (`crates/nidus-config/src/lib.rs:213-228`) and
`Config::get_required_path_typed`
(`crates/nidus-config/src/lib.rs:230-246`). A six-segment borrowed path therefore
creates six segment strings, a vector backing allocation, and the final
dot-separated label even though traversal needs only temporary `&str` access.

Typed deserialization itself is already borrowed: serde_json 1.0.151 implements
`serde::Deserializer` for `&Value`
([official serde_json source][serde-json-value-de]), and Nidus passes the stored
value by reference (`crates/nidus-config/src/error.rs:67-72`). This candidate
must not undo that previous improvement.

### Proposed change

Add one private helper that consumes the path once. For every segment it:

- appends `.` plus the segment bytes to one label `String`;
- advances an `Option<&Value>` through objects or arrays while a value remains;
- continues consuming and appending segments after a lookup failure so the
  final missing/error path remains byte-for-byte identical.

Both public typed methods should delegate to that helper. The raw `get_path`
method and all public signatures remain unchanged. Rust documents `String` as a
growable heap buffer and `push_str` as appending a borrowed slice
([`String` documentation][rust-string]); one final growable label is enough, so
there is no reason to own every segment separately.

The important correctness edge is early failure. Returning immediately when an
intermediate object key is absent would shorten a current error such as
`services.primary.database.pool` to `services.primary`; that would be a
regression even though the value is already known to be missing. The helper must
finish the label.

### Required proof

Add focused parity tests for:

- present object paths and numeric array segments;
- empty and one-segment paths;
- a missing first, middle, and final segment;
- an invalid array index;
- reaching a scalar before the path ends;
- deserialization failure at a nested value;
- empty segment text and other labels that demonstrate exact dot joining;
- a one-shot iterator, proving the implementation does not require a second
  traversal.

For every missing and deserialization-error case, assert the exact
`ConfigError` display text or stored `path`, not only the variant.

The full-document configuration benchmark does not isolate this code. Retain it
as a control and add a scalar typed path row with at least six borrowed
segments. Construct the config and path outside the timed body:

```bash
cargo test -p nidus-config

CARGO_TARGET_DIR=/tmp/nidus-config-baseline \
  cargo bench --bench configuration -- \
  'nidus config (deserialize 128 services|required typed path 6 segments)' \
  --warm-up-time 3 --measurement-time 8 --sample-size 200 --noplot

CARGO_TARGET_DIR=/tmp/nidus-config-candidate \
  cargo bench --bench configuration -- \
  'nidus config (deserialize 128 services|required typed path 6 segments)' \
  --warm-up-time 3 --measurement-time 8 --sample-size 200 --noplot
```

Count allocations if practical. Accept the optimization only if behavior is
exact and the focused path row shows deterministic allocation reduction or a
repeatable latency improvement without a meaningful control regression.

Assessment: **best isolated config candidate**. The current intermediate
ownership is explicit, the replacement is private, and error parity is directly
testable.

## Candidate 3: initialize imported modules before importers

### Current reliability risk

`ModuleGraph` stores definitions in `BTreeMap<String, ModuleDefinition>`
(`crates/nidus-core/src/module/graph.rs:36-40`). Its public `modules()` method
documents deterministic name order and returns `BTreeMap::values`
(`crates/nidus-core/src/module/graph.rs:111-119`). Rust documents that
[`BTreeMap::values` iterates in key order][rust-btreemap], so current bootstrap
order is lexicographic module-name order, not dependency order.

The graph already rejects cycles with a depth-first import traversal
(`crates/nidus-core/src/module/graph.rs:147-155,289-320`). Typed recursive
collection also visits imports before pushing the importer
(`crates/nidus-core/src/module/graph.rs:336-350`), but insertion into the
`BTreeMap` discards that traversal order.

Both async bootstrap drivers then iterate `graph.modules()`:

- core: `crates/nidus-core/src/app/mod.rs:133-139`;
- facade: `crates/nidus/src/app.rs:187-191`.

All synchronous registrars run before any async initializer, but an imported
module may create or replace a usable resource only in its async initializer.
An importer initializer that resolves that resource can therefore fail solely
because its name sorts before the dependency's name. For example,
`AImporterModule -> ZImportedDependencyModule` runs the importer first today.

### Proposed change

Compute and retain one deterministic dependency-first order while performing
the existing acyclicity traversal. Visit imports before the importer and visit
each module once. Root traversal retains stable module-name order, while each
module's imports retain their declared order.

Keep `ModuleGraph::modules()` in its documented name order. Give the validated
graph one shared provider-registration and async-initialization seam that both
core and facade bootstrap call, instead of exposing order and duplicating the
loops. Use dependency order for both phases: `ProviderRegistrant` receives
`&mut Container` and can resolve an imported provider just as an async
initializer can. The seam must guarantee:

- every module appears once;
- every direct and transitive import appears before its importer;
- traversal is deterministic;
- the graph is already validated and acyclic;
- all synchronous registrars finish before async initialization begins.

Do not parallelize initializers. They receive `&mut Container`, run sequentially
today, and may intentionally build on imported initialization.

This is a reliability contract, not a request-path optimization. It changes
startup order for graphs whose alphabetical and dependency orders differ, so it
requires explicit release notes even though it fixes dependency initialization.

### Required proof

Add tests for:

1. an importer whose name sorts before its imported dependency and whose
   initializer resolves a value registered by the dependency initializer;
2. the same graph through core lifecycle bootstrap and the facade builder;
3. a transitive chain and a diamond, proving each initializer runs once;
4. deterministic traversal for independent modules;
5. `ModuleGraph::modules()` still returns documented name order;
6. initializer failure still stops bootstrap and preserves the original error.

The existing 128-module graph-validation benchmark is the regression control
(`benches/dependency_resolution.rs:17-63`). Retaining the order adds one startup
vector plus module-name copies; that is acceptable only if bounded and
documented, and it must not be mislabeled as a speedup:

```bash
cargo test -p nidus-core
cargo test -p nidus-rs --all-features
cargo bench --bench dependency_resolution -- \
  'nidus 128-module graph validation' \
  --warm-up-time 3 --measurement-time 8 --sample-size 200 --noplot
```

Assessment: **implement as deterministic reliability hardening**. Claim only the
tested ordering guarantee, not lower latency or higher throughput.

## Rejected candidate: concrete future for public rate-limit service

### Why the allocation hypothesis is real

`RateLimitService<S>` is public, and its Tower implementation exposes:

```rust
type Future =
    Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
```

Both allowed and rejected calls return `Box::pin`
(`crates/nidus-http/src/middleware/rate_limit.rs:234-280`). The standard library
defines [`Box` as uniquely owning a heap allocation][rust-box], so a per-call
allocation is a reasonable hypothesis. Added allowed, rejected, and store-error
benchmark rows would be the right experiment controls.

### Why it is not a low-risk patch

Tower's `Service` contract makes `Future` a required associated type
([tower-service 0.3.3][tower-service]). Rust permits downstream code to name the
projection as `<RateLimitService<S> as Service<Request>>::Future`
([associated-type reference][rust-associated-types]). Replacing the boxed trait
object with an enum therefore changes a nameable public associated type; it is
not merely a private implementation detail.

The earlier note suggested a *private* concrete response-future enum. That is not
available for this public service: Rust rejects a private type in a public
associated-type signature with E0446
([official E0446 explanation][rust-e0446]). The enum would have to become another
public type. Downstream exact-type bounds or aliases could stop compiling when
the associated type changes, so the experiment is SemVer-sensitive even if
normal `.await` callers are unaffected.

A generic enum that stores arbitrary `S::Future` also needs sound pin projection
to poll that nested future. Rust's pinning documentation states that future
combinators usually need structural pinning for nested futures and that adding
projection requires unsafe code
([pin projections and structural pinning][rust-pin]). Nidus framework roots
forbid unsafe code. Tightening the service bound to `S::Future: Unpin` would
instead reject previously supported inner services. Boxing the inner future
inside the enum avoids projection but retains the allocation on the normal
allowed path.

Finally, Nidus already measured one concrete middleware-future rewrite: the
error-envelope success row regressed by 7.67%-9.62%, and the change was reverted
(`docs/performance.md:819-823`). That result does not prove every concrete future
will regress, but it does disprove any blanket rule that concrete futures are
automatically faster in this stack.

Decision: **reject for this pass**. Keep the boxed associated type and optimize
the independent static-identity allocation instead. Reconsider only at an
explicit SemVer boundary with a deliberately public response-future type,
allocation counts, code-size/compile-time measurements, all three branch
benchmarks, and no unsafe-code policy exception.

## Retained safety decisions and explicit non-candidates

### No default global concurrency limit, buffer, or load shed

Tower provides all three, but [Tower's layer-order documentation][tower-order]
shows that reversing buffer and concurrency layers changes total admitted work,
and load shedding turns unready state into errors. Nidus still has no universal
capacity value, overload response contract, or representative
healthy-traffic-under-overload benchmark. Do not add a default from a
single-request microbenchmark.

### No async-mutex swap for short data critical sections

The in-memory rate limiter holds `std::sync::Mutex` only during synchronous map
work (`crates/nidus-http/src/middleware/rate_limit.rs:103-140`), and the metrics
collector likewise performs non-async data updates while locked
(`crates/nidus-http/src/middleware/metrics.rs:259-323`). Tokio's official
[mutex-selection guidance][tokio-mutex] says the ordinary mutex is often
preferred for data when a guard is not held across `.await`; the async mutex is
more expensive because it supports that capability. Keep the current mutexes
unless concurrent contention, tail latency, and RSS evidence identify a real
problem.

### No blanket boxed-future rewrite or unsafe pin projection

The rate-limit analysis and the measured error-envelope regression are local
counterexamples to a blanket rewrite. Preserve `#![forbid(unsafe_code)]`; do not
introduce manual pin projection for a speculative allocation win.

### No eager-singleton default

`Container::eagerly_resolve_singletons` is already an explicit opt-in that
changes errors and panics from first resolution to startup
(`crates/nidus-core/src/container/mod.rs:178-197`). Enabling it automatically
would also execute every singleton factory even when an application never uses
that provider. This is observable lifecycle and side-effect behavior, not a
low-risk latency optimization. Keep lazy resolution as the default.

### No broad `String`-to-`Cow` migration

`Cow` is justified only where Nidus itself knows data is static and immutable.
Headers, user IDs, tenant IDs, IP strings, configuration labels, and adapter
payloads have different ownership lifetimes. Each would need its own allocation
profile and behavior proof; Candidate 1 is not evidence for a framework-wide
rewrite.

## Non-claims

- This note contains no benchmark result.
- Removing one temporary allocation does not prove a server-throughput or p99
  improvement.
- Fusing config traversal does not prove faster application startup; it measures
  one typed-read boundary.
- Dependency-first initialization is a reliability fix, not an optimization.
- A benchmark harness addition is not evidence until the untouched baseline and
  candidate are measured from independently compiled source in both orders.
- None of these candidates changes the need for Linux/x86_64, TLS/proxy,
  end-to-end latency, availability-under-load, or RSS qualification before
  making production-wide performance claims.

[cargo-semver-private]: https://doc.rust-lang.org/cargo/reference/semver.html#struct-private-fields-with-private
[rust-associated-types]: https://doc.rust-lang.org/reference/items/associated-items.html#associated-types
[rust-box]: https://doc.rust-lang.org/std/boxed/struct.Box.html
[rust-btreemap]: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html
[rust-cow]: https://doc.rust-lang.org/std/borrow/enum.Cow.html
[rust-e0446]: https://doc.rust-lang.org/error_codes/E0446.html
[rust-pin]: https://doc.rust-lang.org/std/pin/index.html#projections-and-structural-pinning
[rust-string]: https://doc.rust-lang.org/std/string/struct.String.html
[serde-json-value-de]: https://docs.rs/serde_json/1.0.151/src/serde_json/value/de.rs.html#817-876
[tokio-mutex]: https://docs.rs/tokio/1.53.1/tokio/sync/struct.Mutex.html#which-kind-of-mutex-should-you-use
[tower-order]: https://docs.rs/tower/0.5.3/tower/builder/struct.ServiceBuilder.html#order
[tower-service]: https://docs.rs/tower-service/0.3.3/tower_service/trait.Service.html
