# Nidus Documentation

Nidus is a modular Rust backend framework that keeps framework behavior explicit while providing typed dependency injection, module graphs, Axum routes, Tower middleware, validation, OpenAPI, observability, testing, and separately installable adapters. It uses Axum, Tower, Tokio, serde, garde, utoipa, and tracing directly instead of replacing the Rust web ecosystem.

## Start

- [Installation](installation.md)
- [Getting Started](getting-started.md)
- [CLI](cli.md)

## Concepts

- [Mental Model](mental-model.md)
- [Modules](modules.md)
- [Providers / DI](dependency-injection.md)
- [Controllers / Routes](controllers.md)
- [Guards](guards.md)
- [Validation / Pipes](pipes.md)
- [Interceptors / Tower Middleware](interceptors.md)

## Runtime

- [Config](config.md)
- [Error Handling](error-handling.md)
- [OpenAPI](openapi.md)
- [Observability](observability.md)
- [Events](events.md)
- [Jobs](jobs.md)
- [Testing](testing.md)

## Production

- [Production Defaults](production-defaults.md)
- [Deployment](deployment.md)
- [Security Notes](security-notes.md)
- [Performance](performance.md)

## Ecosystem

- [Official Adapters](official-adapters.md)
- [SQLx](sqlx.md)
- [Cache](cache.md)
- [Examples](examples.md)

## Reference

- [Architecture](architecture.md)
- [API Reference](api-reference.md)
- [Release 1.0.4](release-1-0-4.md)
- [Release 1.0.3](release-1-0-3.md)

## Current Status

Nidus 1.0.0 established the public crate set. The current release track is 1.0.4, with work focused on launch hygiene, documentation, starter project depth, example proof, and package verification across every publishable crate.

Use the README for the shortest quickstart, these guides for the deeper mental model, and `website/` for the generated GitHub Pages portal.
