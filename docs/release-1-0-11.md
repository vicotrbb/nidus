# Release 1.0.11

Nidus 1.0.11 is a public-API-compatible patch release focused on measurable
OpenAPI construction efficiency and more reliable application shutdown.

## OpenAPI allocation reduction

OpenAPI path normalization and operation-ID rendering now write directly into
one pre-sized `String`. The previous implementation created a temporary
`Vec<String>` and allocated per segment.

The same 100-route document construction and rendering benchmark was measured
before and after the change on one `aarch64-apple-darwin` machine with
`rustc 1.96.0`, 150 samples, a two-second warm-up, and a five-second measurement
window:

```bash
cargo bench --bench request_lifecycle -- 'nidus 100-route openapi document render' --warm-up-time 2 --measurement-time 5 --sample-size 150 --save-baseline pre_elite_20260712
cargo bench --bench request_lifecycle -- 'nidus 100-route openapi document render' --warm-up-time 2 --measurement-time 5 --sample-size 150 --baseline pre_elite_20260712
```

The saved baseline measured `341.66-351.44 us`. The confirming implementation
run measured `280.76-283.64 us`; Criterion reported an `18.40%-20.56%`
improvement with `p = 0.00`. This result is a local document-build
microbenchmark, not an HTTP throughput guarantee.

## Lifecycle shutdown reliability

Application shutdown now attempts every registered hook in reverse order even
when a hook fails. After all cleanup callbacks have run, the existing `Result`
API returns the first failure in shutdown order. This prevents one failing
adapter from skipping unrelated resource cleanup without changing the public
method signature or successful shutdown behavior.

Focused tests preserve path normalization, stable operation IDs, reverse-order
shutdown, and first-error reporting:

```bash
cargo test -p nidus-openapi path::tests
cargo test -p nidus-core --test lifecycle lifecycle_runner_attempts_remaining_shutdown_hooks_after_failure
```

## Verification boundary

The release candidate is checked with workspace formatting, strict Clippy,
all-feature unit and integration tests, rustdoc warnings as errors, dependency
policy and RustSec audits, semver checks, package file-list preflights, the
request-lifecycle benchmark suite, and standalone external examples using
temporary local patches.

After publication, verify all 25 registry artifacts, docs.rs pages, and the two
standalone external examples against crates.io with:

```bash
bash scripts/verify-published-release.sh 1.0.11
```
