# Release 1.0.14

Nidus 1.0.14 is a public-API-compatible patch release focused on removing
avoidable allocation and cloning from typed configuration reads and lifecycle
startup.

## Typed configuration reads

- Typed top-level, nested, and whole-document reads now deserialize directly
  from the stored `serde_json::Value` instead of cloning the complete value
  first.
- Required nested reads resolve their path once rather than rebuilding the same
  owned path inside a second lookup.
- Public method signatures, owned output types, error variants, and repeated
  read behavior are unchanged. Focused tests prove that borrowed deserialization
  does not consume or mutate the stored configuration.

Two paired 150-sample Criterion comparisons, run in opposite execution orders
against the exact `1.0.13` source commit, classified the 128-service
configuration row as 86.04%-86.74% faster. These are isolated in-memory
microbenchmarks, not application-throughput guarantees.

## Lifecycle startup bookkeeping

- Sequential startup no longer allocates a vector containing every successful
  hook index solely for failure rollback.
- When hook `n` fails, the existing sequential contract means hooks `0..n`
  completed; rollback now traverses that exact range in reverse.
- Expanded regression coverage preserves reverse-order shutdown for multiple
  successful hooks followed by a failure.

Two paired 150-sample Criterion comparisons, also run in opposite execution
orders, classified the 32-hook startup row as 18.48%-21.65% faster. Hooks that
perform substantial application work may see a much smaller relative effect.

## Verification boundary

The release candidate is checked with workspace formatting, warnings-denied
Clippy, all-feature tests, rustdoc warnings as errors, dependency policy and
RustSec audits, semver checks for every publishable library, package file-list
preflights, website verification, benchmark-harness compilation, and standalone
external examples using temporary local patches.

After publication, verify all 25 registry artifacts, docs.rs pages, and the two
standalone external examples against crates.io with:

```bash
bash scripts/verify-published-release.sh 1.0.14
```
