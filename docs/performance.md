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
```

For a quick smoke run with reduced Criterion sampling:

```bash
cargo bench --bench dependency_resolution -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
cargo bench --bench routing -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
cargo bench --bench request_lifecycle -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
cargo bench --bench event_bus -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
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
  validated request ID, request context, error envelope, timeout response, and
  rate limit
- rate limit store check with 10,000 tracked identities
- structured logging span creation with request and trace headers
- bounded event publication at a full 10,000-event subscriber capacity
- production default stack with and without in-process metrics
- constructing an OpenAPI document with 64 distinct schemas
- serving a 100-route OpenAPI document
- Prometheus metrics record-response, record-error, and render-text paths

The request lifecycle benchmark includes equivalent raw Axum request and routing
composition baselines where they are meaningful. Other rows are microbenchmarks
for specific framework behavior and should be compared to their own history.

## Local Results

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
