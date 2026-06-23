# Architecture

The workspace is split into focused crates:

- `nidus`: public facade and prelude.
- `nidus-core`: container, modules, app, lifecycle, and errors.
- `nidus-macros`: procedural macro validation and generation.
- `nidus-http`: Axum controller and route composition.
- `nidus-config`: typed configuration.
- `nidus-openapi`: OpenAPI metadata.
- `nidus-validation`: validation pipes.
- `nidus-auth`: guards.
- `nidus-events`: typed event bus with weak subscriber cleanup.
- `nidus-jobs`: background jobs with explicit run reports.
- `nidus-testing`: app test helpers.
- `cargo-nidus`: CLI tooling.

Crates should depend inward on stable abstractions and avoid circular dependencies.

Module graph construction emits `tracing` debug events for validation start,
each graph node, and validation success. This keeps graph diagnostics available
without coupling Nidus to a specific logging or metrics backend.
