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
