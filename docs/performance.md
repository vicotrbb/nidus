# Performance

Nidus aims to keep the framework layer visible and measurable instead of hiding
cost behind broad throughput claims. The benchmark suite is intentionally split
into dependency resolution, routing composition, and request lifecycle targets so
changes can be compared at the right boundary.

Run the full local benchmark surface with:

```bash
cargo bench --bench dependency_resolution
cargo bench --bench routing
cargo bench --bench request_lifecycle
cargo bench --bench event_bus
cargo bench --bench integration_hot_paths
cargo bench -p nidus-cache --bench cache_hot_paths
```

For a quick smoke run with reduced Criterion sampling:

```bash
cargo bench --bench dependency_resolution -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
cargo bench --bench routing -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
cargo bench --bench request_lifecycle -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
cargo bench --bench event_bus -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
cargo bench --bench integration_hot_paths -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
cargo bench -p nidus-cache --bench cache_hot_paths -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
```

## Coverage

The current benchmark surface covers:

- singleton dependency resolution
- raw Axum route composition
- Nidus controller route composition
- multi-route Nidus controller construction
- raw Axum baseline request handling
- hello-world request handling and app construction
- controller plus injected service request handling and app construction
- controller setup
- guard middleware
- validation extraction
- request-scoped dependency resolution through HTTP
- request context cloning
- per-layer middleware: security headers, body limit, legacy request ID,
  validated request ID, request context, error envelope, panic catching,
  timeout response, and rate limit
- rate limit store check with 10,000 tracked identities
- trusted-proxy identity extraction and extractor cloning
- structured logging span creation with request and trace headers
- bounded event publication at a full 10,000-event subscriber capacity, plus
  one- and four-subscriber fan-out controls
- production default stack with and without in-process metrics
- constructing an OpenAPI document with 64 distinct schemas
- separately constructing 8- and 100-route OpenAPI documents, constructing a
  100-route document from generated metadata, and rendering 100 routes
- serving a 100-route OpenAPI document
- Prometheus metrics record-response, record-error, and render-text paths
- shared integration envelope serialization/deserialization at a 1 KiB payload
- durable job validation/construction and retry-bound calculation
- lifecycle, adapter, and event observability recording with repeated bounded
  labels
- Moka cache hits with and without a configured key namespace

The integration benchmark uses a 5% Criterion noise threshold. A change beyond
that bound must be explained or reverted before release; smaller movement is
reported but is not treated as a regression, particularly for the sub-2 ns
retry arithmetic row.

The request lifecycle benchmark includes equivalent raw Axum request and routing
composition baselines where they are meaningful. Other rows are microbenchmarks
for specific framework behavior and should be compared to their own history.

## Local Results

### Moka cache key allocation pass (2026-07-14)

`MokaCacheProvider::get` and `invalidate` previously constructed an owned
`CacheKey` for every operation, even when no namespace was configured and Moka
could look up the existing `String` key through a borrowed `&str`. The
unnamespaced paths now pass that borrowed key directly. Namespaced keys still
own one string, but compose it into one exactly sized buffer instead of using
formatting machinery. Inserts remain owned because Moka must retain their keys.

The benchmark definition was added before the implementation change. Both
sides ran in the same dedicated target directory on the same
`aarch64-apple-darwin` machine with `rustc 1.96.0`, using 150 samples, a
two-second warm-up, and a five-second measurement window. Empty cached values
keep value cloning out of the measured key-path comparison:

```bash
CARGO_TARGET_DIR=target/cache-key-pass cargo bench -p nidus-cache --bench cache_hot_paths -- --save-baseline before-cache-key-pass-20260714 --noplot --warm-up-time 2 --measurement-time 5 --sample-size 150
CARGO_TARGET_DIR=target/cache-key-pass cargo bench -p nidus-cache --bench cache_hot_paths -- --baseline before-cache-key-pass-20260714 --noplot --warm-up-time 2 --measurement-time 5 --sample-size 150
```

| Benchmark | Before | First final-source run | Repeated final-source run |
| --- | ---: | ---: | ---: |
| Get without namespace | 172.49-175.69 ns | 139.48-143.27 ns (23.71%-33.17% faster) | 137.10-140.61 ns (26.96%-35.99% faster) |
| Get with namespace | 180.27-182.59 ns | 148.77-156.04 ns (18.09%-20.22% faster) | 145.21-146.15 ns (20.15%-21.44% faster) |

Criterion classified all four final comparisons as improvements (`p = 0.00`).
These are isolated in-process cache-hit measurements, not end-to-end request or
service-throughput claims. An initial `Cow<str>` implementation improved the
unnamespaced row but regressed the namespaced control by 11.53%-17.21%; it was
rejected in favor of the explicit branches and pre-sized owned-key path above.

### Observability label interning pass (2026-07-14)

The non-HTTP observability collector previously allocated fresh `String` keys
for every repeated event, job, and lifecycle observation. Adapter recording
also formatted the adapter and operation into one temporary string, split it
again, and allocated the resulting fields and status for both the counter and
histogram maps.

Stable dynamic labels are now interned as `Arc<str>` values and repeated
records clone only the shared reference. Static status values remain borrowed,
and adapter identity is a private typed pair of static strings. Besides removing
the recording-path allocations, the typed pair prevents two distinct adapter
labels containing `:` from being merged or rendered with the wrong boundary.
The public API, metric names, normal output ordering, and bounded-cardinality
overflow policy are unchanged.

The benchmark definitions were applied before the implementation change, then
measured on both sides in the same dedicated target directory. Both sides ran
on the same `aarch64-apple-darwin` machine with `rustc 1.96.0`, using 100
samples, a three-second warm-up, and a five-second measurement window:

```bash
CARGO_TARGET_DIR=target/observability-label-pass cargo bench --bench integration_hot_paths --all-features -- observability --save-baseline before-observability-labels --noplot --warm-up-time 3 --measurement-time 5 --sample-size 100
CARGO_TARGET_DIR=target/observability-label-pass cargo bench --bench integration_hot_paths --all-features -- observability --baseline before-observability-labels --noplot --warm-up-time 3 --measurement-time 5 --sample-size 100
```

| Benchmark | Before | Confirming implementation run | Criterion change |
| --- | ---: | ---: | ---: |
| Lifecycle record | 88.857-89.604 ns | 25.309-25.867 ns | 71.43%-72.04% faster |
| Adapter record | 197.55-200.70 ns | 41.503-42.046 ns | 78.79%-79.23% faster |
| Event record | 36.765-37.450 ns | 23.411-23.690 ns | 34.74%-36.27% faster |

Criterion classified all three changes as improvements (`p = 0.00`). Two
earlier implementation comparisons reported larger improvements; the table
uses the slower final-source run. These are repeated-label, in-process collector
microbenchmarks, not end-to-end service throughput claims.

Broader candidates were deliberately rejected for this pass. Allocator swaps,
blanket inlining, and release-profile tuning are workload or deployment
dependent and lack an isolated repository proof. Changing provider lifetimes,
eager singleton behavior, or HTTP lifecycle coupling would change semantics.
Previously measured boxed-future and event fan-out experiments were not retried
without new evidence after their control workloads regressed.

### Trusted-proxy identity pass (2026-07-14)

`trusted_proxy_client_ip_identity` previously copied the complete
`X-Forwarded-For` header before parsing one address and captured the configured
proxy list in a `Vec`, so cloning the extractor also cloned that allocation.
Forwarded values are now parsed in place, while the immutable proxy list is
shared as `Arc<[IpAddr]>`.

The trust algorithm now starts at Axum's connected peer and walks all forwarded
header values from right to left only while each hop is trusted. The first
non-trusted address is the client identity; malformed hops stop traversal at
the last verified address. Focused tests cover trusted multi-proxy chains,
attacker-controlled prefixes, split header fields, malformed values, and
untrusted direct peers.

The same `aarch64-apple-darwin` checkout and `rustc 1.96.0` used 150 samples, a
two-second warm-up, and a five-second measurement window. The extraction row
uses one trusted direct proxy and one forwarded client; the clone row uses an
eight-proxy configuration:

```bash
cargo bench --bench request_lifecycle --all-features -- 'nidus trusted proxy' --warm-up-time 2 --measurement-time 5 --sample-size 150 --save-baseline pre_trusted_proxy_chain_20260714
cargo bench --bench request_lifecycle --all-features -- 'nidus trusted proxy' --warm-up-time 2 --measurement-time 5 --sample-size 150 --baseline pre_trusted_proxy_chain_20260714
```

| Benchmark | Before | First implementation run | Repeated implementation run |
| --- | ---: | ---: | ---: |
| Client-IP extraction | 112.47-114.32 ns | 99.445-100.75 ns (8.88%-11.18% faster) | 98.781-100.97 ns (11.19%-12.64% faster) |
| Eight-proxy extractor clone | 15.533-16.226 ns | 3.3060-3.3782 ns (79.22%-79.95% faster) | 3.2071-3.2477 ns (79.57%-80.25% faster) |

Criterion classified all four comparisons as improvements (`p = 0.00`). These
are isolated in-process identity microbenchmarks, not end-to-end request or
server-throughput claims.

### OpenAPI method allocation pass (2026-07-14)

`OpenApiRoute` previously stored its fixed HTTP method as an owned `String`, so
each manual `get`, `post`, `put`, `patch`, or `delete` route paid for a heap
allocation. The private field now uses `Cow<'static, str>` and borrows those five
lowercase literals. Generated `RouteMetadata` uses the same borrowed literals
for the framework-supported uppercase/lowercase methods and retains an owned
lowercase fallback for uncommon methods.

The same `aarch64-apple-darwin` checkout and `rustc 1.96.0` used 100 samples, a
two-second warm-up, and a five-second measurement window:

```bash
cargo bench --bench request_lifecycle -- 'nidus 100-route openapi document construction' --warm-up-time 2 --measurement-time 5 --sample-size 100 --save-baseline before-openapi-index-100-20260714
cargo bench --bench request_lifecycle -- 'nidus 100-route openapi document construction' --warm-up-time 2 --measurement-time 5 --sample-size 100 --baseline before-openapi-index-100-20260714
cargo bench --bench request_lifecycle -- 'nidus 8-route openapi document construction' --warm-up-time 2 --measurement-time 5 --sample-size 100 --save-baseline before-openapi-index-8-20260714
cargo bench --bench request_lifecycle -- 'nidus 8-route openapi document construction' --warm-up-time 2 --measurement-time 5 --sample-size 100 --baseline before-openapi-index-8-20260714
cargo bench --bench request_lifecycle -- 'nidus 100-route openapi metadata construction' --warm-up-time 2 --measurement-time 5 --sample-size 100 --save-baseline before-openapi-metadata-cow-20260714
cargo bench --bench request_lifecycle -- 'nidus 100-route openapi metadata construction' --warm-up-time 2 --measurement-time 5 --sample-size 100 --baseline before-openapi-metadata-cow-20260714
```

| Benchmark | Before | After | Criterion change |
| --- | ---: | ---: | ---: |
| 8-route manual construction | 2.7585-2.8047 us | 2.6277-2.6494 us | 4.23%-5.72% faster |
| 100-route manual construction | 40.022-40.766 us | 37.607-38.027 us | 5.69%-7.87% faster |
| 100-route generated-metadata construction | 31.437-31.650 us | 31.245-31.552 us | no change detected |

Criterion classified both manual-route comparisons as improvements (`p =
0.00`). The generated-metadata control was statistically unchanged (`p =
0.90`, change interval -0.55% to +0.50%), so no generated-metadata performance
claim is made. Tests cover all five manual builders, generated metadata,
duplicate operations after many routes and cloning, and the uncommon-method
lowercase fallback.

Several broader experiments were rejected. An event-bus single-subscriber
inline collector improved that row by more than 58%, but its best fan-out-safe
shape still regressed four subscribers by 3.87%-5.80%. A lazy OpenAPI hash index
improved 100 routes but regressed 16 routes by 11.63%-13.90% and 32 routes by
9.90%-11.36%. Route/schema capacity reservations stayed within Criterion's
noise threshold. All three experiments were reverted instead of being presented
as general optimizations.

### Request-context correlation fallback pass (2026-07-14)

`RequestContext::from_parts` previously cloned the final request-ID `String`
when no explicit `x-correlation-id` header was present. That is the normal
production path, and the clone existed only so `correlation_id()` could return
the same bytes. The private context representation now records that the
correlation ID refers to the existing request ID, while preserving the public
constructor and getter behavior for explicit, fallback, and absent values.

The saved baseline was built from a detached worktree at the exact pre-change
commit (`6dad920`). The edited checkout then used the same target directory and
Criterion baseline. Both sides used 200 samples, a five-second warm-up, and a
ten-second measurement window on the same `aarch64-apple-darwin` machine with
`rustc 1.96.0`:

```bash
# Run from the detached 6dad920 worktree.
CARGO_TARGET_DIR="$NIDUS_TARGET" cargo bench --bench request_lifecycle --all-features -- --save-baseline context-clean-before-20260714 --warm-up-time 5 --measurement-time 10 --sample-size 200 'nidus (middleware request context|api defaults production) request'

# Run twice from the edited checkout.
CARGO_TARGET_DIR="$NIDUS_TARGET" cargo bench --bench request_lifecycle --all-features -- --baseline context-clean-before-20260714 --warm-up-time 5 --measurement-time 10 --sample-size 200 'nidus (middleware request context|api defaults production) request'
```

| Benchmark | Before | First implementation run | Repeated implementation run |
| --- | ---: | ---: | ---: |
| Request-context middleware | 909.95-923.93 ns | 861.19-865.31 ns (6.35%-7.96% faster) | 871.02-879.19 ns (4.51%-6.28% faster) |
| Production defaults | 2.3833-2.4454 us | 2.2166-2.2481 us (6.96%-9.79% faster) | 2.2055-2.2262 us (7.83%-10.59% faster) |

Criterion classified all four comparisons as improvements (`p = 0.00`). These
are local in-process request measurements, not end-to-end throughput claims.
Focused tests also inspect the private fallback state, deterministically proving
that it carries no second owned `String`.

An additional experiment replaced the error-envelope service's boxed future
with a concrete `futures-util` composition. A clean detached-worktree A/B moved
the successful-response row from `671.67-676.20 ns` to `719.08-733.51 ns`, a
`7.67%-9.62%` regression (`p = 0.00`). That experiment was reverted; the public
Tower service future and the measured implementation remain unchanged.

### Guard route-label sharing pass (2026-07-14)

`GuardLayer`, `GuardService`, and `GuardContext` previously retained route
labels as owned `String` values. Router/service cloning and every guarded
request therefore cloned an immutable label. They now retain one `Arc<str>` and
clone only its reference count. Macro-generated container-composed guards also
retain one shared route label per route and move the final synthetic header map
into the last guard context instead of cloning it again.

The existing explicit guard row was measured immediately before and after the
change on the same `aarch64-apple-darwin` machine with `rustc 1.96.0`. Both
sides used 150 samples, a two-second warm-up, and a five-second measurement
window:

```bash
cargo bench --bench request_lifecycle -- 'nidus guarded route' --warm-up-time 2 --measurement-time 5 --sample-size 150 --save-baseline pre_guard_label_arc_20260714
cargo bench --bench request_lifecycle -- 'nidus guarded route$' --warm-up-time 2 --measurement-time 5 --sample-size 150 --baseline pre_guard_label_arc_20260714
```

The saved baseline measured `886.93-927.27 ns`. The immediate implementation
run measured `687.71-710.61 ns`, with Criterion reporting an
`18.33%-22.87%` improvement (`p = 0.00`). A later repeat measured
`629.59-647.08 ns` and was also classified as an improvement. These are local
in-process route measurements, not end-to-end throughput claims.

A new `nidus module-composed guarded route` benchmark separately covers the
generated container path:

```bash
cargo bench --bench request_lifecycle -- 'nidus module-composed guarded route' --warm-up-time 2 --measurement-time 5 --sample-size 150 --save-baseline pre_macro_guard_context_20260714
cargo bench --bench request_lifecycle -- 'nidus module-composed guarded route' --warm-up-time 2 --measurement-time 5 --sample-size 150 --baseline pre_macro_guard_context_20260714
```

Its repeated latency comparison moved only
`0.45%-1.61%` lower and remained within Criterion's noise threshold, so no
latency improvement is claimed for that row. The source change deterministically
removes the final header-map clone and per-request label allocation, while a
two-guard runtime test proves that ordered checks still receive request headers.

### HTTP success-path middleware pass (2026-07-13)

The production error envelope previously allocated owned path and request-ID
strings before knowing whether the response was an error. It now retains the
request metadata and creates those strings only for 4xx/5xx responses. The
panic-catching layer previously boxed its response future for every request;
it now composes the existing concrete `futures-util` future types while
preserving both synchronous `Service::call` and asynchronous polling panic
handling.

The benchmark definitions were identical on both sides. Both measurements ran
on the same `aarch64-apple-darwin` machine with `rustc 1.96.0`, using 150
samples, a two-second warm-up, and a five-second measurement window:

```bash
CARGO_TARGET_DIR=target/quality-pass cargo bench --bench request_lifecycle -- 'nidus middleware (error envelope|catch panic) success request' --warm-up-time 2 --measurement-time 5 --sample-size 150 --save-baseline pre_http_success_paths
CARGO_TARGET_DIR=target/quality-pass cargo bench --bench request_lifecycle -- 'nidus middleware (error envelope|catch panic) success request' --warm-up-time 2 --measurement-time 5 --sample-size 150 --baseline pre_http_success_paths
```

| Benchmark | Before | After | Criterion change |
| --- | ---: | ---: | ---: |
| Error envelope, successful response | 711.61-725.38 ns | 666.96-700.17 ns | 4.98%-7.21% faster |
| Panic catcher, non-panicking response | 598.33-607.41 ns | 554.40-559.28 ns | 6.66%-8.19% faster |

Criterion classified both changes as improvements (`p = 0.00`). These are
isolated in-process middleware measurements, not end-to-end server throughput
claims. Existing error-envelope behavior tests and both synchronous-call and
future-poll panic regression tests remain green.

### Prometheus label interning pass (2026-07-13)

The in-process collector previously converted both the HTTP method and route
label into new `String` values on request start and completion. It now keeps
`http::Method` as the map key and interns each admitted route as an `Arc<str>`,
including one shared overflow label for capped collectors. The public API,
rendered metric names and labels, and route-cardinality policy are unchanged.

The same three existing Criterion rows were measured before and after the
change on the same `aarch64-apple-darwin` machine with `rustc 1.96.0`. Both
sides used 150 samples, a two-second warm-up, and a five-second measurement
window:

```bash
cargo bench --bench request_lifecycle -- 'nidus (metrics record response|metrics record inner error|api defaults production with metrics request)' --warm-up-time 2 --measurement-time 5 --sample-size 150 --save-baseline pre_quality_20260713
cargo bench --bench request_lifecycle -- 'nidus (metrics record response|metrics record inner error|api defaults production with metrics request)' --warm-up-time 2 --measurement-time 5 --sample-size 150 --baseline pre_quality_20260713
```

| Benchmark | Before | After | Criterion change |
| --- | ---: | ---: | ---: |
| Production defaults with metrics | 3.4368-3.5288 us | 2.6430-2.6622 us | 22.44%-24.35% faster |
| Metrics record response | 117.25-121.13 ns | 48.162-48.397 ns | 60.37%-61.42% faster |
| Metrics record inner error | 117.54-122.23 ns | 48.725-48.935 ns | 56.66%-57.84% faster |

Criterion classified all three changes as improvements (`p = 0.00`). These are
local request-lifecycle and collector microbenchmarks, not end-to-end server
throughput claims.

### OpenAPI path and operation-ID allocation pass (2026-07-12)

OpenAPI path normalization and operation-ID rendering were changed from a
temporary `Vec<String>` plus per-segment strings to one pre-sized output
`String`. A new 100-route construction/render row was applied identically
before and after the implementation change. Both sides used 150 samples, a
two-second warm-up, and a five-second measurement window on the same
`aarch64-apple-darwin` machine with `rustc 1.96.0`:

```bash
cargo bench --bench request_lifecycle -- 'nidus 100-route openapi document render' --warm-up-time 2 --measurement-time 5 --sample-size 150 --save-baseline pre_elite_20260712
cargo bench --bench request_lifecycle -- 'nidus 100-route openapi document render' --warm-up-time 2 --measurement-time 5 --sample-size 150 --baseline pre_elite_20260712
```

The saved baseline measured `341.66-351.44 us`. The confirming implementation
run measured `280.76-283.64 us`, with Criterion reporting an
`18.40%-20.56%` improvement (`p = 0.00`). This is a local document-build
microbenchmark, not an HTTP throughput claim.

### First-party integration baseline (2026-07-11)

The new envelope and durable-job hot paths were captured and immediately
repeated on the same `aarch64-apple-darwin` machine with `rustc 1.96.0`, 100
samples, a one-second warm-up, and a three-second measurement window:

```bash
cargo bench --bench integration_hot_paths -- --warm-up-time 1 --measurement-time 3 --sample-size 100 --save-baseline integration_initial
cargo bench --bench integration_hot_paths -- --warm-up-time 1 --measurement-time 3 --sample-size 100 --baseline integration_initial
```

| Benchmark | Initial interval | Confirming interval | Result |
| --- | ---: | ---: | --- |
| Envelope serialize, 1 KiB | 558.23-562.54 ns | 560.33-563.86 ns | no change |
| Envelope deserialize, 1 KiB | 534.84-550.46 ns | 525.53-527.74 ns | within threshold, lower |
| Durable job validate/construct, 1 KiB | 1.3829-1.3907 us | 1.3754-1.3985 us | within threshold |
| Retry bound calculation | 1.4657-1.4753 ns | 1.3555-1.3609 ns | improved |

An intervening identical-binary sample moved the 1.5 ns retry arithmetic row
by 2.8%, demonstrating why this suite uses an explicit 5% noise threshold. The
confirming run had no regressions beyond that threshold.

The final release-state run repeated the same command after all service and
failure-path gates. A pre-final sample had reported a 6.47% regression in job
construction, so it was not waived: payload validation was changed from
serializing into a temporary `Vec` to an allocation-free bounded counting
writer while preserving the exact 1 MiB serialized-size limit. A 150-sample
isolated confirmation then reported `[1.4033 us, 1.4258 us]`, a statistically
insignificant `+0.75%` (`p = 0.31`). The complete final run reported:

| Benchmark | Final interval | Change from saved baseline | Result |
| --- | ---: | ---: | --- |
| Envelope serialize, 1 KiB | 575.80-580.08 ns | +2.15% | within threshold |
| Envelope deserialize, 1 KiB | 538.50-540.29 ns | +0.91% | within threshold |
| Durable job validate/construct, 1 KiB | 1.3464-1.3521 us | -4.17% | within threshold |
| Retry bound calculation | 1.4157-1.4317 ns | -2.81% | within threshold |

No final row exceeded the documented 5% release threshold.

### 1.0.9 routing and OpenAPI builder pass (2026-07-10)

Repeated path normalization and cumulative OpenAPI schema-map cloning were
measured with saved Criterion baselines on the same `aarch64-apple-darwin`
machine and `rustc 1.96.0`. Each reported row used 150 samples. The benchmark
definition was identical on both sides of each comparison.

```bash
cargo bench --bench request_lifecycle -- 'nidus (32-route controller app|middleware request context request|api defaults production request|api defaults production with metrics request)' --warm-up-time 2 --measurement-time 5 --sample-size 150 --save-baseline pre_elite_pass
cargo bench --bench request_lifecycle -- 'nidus 64-schema openapi document construction' --warm-up-time 2 --measurement-time 5 --sample-size 150 --save-baseline pre_elite_pass
cargo bench --bench request_lifecycle -- 'nidus (32-route controller app|64-schema openapi document construction)' --warm-up-time 2 --measurement-time 5 --sample-size 150 --baseline pre_elite_pass
```

| Benchmark | Before | After | Criterion change |
| --- | ---: | ---: | ---: |
| 32-route controller construction | 24.195-24.331 us | 15.885-16.102 us | 34.1%-35.8% faster |
| 64-schema OpenAPI construction | 177.04-179.30 us | 14.324-14.425 us | 92.0%-92.1% faster |

The routing change normalizes a controller mount prefix once and joins route
paths that were already normalized by `RouteDefinition`. Path normalization
also writes into one pre-sized `String` rather than allocating a temporary
`String` for every segment plus a `Vec` and joined output. The OpenAPI change
uses the document's owned `BTreeMap` entry API directly, preserving its existing
first-registration-wins rule without cloning accumulated schemas.

A request-context in-place refresh was also tested against the production
defaults rows. Repeated comparisons were inconsistent: one isolated run showed
a small improvement, while a later run found no significant change without
metrics and a 9.8%-15.2% regression with metrics. The experiment was reverted
and is not included in this pass.

### 1.0.8 framework hot-path pass (2026-07-10)

Controller assembly, request-context cloning, and immutable OpenAPI responses
were measured with saved Criterion baselines on the same
`aarch64-apple-darwin` machine and `rustc 1.96.0`. Each row used 150 samples.
The new benchmark rows were applied identically to the pre-change and current
source trees; the request-stack baseline was rebuilt from the pre-change commit
before comparison.

```bash
cargo bench --bench request_lifecycle -- 'nidus (hello-world app|controller \+ service app|controller setup|guarded route|rate limit store check with 10k identities)' --sample-size 150 --save-baseline pre_nidus_quality
cargo bench --bench request_lifecycle -- 'nidus (32-route controller app|request context clone|100-route openapi json request)' --sample-size 150 --save-baseline pre_nidus_quality
cargo bench --bench request_lifecycle -- 'nidus (hello-world app|32-route controller app|request context clone|100-route openapi json request)' --sample-size 150 --baseline pre_nidus_quality

cargo bench --bench request_lifecycle -- 'nidus (middleware request context request|api defaults production request|api defaults production with metrics request)' --sample-size 150 --save-baseline pre_context_stack
cargo bench --bench request_lifecycle -- 'nidus (middleware request context request|api defaults production request|api defaults production with metrics request)' --sample-size 150 --baseline pre_context_stack
```

| Benchmark | Before | After | Criterion change |
| --- | ---: | ---: | ---: |
| Hello-world controller construction | 1.9211-1.9452 us | 1.0304-1.0447 us | 45.9%-46.7% faster |
| 32-route controller construction | 52.792-53.242 us | 23.881-23.947 us | 55.1%-56.2% faster |
| Request context clone | 92.385-92.924 ns | 3.6536-3.6717 ns | 96.0%-96.1% faster |
| 100-route OpenAPI JSON response | 188.42-190.58 us | 519.09-520.22 ns | 99.7% faster |
| Request context middleware | 973.37-1009.1 ns | 896.81-901.10 ns | 3.8%-6.0% faster |
| Production defaults | 2.6012-2.6602 us | 2.2908-2.3400 us | 14.7%-17.2% faster |
| Production defaults with metrics | 2.9016-2.9701 us | 2.6952-2.7014 us | 7.1%-8.7% faster |

These are local latency and construction-time measurements, not universal
throughput claims. A borrowed-key rate-limit lookup and a request-scope clone
avoidance experiment were also measured. The former regressed its benchmark
and the latter stayed within Criterion's noise threshold, so both were
reverted.

### 1.0.7 optimization pass (2026-07-09)

The bounded event queue and structured logging changes were measured with saved
Criterion baselines on the same `aarch64-apple-darwin` machine and `rustc
1.96.0`. The intervals below are Criterion's 100-sample estimates; they are
local evidence, not universal throughput claims.

```bash
cargo bench --bench event_bus -- 'nidus bounded event publish at 10k capacity' --warm-up-time 2 --measurement-time 5 --sample-size 100 --save-baseline before-event-queue
cargo bench --bench event_bus -- 'nidus bounded event publish at 10k capacity' --warm-up-time 2 --measurement-time 5 --sample-size 100 --baseline before-event-queue

cargo bench --bench request_lifecycle -- '(rate limit store check with 10k identities|structured logging span creation)' --warm-up-time 2 --measurement-time 5 --sample-size 100 --save-baseline before-http-hotpaths
cargo bench --bench request_lifecycle -- 'structured logging span creation' --warm-up-time 2 --measurement-time 5 --sample-size 100 --baseline before-http-hotpaths
```

| Benchmark | Before | After | Criterion change |
| --- | ---: | ---: | ---: |
| Bounded event publish at 10k capacity | 1.0719-1.1939 us | 67.098-78.214 ns | 93.9%-94.7% faster |
| Structured logging span creation | 158.54-159.89 ns | 83.327-86.991 ns | 37.9%-42.0% faster |

The rate-limit row was included in the saved HTTP baseline to evaluate a
borrowed-key lookup experiment. That experiment regressed the measured row and
was reverted; the rate-limit implementation is unchanged by this pass.

### Historical reference

These numbers are one local validation run, not a universal performance claim.
They were captured on 2026-06-25 at commit `4d19496` with `cargo bench`, `rustc
1.96.0 (ac68faa20 2026-05-25)`, `aarch64-apple-darwin`, and macOS 14.5
(`23F79`) on arm64 hardware. Criterion reported several outliers and mixed
regressions/improvements versus local saved history, including raw or unrelated
benchmarks moving in different directions. Treat the table as a current
reference point, not publish-grade proof.

The 1.0.6 optimization pass re-measured the rows it changed; see
`docs/release-1-0-6.md` for the per-change before/after figures and
methodology (stash-based A/B runs on the same machine). The table below
remains a compact reference that mixes the original full-table capture with
later follow-up runs and predates the 1.0.6 improvements for the affected
rows.

Headline 1.0.6 deltas measured locally on 2026-07-03/04:

- singleton dependency resolution: 11.78 ns -> 3.96 ns
- production default stack request: 2.109 us -> 1.962 us
- Prometheus record response: 126.7 ns -> 81.3 ns; render text: 30.1 us ->
  7.2 us
- rate limit store check with 10k identities: 27.54 us -> 32.4 ns

| Benchmark | Central estimate | Notes |
| --- | ---: | --- |
| Nidus singleton dependency resolution | 24.944 ns | direct container lookup |
| raw Axum route composition | 1.9600 us | composition baseline |
| Nidus controller route composition | 5.7504 us | controller route builder path |
| raw Axum baseline request | 688.30 ns | request baseline |
| Nidus hello-world request | 716.20 ns | simple controller request |
| Nidus hello-world app | 3.2809 us | app construction microbenchmark |
| Nidus controller + service request | 777.87 ns | injected service route |
| Nidus controller + service app | 3.6975 us | app construction with DI setup |
| Nidus controller setup | 267.71 ns | controller builder setup |
| Nidus guarded route | 1.1572 us | authorization guard layer |
| Nidus validation route | 2.3850 us | validation extractor path |
| Nidus request-scoped route | 1.5848 us | request-scoped provider resolution |
| Nidus middleware security headers request | 1.1748 us | response header layer |
| Nidus middleware body limit request | 858.22 ns | declared `Content-Length` check |
| Nidus middleware legacy request ID request | 1.4592 us | Wave 28 follow-up run; legacy generated UUID layer |
| Nidus middleware validated request ID request | 1.6663 us | strict UUID request ID layer |
| Nidus middleware request context request | 1.3570 us | request context layer |
| Nidus middleware error envelope success request | 1.0651 us | success path through envelope layer |
| Nidus middleware timeout response request | 982.22 ns | non-timeout success path |
| Nidus API defaults production request | 3.4495 us | production stack without metrics |
| Nidus API defaults production with metrics request | 4.2138 us | production stack with metrics hook |
| Nidus metrics record response | 243.52 ns | in-process Prometheus collector |
| Nidus metrics record inner error | 292.54 ns | error path recording |
| Nidus metrics render text | 53.494 us | renders 10 routes with 100 samples each |

## Reading Results

Default request handling does not resolve the dependency graph per request.
Request-scoped providers, validation, guards, production defaults, and metrics
are opt-in layers with measurable costs. The in-process Prometheus collector is
useful for examples, local services, and tests, but high-cardinality route labels
increase render output and memory use; prefer stable route patterns such as
`/users/{id}` over concrete IDs.

Before publishing performance claims, rerun these benchmarks on the release
machine, include equivalent raw Axum baselines where relevant, preserve
Criterion output, and report noise, outliers, and tradeoffs directly.
