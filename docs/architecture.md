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
- `nidus-events`: typed event bus.
- `nidus-jobs`: background jobs.
- `nidus-testing`: app test helpers.
- `cargo-nidus`: CLI tooling.

Crates should depend inward on stable abstractions and avoid circular dependencies.

