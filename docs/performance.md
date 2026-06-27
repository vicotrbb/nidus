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
```

For a quick smoke run with reduced Criterion sampling:

```bash
cargo bench --bench dependency_resolution -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
cargo bench --bench routing -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
cargo bench --bench request_lifecycle -- --warm-up-time 0.1 --measurement-time 0.2 --sample-size 10
```

## Coverage

The current benchmark surface covers:

- singleton dependency resolution
- raw Axum route composition
- Nidus controller route composition
- raw Axum baseline request handling
- hello-world request handling and app construction
- controller plus injected service request handling and app construction
- controller setup
- guard middleware
- validation extraction
- request-scoped dependency resolution through HTTP
- per-layer middleware: security headers, body limit, legacy request ID,
  validated request ID, request context, error envelope, and timeout response
- production default stack with and without in-process metrics
- Prometheus metrics record-response, record-error, and render-text paths

The request lifecycle benchmark includes equivalent raw Axum request and routing
composition baselines where they are meaningful. Other rows are microbenchmarks
for specific framework behavior and should be compared to their own history.

## Local Results

These numbers are one local validation run, not a universal performance claim.
They were captured on 2026-06-25 at commit `4d19496` with `cargo bench`, `rustc
1.96.0 (ac68faa20 2026-05-25)`, `aarch64-apple-darwin`, and macOS 14.5
(`23F79`) on arm64 hardware. Criterion reported several outliers and mixed
regressions/improvements versus local saved history, including raw or unrelated
benchmarks moving in different directions. Treat the table as a current
reference point, not publish-grade proof.

One request-lifecycle scenario was added after that full-table capture:
`Nidus middleware legacy request ID request`. Its row is from the later Wave 28
`cargo bench --bench request_lifecycle` run and should be rerun with the full
surface before publishing updated performance claims.

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
