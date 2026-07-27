# Rust Framework Performance and Safety Research Follow-up - 2026-07-27

This note follows
[`rust-framework-performance-safety-research-2026-07-27.md`](rust-framework-performance-safety-research-2026-07-27.md).
It does not reopen that note's completed configuration, request-identity, or
dependency-first initialization work. It narrows the remaining review to typed
module discovery, request-scoped extraction, bounded dashboard serialization,
Tower readiness, and build-policy gates.

This is a candidate-selection document. Tests can prove behavior and graph
properties; only representative measurements can support a latency, allocation,
compile-time, or throughput claim.

## Inspected baseline

- Committed source inspected: `eb9796a95578`, plus the focused uncommitted test
  and benchmark harness additions present during the review.
- Local toolchain: `rustc 1.96.0` and `cargo 1.96.0`.
- Workspace: Rust edition 2024, MSRV 1.96, Cargo resolver 3.
- Locked relevant versions: Axum 0.8.9, Tower 0.5.3, tower-service 0.3.3,
  Tokio 1.53.1, serde_json 1.0.151, and Criterion 0.8.2.
- Sources were checked on 2026-07-27. Versioned documentation is linked where
  the contract is stack-version-sensitive.

Primary sources used for this follow-up:

- [Axum 0.8.9 `FromRequestParts`][axum-from-parts]
- [Rust Reference: async blocks][rust-async-block]
- [`std::future::ready`][rust-ready]
- [`Arc::clone` and shared ownership][rust-arc-clone]
- [`BTreeSet::insert`][rust-btreeset-insert]
- [serde_json 1.0.151 serialization API][serde-json]
- [serde_json `to_vec`][serde-json-to-vec]
- [serde_json `to_writer`][serde-json-to-writer]
- [`std::io::Write`][rust-io-write]
- [Tower `Service` readiness and cloning contract][tower-service]
- [Cargo dependency resolution, features, and resolver 3][cargo-resolver]
- [Cargo `check --locked`][cargo-check-locked]
- [Cargo `rust-version`][cargo-rust-version]
- [Rust `unsafe_code` lint][rust-unsafe-code]
- [Criterion comparison and noise guidance][criterion-output]
- [RUSTSEC-2023-0071][rustsec-rsa]

## Runtime boundary retained

Module collection and graph validation are startup work. The validated graph
then registers synchronous providers and awaits async provider initializers in
dependency-first order before the facade constructs its Axum router
(`crates/nidus-core/src/module/graph.rs:132-167`;
`crates/nidus/src/app.rs:164-184`). Ordinary requests do not traverse the
module graph.

Each HTTP request that has request-scoped DI enabled receives one
`Arc<RequestScope<'static>>` in its extensions
(`crates/nidus-http/src/middleware/request_scope.rs:46-67`). The
`RequestScoped<T>` extractor looks up that extension and resolves `T` through
the scope (`crates/nidus-http/src/request.rs:93-113`). A request-lifetime
provider is cached by `TypeId` in that request scope; singleton and transient
providers delegate to the application container
(`crates/nidus-core/src/container/request_scope.rs:57-76`).

Those boundaries prevent two invalid claims:

1. typed subgraph collection cannot be marketed as request-latency work; and
2. changing the extractor's future does not eliminate the scope allocation,
   provider allocation, cache insertion, or factory work that may occur during
   resolution.

## Decision matrix

| Candidate | Boundary | Decision |
| --- | --- | --- |
| Deduplicate shared recursively discovered typed subgraphs across explicit roots | Application graph construction | **Accept as deterministic reliability hardening** |
| Replace `RequestScoped<T>`'s lazy async block with `std::future::ready` | Extractor invocation | **Reject for this pass** |
| Count dashboard payload JSON through a bounded writer instead of materializing a temporary `Vec<u8>` | Opt-in dashboard payload capture | **Defer to a focused bounded-memory pass** |
| Make the feature-matrix `cargo check` commands locked | Build/CI policy | **Accept as deterministic CI hardening** |
| Rewrite public Tower service futures or add default admission middleware | Request middleware | **Reject without new representative proof** |

## Candidate 1: deduplicate shared typed subgraphs across explicit roots

### Baseline reliability defect

At the inspected `eb9796a95578` baseline,
`ModuleGraph::from_root_and_modules` recursively collected the root module with
a new `BTreeSet`, then created another new `BTreeSet` for every supplied
explicit definition (`crates/nidus-core/src/module/graph.rs:57-68` at that
commit). The recursive helper deduplicated by module name only within the set
supplied to that one traversal.

That works for a diamond reachable entirely from one typed root. It fails for a
valid mixed graph of this shape:

```text
StringOnlyRoot
├── ExplicitFeatureA ──typed──> SharedTypedDependency
└── ExplicitFeatureB ──typed──> SharedTypedDependency
```

The root has string-only imports, so callers correctly pass the two feature
definitions to `from_root_and_modules`, as documented in `docs/modules.md:43-62`.
Each explicit feature's fresh traversal set independently emits
`SharedTypedDependency`. `ModuleGraph::from_modules` then sees the same generated
module name twice and returns `NidusError::DuplicateModule`
(`crates/nidus-core/src/module/graph.rs:71-87`).

This is not an intentionally ambiguous explicit definition. It is one shared
typed dependency reached through two valid subgraphs. Nidus already treats a
shared typed dependency as one node when the diamond is reachable through one
root; changing behavior based only on whether the parents were supplied
explicitly is inconsistent.

### Implemented safe change boundary

Use one discovery set for recursively followed typed imports across the root and
all supplied definitions, while preserving every caller-supplied top-level
definition in the final input to `ModuleGraph::from_modules`.

The final implementation owns that shared set at
`crates/nidus-core/src/module/graph.rs:63-69` and separates recursive discovery
from explicit-definition handling at
`crates/nidus-core/src/module/graph.rs:387-423`.

That distinction is essential:

- a recursively discovered typed dependency already seen by another subgraph is
  emitted once;
- two caller-supplied definitions with the same name are still both emitted and
  still fail with `DuplicateModule`;
- a caller-supplied definition whose name was already followed from the typed
  root is still an explicit duplicate and still fails;
- import declarations remain on their parents, so the existing missing-import
  and cycle validators retain their authority;
- the existing dependency-order traversal remains unchanged.

`BTreeSet::insert` reports whether a value was newly inserted, so the existing
standard-library collection directly supports this distinction
([official documentation][rust-btreeset-insert]). No public type, method
signature, module name, error variant, provider lifecycle, or router behavior
needs to change.

Do not implement this as “one global set, silently skip every already-seen
top-level module.” That would weaken duplicate-definition validation and could
hide conflicting explicit metadata.

### Required proof

Add focused tests that prove:

1. two explicit feature modules can share one recursively typed dependency;
2. the shared dependency appears once in `ModuleGraph::modules()`;
3. its provider registrar and async initializer run once;
4. the shared dependency runs before both importers;
5. two duplicate explicit definitions still return `DuplicateModule`;
6. explicitly supplying a definition already followed from the typed root still
   returns `DuplicateModule`;
7. missing imports, cycles, and deterministic name iteration remain unchanged.

Run:

```bash
cargo test --locked -p nidus-core
cargo test --locked -p nidus-rs --all-features
cargo test --locked -p nidus-testing --all-features
```

The existing 128-module graph-validation row is a startup regression control:

```bash
CARGO_TARGET_DIR=/tmp/nidus-shared-subgraph-candidate \
  cargo bench --locked --bench dependency_resolution -- \
  'nidus 128-module graph validation' \
  --warm-up-time 2 --measurement-time 5 --sample-size 150 --noplot
```

The reproducer failed on the baseline with
`DuplicateModule { module: "SharedExplicitDependencyModule" }`; four duplicate
and missing-import controls passed in the same run. After the fix, all five
focused tests passed. The shared dependency's provider registrar ran once, its
async initializer ran once, and the recorded initialization order was
`shared`, `first`, `second`.

There is no honest before/after latency comparison for successful
shared-subgraph construction because the baseline rejects that graph. The
unchanged 128-module `from_modules` regression control measured
`43.366-43.770 us`, within the repository's 5% noise threshold of the prior
`42.078-42.424 us` result. Accept this as a correctness fix; do not claim a
graph-construction speedup unless a separate successful-equivalent benchmark is
designed and measured in both orders.

Assessment: **accept**. The failure follows directly from the fresh traversal
sets, the fix can preserve explicit duplicate diagnostics, and the behavior is
deterministically testable.

## Rejected candidate: synchronous `ready` future for `RequestScoped<T>`

### What the source actually proves

Axum 0.8.9 defines `FromRequestParts::from_request_parts` as returning
`impl Future`, not a boxed trait object
([official trait documentation][axum-from-parts]). Nidus currently returns an
`async move` block with no `.await`
(`crates/nidus-http/src/request.rs:93-113`).

The Rust Reference describes an async block as an anonymous future whose stored
state is roughly an enum containing values needed across suspension points
([official reference][rust-async-block]). There is no `Box`, `Box::pin`, or
other heap-owning future wrapper in this extractor. Therefore:

- the current **extractor future itself is not evidence of a per-call heap
  allocation**;
- first request-scoped provider construction may still allocate in the scope
  cache;
- `request_scope_layer` still allocates the shared request scope;
- the extension lookup currently clones one `Arc`, which changes the strong
  count but does not allocate a new `RequestScope`.

`std::future::ready` creates a named, immediately ready, `Unpin` future and is
functionally similar to `async {}` ([official documentation][rust-ready]).
Those type properties do not by themselves establish faster extraction.

### Why the proposed rewrite is not behavior-neutral

A synchronous implementation would need to resolve the provider before it can
construct `ready(result)`:

```rust
let result = parts
    .extensions
    .get::<SharedRequestScope>()
    .ok_or(RequestScopeRejection::MissingScope)
    .and_then(|scope| scope.inject::<T>().map(RequestScoped::new))
    .map_err(RequestScopeRejection::ResolutionFailed);
std::future::ready(result)
```

Today the extension `Arc` clone happens when `from_request_parts` is called, but
provider resolution happens only when the returned async future is polled. With
`ready(result)`, provider resolution, factory side effects, errors, and panics
happen while constructing the future. A direct caller can create and drop the
current future without resolving `T`; that would no longer be true.

Nidus provider factories are arbitrary synchronous callbacks and may construct
resources or mutate application-owned state. Moving their execution earlier is
an observable semantic change, not merely a future representation change.
Axum normally awaits extractors immediately, but `FromRequestParts` is a public
trait and the low-risk requirement must cover valid direct callers too.

### Existing repository evidence is already negative

The historical `nidus request-scoped route` benchmark does not exercise
`RequestScoped<T>`; its handler extracts `Extension<SharedRequestScope>` and
calls `resolve` directly (`benches/request_lifecycle.rs:225-248,474-494`).
A focused extractor row is therefore necessary before any future-shape claim.

More importantly, the repository already records two relevant negative results:

- a request-scope extractor experiment initially moved 2.36%-5.17% but was
  statistically unchanged when repeated (`p = 0.81`);
  `docs/performance.md:583-589`;
- another request-scope clone-avoidance experiment stayed inside Criterion's
  noise threshold and was reverted; `docs/performance.md:1220-1224`.

The focused row added during this pass confirmed why those results must remain
negative. An unchanged repeat moved both the direct-resolution control and the
real `RequestScoped<T>` extractor row by approximately 8.6% in the same
direction. That common-mode shift is environment drift, not evidence that
changing the extractor helped. The extractor-specific benchmark remains useful
as a future control; no runtime extractor change is retained.

Criterion explicitly cautions that system noise can produce apparent changes
and classifies changes overlapping the configured noise threshold as
insignificant ([official guidance][criterion-output]). Nidus uses a stricter 5%
repository threshold.

### What could be reconsidered later

A lazy future may borrow the `SharedRequestScope` from request extensions rather
than clone the `Arc`, preserving poll-time resolution. That is closer to
behavior-neutral than `ready`, but it is still the already unproven
clone-avoidance hypothesis. Reconsider it only with:

1. a handler that actually extracts `RequestScoped<T>`;
2. repeated cached and first-construction rows;
3. a direct test proving provider factories remain lazy until the future is
   polled;
4. missing-scope and resolution-error parity;
5. cancellation/drop parity;
6. A/B runs in both orders with isolated target directories;
7. an allocation or reference-count instrument if an allocation claim is made.

Suggested measurement:

```bash
CARGO_TARGET_DIR=/tmp/nidus-scope-baseline \
  cargo bench --locked --bench request_lifecycle -- \
  'nidus request-scoped (route|extractor route)' \
  --warm-up-time 2 --measurement-time 5 --sample-size 200 --noplot

CARGO_TARGET_DIR=/tmp/nidus-scope-candidate \
  cargo bench --locked --bench request_lifecycle -- \
  'nidus request-scoped (route|extractor route)' \
  --warm-up-time 2 --measurement-time 5 --sample-size 200 --noplot
```

Decision: **reject the synchronous `ready` rewrite**. It does not remove a
proven heap allocation, it changes lazy provider-resolution timing, and the
smaller clone-avoidance hypothesis has already failed repeatability.

## Candidate 2: make the dashboard payload cap bound serialization output memory

### Current bounded-memory gap

`DashboardCapture::payloads` advertises bounded payload capture and defaults to
16 KiB (`crates/nidus-dashboard/src/config.rs:76-103`). After redaction,
`cap_payload` serializes the entire value with `serde_json::to_vec`, checks the
vector length, drops it, and either retains the original `Value` or stores a
small truncation marker (`crates/nidus-dashboard/src/collector.rs:196-205`).

serde_json documents `to_vec` as producing a JSON `Vec<u8>`
([official documentation][serde-json-to-vec]). Therefore a payload with an
encoded size of `N` bytes can create an additional `O(N)` output buffer before
Nidus decides it exceeds a 16 KiB cap. The persisted payload is bounded, but the
transient serialization output is not.

This is an opt-in dashboard path, not the default HTTP request path. The caller
already owns a complete `serde_json::Value`, so this candidate cannot bound the
memory needed to construct that input. It can remove the additional full
serialized copy used only for length checking.

### Proposed low-risk change

Serialize through `serde_json::to_writer` into a private `std::io::Write`
implementation that:

- counts accepted bytes without storing them;
- uses checked arithmetic;
- marks itself exceeded and returns a private sentinel I/O error as soon as the
  next write would cross `max_bytes`;
- distinguishes that sentinel from any other serialization error.

serde_json documents `to_writer` as serializing into a supplied `Write`
implementation ([official documentation][serde-json-to-writer]); `Write`
receives byte slices and reports how many bytes were accepted
([standard-library contract][rust-io-write]). This lets Nidus preserve the exact
compact-JSON byte boundary without allocating a full output vector and stop
walking an oversized value after the boundary is crossed.

Keep `cap_payload` private and preserve:

- redaction before size evaluation;
- `encoded_len <= max_bytes` retaining the original redacted value;
- the exact `{"truncated":true,"max_payload_bytes":...}` marker above the cap;
- propagation of genuine serialization failures.

Do not replace the compact serialized-byte contract with an estimate based on
string lengths or `Value` node counts. Escaping, UTF-8 width, numbers, commas,
and object syntax make such estimates observably different.

### Required proof

Add exact tests for:

1. a value whose compact JSON is exactly at the limit;
2. the same value at one byte over;
3. `max_bytes == 0`;
4. multibyte UTF-8 and strings requiring JSON escaping;
5. arrays, objects, numbers, booleans, and null;
6. redaction changing the encoded length before the cap is applied;
7. the writer stopping after crossing the bound;
8. non-sentinel writer/serialization errors remaining errors.

Run:

```bash
cargo test --locked -p nidus-dashboard
cargo test --locked -p nidus-dashboard --all-features
```

If a performance claim is desired, first add a focused Criterion benchmark for
accepted and rejected 16 KiB and 1 MiB values. Compare isolated baseline and
candidate targets in both orders. The structural removal of the full output
`Vec` supports a bounded-transient-output-memory claim; it does not establish
overall process RSS, handler throughput, or end-to-end latency.

Assessment: **defer** to a focused bounded-memory pass. The current temporary
vector is explicit, byte parity is testable, and serde_json exposes the required
streaming writer API. Do not mix an unmeasured opt-in dashboard change into
this pass's deterministic graph and CI hardening.

## Accepted candidate 3: lock the feature-matrix checks

### Baseline reproducibility gap

Before this pass, the feature matrix's dependency assertions used
`cargo tree --locked`, but its `check` helper ran plain `cargo check`. Cargo may
update `Cargo.lock` when requirements require a new resolution. Cargo's
official `--locked` documentation says the flag asserts the exact locked
dependency versions and exits if Cargo would change the lockfile; it explicitly
identifies deterministic CI as a use case
([Cargo documentation][cargo-check-locked]).

The prior script could therefore compile a graph that Cargo had just changed
and then assert a locked graph afterward. A clean CI checkout made accidental
lock mutation less visible, not impossible.

### Implemented change and proof

The helper now runs `cargo check --locked "$@"`. Package selection and feature
combinations are unchanged. This changes no Rust API or runtime behavior.

Validate with:

```bash
bash scripts/check-integration-feature-matrix.sh
git diff --exit-code -- Cargo.lock
```

The workspace already uses resolver 3 and a workspace MSRV. Cargo documents
that resolver 3 changes incompatible-Rust-version handling to `fallback`, while
features for dependencies selected in one invocation are still unified
([resolver documentation][cargo-resolver]). The script is correct to use
separate package invocations for backend-isolation checks; `--locked` makes
those checks reproducible rather than changing their feature semantics.

Assessment: **accept** as a deterministic quality-gate improvement. The full
matrix passed and the `Cargo.lock` SHA-256 remained
`9be4f2c0258fefc18bad229b660de25eea393e40f6774a7f675bfe5d66eb033b`.
Do not claim a compile-time improvement.

## Tower readiness and future-allocation conclusions

Tower's `Service` contract requires callers to obtain readiness before `call`
and warns that a cloned service may not share the original service's ready
reservation ([official contract][tower-service]).

The inspected Nidus middleware services delegate `poll_ready` to the same inner
service. The guard middleware is the one path that must move an inner service
into an async future; it correctly replaces `self.inner` with a fresh clone and
moves the already-readied original into the future
(`crates/nidus-auth/src/middleware.rs:67-104`). Preserve that pattern.

Several public Nidus service implementations expose a boxed associated future,
including rate limiting, metrics, request IDs, guards, and error envelopes.
A box is a plausible per-call allocation, but changing the associated future of
a public `Service` implementation is SemVer-sensitive and concrete future state
machines can grow code size. The existing note also records an actual
error-envelope concrete-future regression and the rate-limit projection/API
constraints.

Decisions:

- **do not** clone an inner service after readiness and call the clone;
- **do not** blanket-rewrite boxed futures;
- **do not** use unsafe pin projection; all framework/tooling crate roots forbid
  unsafe code;
- **do not** add a default `Buffer`, `ConcurrencyLimit`, or `LoadShed` layer
  without a workload-specific capacity and overload-response contract;
- **do** require allowed, rejected, error, cancellation, future-size, allocation,
  and both-order latency evidence for any one future experiment.

No new repository evidence overturns the prior rejection.

## Feature, MSRV, and safety gates to retain

The current checkout has a coherent toolchain boundary:

- `[workspace.package] rust-version = "1.96"` (`Cargo.toml:110-117`);
- `rust-toolchain.toml` pins 1.96.0 with rustfmt and Clippy;
- CI installs and runs 1.96.0 (`.github/workflows/ci.yml:21-25`);
- the workspace uses resolver 3 (`Cargo.toml:1-43`);
- framework/tooling crate roots use `#![forbid(unsafe_code)]`;
- isolated feature checks cover facade and adapter default, no-default, and
  all-feature graphs;
- cargo-deny and cargo-audit are independent policy gates.

Cargo defines `rust-version` as the minimum supported Rust version and uses it
for compatibility diagnostics and resolution
([official documentation][cargo-rust-version]). Running the main suite on that
exact compiler is the meaningful MSRV proof. A second latest-nightly build is
not a substitute.

The `unsafe_code` lint detects unsafe blocks and other unsafe declarations;
`forbid` prevents nested code from lowering the lint
([official lint documentation][rust-unsafe-code]). Keep the crate-root policy
and its source audit:

```bash
rg --files-without-match '^#!\[forbid\(unsafe_code\)\]' \
  src/lib.rs crates/*/src/lib.rs crates/cargo-nidus/src/main.rs
rg -n '\bunsafe\b|no_mangle|export_name|link_section' \
  --glob '*.rs' --glob '!target/**' --glob '!fuzz/**' .
```

The dependency audit still needs its explicit RSA qualification.
RUSTSEC-2023-0071 was last modified on 2026-04-25 and still lists no patched
`rsa` release; it concerns remotely observable timing when private-key
operations use the affected crate ([RustSec][rustsec-rsa]). Keep the narrowly
reviewed audit exception visible and do not describe a passing wrapper script as
an advisory-free graph.

## Explicit rejections and non-claims

- Do not optimize dependency-order storage again from the current +2.68%
  startup-only control. The current implementation is deterministic and tested;
  another map-based implementation already regressed substantially.
- Do not replace request-scope synchronization with an async mutex. Provider
  factories and the public DI API are synchronous, and current critical
  sections do not cross `.await`.
- Do not claim that `RequestScoped<T>` is allocation-free overall. Its future is
  unboxed, but scope construction and first provider construction may allocate.
- Do not advertise the dashboard counting writer as bounding the caller's
  original `serde_json::Value`, total process RSS, or recursive nesting depth.
- Do not infer compile-time percentages from a smaller dependency graph.
- Do not infer server throughput or p95/p99 changes from startup, extractor, or
  serialization microbenchmarks.
- Do not enable a default streaming request-body cap, concurrency limit, buffer,
  or load shedding from focused microbenchmark evidence. Those change public
  behavior and require connection-recovery and mixed-load availability proof.

The accepted implementation set from this follow-up is deliberately small:
shared typed-subgraph deduplication that preserves explicit duplicate errors,
plus locked feature-matrix checks that make CI dependency resolution
reproducible.

The extractor-specific benchmark row is retained as proof infrastructure. The
synchronous `ready` rewrite is rejected, and the bounded dashboard writer
remains a documented follow-up rather than a bundled change.

[axum-from-parts]: https://docs.rs/axum/0.8.9/axum/extract/trait.FromRequestParts.html
[rust-async-block]: https://doc.rust-lang.org/reference/expressions/block-expr.html#async-blocks
[rust-ready]: https://doc.rust-lang.org/std/future/fn.ready.html
[rust-arc-clone]: https://doc.rust-lang.org/std/sync/struct.Arc.html#impl-Clone-for-Arc%3CT,+A%3E
[rust-btreeset-insert]: https://doc.rust-lang.org/std/collections/struct.BTreeSet.html#method.insert
[serde-json]: https://docs.rs/crate/serde_json/1.0.151
[serde-json-to-vec]: https://docs.rs/serde_json/1.0.151/serde_json/fn.to_vec.html
[serde-json-to-writer]: https://docs.rs/serde_json/1.0.151/serde_json/fn.to_writer.html
[rust-io-write]: https://doc.rust-lang.org/std/io/trait.Write.html
[tower-service]: https://docs.rs/tower-service/0.3.3/tower_service/trait.Service.html#backpressure
[cargo-resolver]: https://doc.rust-lang.org/cargo/reference/resolver.html
[cargo-check-locked]: https://doc.rust-lang.org/cargo/commands/cargo-check.html#manifest-options
[cargo-rust-version]: https://doc.rust-lang.org/cargo/reference/rust-version.html
[rust-unsafe-code]: https://doc.rust-lang.org/stable/nightly-rustc/rustc_lint/builtin/static.UNSAFE_CODE.html
[criterion-output]: https://bheisler.github.io/criterion.rs/book/user_guide/command_line_output.html
[rustsec-rsa]: https://rustsec.org/advisories/RUSTSEC-2023-0071.html
