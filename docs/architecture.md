# Architecture

The workspace is split into focused crates:

- `nidus`: public facade and prelude.
- `nidus-core`: container, modules, app, lifecycle, and errors.
- `nidus-macros`: procedural macro validation and generation.
- `nidus-http`: Axum controller, route composition, middleware, and default HTTP errors.
- `nidus-observability`: additive production observability composition over HTTP, events, jobs, lifecycle validation, and adapter-owned operations.
- `nidus-config`: typed configuration.
- `nidus-openapi`: OpenAPI metadata.
- `nidus-validation`: validation pipes.
- `nidus-auth`: guards.
- `nidus-events`: typed event bus with weak subscriber cleanup.
- `nidus-jobs`: background jobs with explicit run reports.
- `nidus-sqlx`: separately installable SQLx adapter.
- `nidus-cache`: separately installable cache adapter.
- `nidus-testing`: app test helpers.
- `cargo-nidus`: CLI tooling.

Crates should depend inward on stable abstractions and avoid circular dependencies.
Adapter crates are intentionally outside the `nidus` facade dependency graph so
backend vendors are only compiled when applications install the matching
adapter.
Framework library crates deny missing public documentation at the crate root, so
public API documentation is enforced by normal build and documentation checks
rather than being only a release-time manual audit.

Module graph construction emits `tracing` debug events for validation start,
each graph node, and validation success. This keeps graph diagnostics available
without coupling Nidus to a specific logging or metrics backend.

When the facade builder is configured with `Observability`, module graph
validation also records `module.graph.validate` lifecycle metrics.

Lifecycle startup and shutdown also emit `tracing` events for hook execution,
failures, and rollback. Hook logs use stable indexes rather than runtime
reflection so diagnostics stay explicit without adding hidden registries.
