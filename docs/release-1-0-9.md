# Release 1.0.9

Nidus 1.0.9 is a measured framework-construction performance and engineering
quality patch release. Public APIs remain compatible with 1.0.8; the retained
changes remove avoidable allocations and cumulative cloning without changing
request, provider, routing, or OpenAPI semantics.

## Route Construction

- Controller mount prefixes are normalized once per controller build instead
  of once for every stored route.
- Route parameter normalization writes into one pre-sized `String` instead of
  allocating a temporary string per segment, collecting a `Vec`, and joining a
  second output string.
- Existing root, trailing-slash, repeated-slash, braced-parameter, and empty
  parameter behavior has focused regression coverage.
- In the final local 150-sample Criterion comparison, 32-route controller
  construction improved from 24.195-24.331 us to 15.885-16.102 us, a
  34.1%-35.8% improvement interval.

## OpenAPI Schema Registration

- Schema registration now inserts directly into the document's owned
  `BTreeMap` instead of cloning every previously registered schema on each
  addition.
- The existing first-registration-wins behavior for duplicate schema names is
  preserved by a focused integration test.
- A new 64-schema construction benchmark improved from 177.04-179.30 us to
  14.324-14.425 us, a 92.0%-92.1% improvement interval in the final local
  150-sample comparison.

## Evidence Boundary

The benchmark figures above are local `aarch64-apple-darwin`, Rust 1.96.0
construction-time measurements, not universal request-throughput claims. A
request-context in-place refresh experiment produced inconsistent results and
regressed the metrics-enabled production stack by 9.8%-15.2% in the final
repeat; it was reverted and is not part of 1.0.9.

The release candidate is validated with:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --bench request_lifecycle --all-features
cargo test --bench routing --all-features
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
bash scripts/verify-published-release.sh 1.0.9
```
