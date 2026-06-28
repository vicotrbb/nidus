# Nidus Documentation

Nidus is a modular Rust backend framework that keeps framework behavior explicit while providing NestJS-like project organization. It uses Axum, Tower, Tokio, serde, garde, utoipa, and tracing directly instead of replacing the Rust web ecosystem.

## Guides

- [Getting Started](getting-started.md)
- [Mental Model](mental-model.md)
- [Architecture](architecture.md)
- [Modules](modules.md)
- [Dependency Injection](dependency-injection.md)
- [Providers](providers.md)
- [Controllers](controllers.md)
- [Guards](guards.md)
- [Pipes](pipes.md)
- [Interceptors](interceptors.md)
- [Config](config.md)
- [Integrations](integrations.md)
- [Error Handling](error-handling.md)
- [OpenAPI](openapi.md)
- [Observability](observability.md)
- [Testing](testing.md)
- [Events](events.md)
- [Jobs](jobs.md)
- [Performance](performance.md)
- [Deployment](deployment.md)
- [Examples](examples.md)
- [Migration From NestJS Concepts](migration-from-nestjs.md)

## Current Status

The repository is on the Nidus 1.0 launch track. It contains the core workspace crates, CLI, examples, compile-fail tests, integration tests, benchmark targets, package dry-run proof, and a generated documentation website.

Use the README for the shortest quickstart, these guides for the deeper mental model, and `website/` for the generated GitHub Pages portal.
