# Release 1.0.15

Nidus 1.0.15 is a public-API-compatible patch release focused on dependency,
CI automation, macro compatibility, and security-scanning maintenance.

## Compatible dependency and automation updates

- Compatible Rust dependencies were refreshed, including Criterion 0.8,
  Redis 1.4, Tokio 1.53, Sentry 0.48.5, and current patch releases across the
  workspace.
- Pinned GitHub Actions were updated to their current release commits for
  checkout, caching, Node setup, artifact upload, and OpenSSF Scorecard.
- The fuzz lockfile now records the current Nidus workspace version, and an
  unused direct dependency was removed from the durable SQLx jobs adapter.

## Syn 3 macro compatibility

- Internal macro parsing now uses Syn 3 while keeping Nidus's generated API and
  route contracts unchanged.
- Route receiver validation was adapted to Syn's receiver representation and
  now has focused regression coverage proving that route methods accept only
  shared `&self` receivers, rejecting by-value, mutable, and typed receivers.

## Security-scanning policy

- The reviewed `RUSTSEC-2023-0071` non-applicability for SQLx 0.8.6's MySQL
  public-key password encryption is mirrored in an expiring OSV Scanner entry.
  The independent RustSec gate still verifies that `sqlx-mysql` is the only
  reverse dependency before accepting the exception.
- Dependabot continues to accept patch and security updates while deferring
  SQLx, async-nats, and tower-http version transitions that would change
  foreign types exposed by Nidus 1.x public APIs.

## Verification boundary

The release candidate is checked with workspace formatting, warnings-denied
Clippy, all-feature tests and doctests, isolated feature combinations, live
service adapters, rustdoc warnings as errors, dependency and RustSec policies,
OSV Scanner, semver checks for every publishable library, package file-list
preflights, website verification, benchmark-harness compilation, fuzz-target
compilation, SBOM and coverage generation, and standalone external examples.

After publication, verify all 25 registry artifacts, docs.rs pages, and the two
standalone external examples against crates.io with:

```bash
bash scripts/verify-published-release.sh 1.0.15
```
