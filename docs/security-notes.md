# Security Notes

Nidus provides framework boundaries that help keep service behavior
inspectable, but it does not replace application security design.

## Provided Boundaries

- guard traits, guard combinators, and Tower guard layers for authorization
- typed validation pipes and stable validation error responses
- production HTTP defaults for security headers, body limits, timeouts, request IDs, and error envelopes
- explicit feature flags so optional surfaces and dependencies remain visible in Cargo manifests
- source-driven CLI inspection for routes and module graphs

## Application Responsibilities

- authentication protocol selection, key management, session policy, and credential storage
- authorization rules and tenant isolation semantics
- SQL migrations, query review, transaction boundaries, and data-retention policy
- cache key design and cache invalidation semantics
- deployment TLS, DNS, secrets, network policy, and runtime sandboxing
- security review of any raw Axum/Tower layers added outside the Nidus defaults

## Release Boundary

Local verification can prove tests, docs, package dry-runs, and example runtime
behavior. crates.io publication, docs.rs rendering, GitHub Pages settings, and
DNS state are external systems and must be verified after release.

## Fuzzing

The repository includes a cargo-fuzz setup in `fuzz/` for security-relevant
parsing boundaries:

- `config_env` covers prefixed environment parsing and JSON config parsing.
- `route_paths` covers manual HTTP route path normalization.
- `openapi_paths` covers OpenAPI route path normalization.

Build the fuzz targets locally with:

```bash
cargo +nightly fuzz build
```

Run an individual target when investigating parser behavior:

```bash
cargo +nightly fuzz run route_paths
```
## Dependency advisory review

The current upstream SQLx release was reviewed as 0.9.0. Nidus 1.x retains
0.8.6 because its existing public providers expose concrete SQLx pool types;
the 0.9 transition requires a major-version compatibility review. This also
means the following narrowly scoped advisory exception remains visible until
that migration or an upstream 0.8 maintenance release removes it.

`scripts/audit-dependencies.sh` first runs the cargo-deny advisory policy, with
yanked dependencies denied, then runs `cargo audit --deny warnings` with one
narrow exception for `RUSTSEC-2023-0071`. SQLx 0.8.6's MySQL client is the only
reverse dependency on `rsa 0.9.10`. Its authentication implementation parses a
server `RsaPublicKey` and performs randomized public-key encryption; it does not
hold or operate on an RSA private key. The RustSec advisory concerns timing
leakage from private-key operations, so there is no private key in this usage
for a remote observer to recover.

The audit script verifies that the direct reverse path remains `sqlx-mysql`
before applying the exception. Remove the exception when SQLx removes or
updates the dependency, or immediately re-review it if another dependency path
appears. Nidus also requires MySQL `ssl-mode=VERIFY_IDENTITY` in production,
which keeps authentication inside a hostname-verified TLS connection.

Primary evidence: [RustSec RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071)
and SQLx 0.8.6 `sqlx-mysql/src/connection/auth.rs` (`RsaPublicKey::encrypt`).
