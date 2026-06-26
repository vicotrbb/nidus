# Nidus Documentation

Nidus is a modular Rust backend framework that keeps framework behavior explicit while providing NestJS-like project organization. It uses Axum, Tower, Tokio, serde, validator, utoipa, and tracing directly instead of replacing the Rust web ecosystem.

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
- [Testing](testing.md)
- [Events](events.md)
- [Jobs](jobs.md)
- [Performance](performance.md)
- [Deployment](deployment.md)
- [Examples](examples.md)
- [Migration From NestJS Concepts](migration-from-nestjs.md)

## Current Status

The framework is under active development. The repository contains the core workspace crates, CLI, examples, compile-fail tests, integration tests, and benchmark targets. Public APIs can still change before the first published release.

Use the README for the shortest quickstart and these guides for the deeper mental model.
