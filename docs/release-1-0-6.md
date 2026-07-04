# Release 1.0.6

Nidus 1.0.6 is a performance and reliability patch release. Every optimization
was landed as an isolated commit with either a Criterion A/B measurement or a
structural proof plus test coverage, and the request/response behavior of every
touched surface is unchanged.

## Dependency Injection

- Constructed singletons now resolve through a lock-free cache instead of
  re-acquiring the provider mutex on every resolution. Failed or panicking
  factories keep their retry semantics.
- The container provider map and per-request instance map hash `TypeId` keys
  with an identity hasher (the same approach `http::Extensions` uses), removing
  redundant SipHash work from every resolution.
- Measured locally: singleton dependency resolution 11.78 ns -> 3.96 ns.

## Request Middleware

- The validated request ID middleware validates inbound IDs against the
  borrowed header value, reuses the inbound `HeaderValue` instead of re-parsing
  it, and moves the final ID into the request context instead of cloning it.
- The request context layer returns the inner service future directly instead
  of boxing it, and trace IDs are extracted from `traceparent` without an
  intermediate allocation.
- The error envelope layer captures only the request ID instead of cloning the
  full request context on every request.
- Measured locally: production default stack 2.109 us -> 1.962 us (with
  metrics: 2.576 us -> 2.400 us).

## Metrics

- The in-process Prometheus collector stores requests, errors, and duration
  histograms in one series map keyed by method, route, and status, recording
  observations with zero label clones. Text exposition output is unchanged and
  remains covered by byte-level tests.
- Rendering writes directly into the output string, borrows clean label values,
  and uses precomputed histogram bucket labels kept in sync by test.
- Matched route patterns are carried as shared references instead of being
  copied per request.
- Measured locally: record response 126.7 ns -> 81.3 ns, record error 153.5 ns
  -> 81.6 ns, render text 30.1 us -> 7.2 us.

## Rate Limiting

- The in-memory store prunes expired identity windows at most once per window
  duration instead of sweeping every identity on every request. Rate-limit
  decisions never depended on that sweep; expired windows now linger at most
  one extra window, which affects only memory and `len()`.
- Rate limit response headers are built with infallible integer conversions.
- Measured locally: store check with 10,000 tracked identities 27.54 us ->
  32.4 ns; the single-identity request path is unchanged within noise.

## Reliability

- The guard service now moves the inner service that was driven to readiness
  into its response future, per the Tower readiness contract, instead of
  calling a fresh un-polled clone.
- Rate limit header construction no longer has `expect` paths.

## Benchmark Coverage

New Criterion rows: `nidus middleware rate limit request` and `nidus rate limit
store check with 10k identities`. All figures above are single-machine local
measurements (aarch64-apple-darwin) captured with the commands in
`docs/performance.md`; rerun them on release hardware before quoting numbers.

## Release Boundary

After publishing, verify the public package and documentation state:

```bash
bash scripts/verify-published-release.sh 1.0.6
```
