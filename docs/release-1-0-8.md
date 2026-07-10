# Release 1.0.8

Nidus 1.0.8 is a measured framework hot-path and engineering-quality patch
release. Public APIs remain compatible with 1.0.7; the changes optimize router
construction and immutable request metadata or response bodies without changing
the framework's request, provider, or middleware semantics.

## Controller Assembly

- Controller routes now mount directly into their destination Axum router
  instead of allocating a temporary router and merging it for every route.
- Same-path handlers for different HTTP methods remain supported and have
  focused integration coverage.
- In the final local 150-sample Criterion comparison, hello-world controller
  construction improved by 45.9%-46.7%, and 32-route controller construction
  improved by 55.1%-56.2%.

## Request Context

- Cloned request contexts now share immutable string metadata through `Arc`.
- Consuming enrichment methods retain independent-value semantics through
  `Arc::make_mut` copy-on-write behavior.
- Public constructors, getters, equality, and the existing `const` getters are
  unchanged; semver checks found no public API break.
- Request-context clone time improved by 96.0%-96.1% locally. Composed request
  stacks improved by 3.8%-17.2% across the measured configurations.

## OpenAPI Responses

- OpenAPI JSON and Swagger UI HTML are rendered once when the router is built
  and served from reference-counted byte buffers thereafter.
- JSON structure and the existing JSON and HTML response media types remain
  covered by integration tests.
- The 100-route OpenAPI JSON benchmark improved from 188.42-190.58 us to
  519.09-520.22 ns locally.

## Evidence Boundary

The benchmark figures above are local `aarch64-apple-darwin`, Rust 1.96.0
measurements, not universal throughput claims. A borrowed-key rate-limit
experiment regressed its target benchmark, and a request-scope clone experiment
remained within Criterion noise; both were reverted and are not part of 1.0.8.

The release candidate is validated with:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --bench request_lifecycle --all-features
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps
cargo deny check
cargo audit --deny warnings
bash scripts/semver-check-publishable-crates.sh
bash scripts/package-publishable-crates.sh --list-only
NIDUS_EXTERNAL_EXAMPLES_LOCAL_PATCH=1 bash scripts/verify-external-examples.sh
npm --prefix website run verify
cargo +nightly fuzz build
```

After publishing, verify every crate, docs.rs page, and standalone example:

```bash
bash scripts/verify-published-release.sh 1.0.8
```
