# Migration From NestJS Concepts

Nidus borrows organizational ideas from NestJS, not runtime mechanics.

Concept mapping:

- NestJS module -> explicit Nidus module definition.
- Provider token -> Rust concrete type.
- Constructor injection -> typed `Inject<T>` or explicit factory.
- Pipe -> Rust validation or transformation type.
- Guard -> async Rust trait.
- Interceptor -> Tower layer where possible.
- Decorator metadata -> procedural macro generated code.

Prefer Rust ownership, traits, and compile-time validation over dynamic runtime metadata.

## Main Differences

NestJS leans on runtime metadata, decorators, dependency tokens, and a dynamic
container. Nidus should not. A Nidus application should remain understandable as
ordinary Rust:

- Provider identity is a Rust type.
- Module metadata lowers to explicit definitions.
- Route handling is Axum.
- Middleware is Tower.
- Validation uses Rust types and traits.
- Errors are ordinary Rust enums or `HttpError` values that implement Axum response conversion.

## Migration Pattern

Start by mapping feature boundaries to modules, not by translating every class.
Then introduce providers for real dependencies and keep business logic in normal
Rust methods.

```rust
#[module]
struct UsersModule {
    providers: (UsersRepository, UsersService),
    controllers: (UsersController,),
    exports: (UsersService,),
}
```

Use `Inject<T>` when a provider needs another provider and use explicit factory
registration when the equivalent NestJS provider used dynamic configuration.

## What Not To Port Directly

- Do not model string provider tokens unless an integration genuinely needs an indirection layer.
- Do not create global mutable registries to mimic runtime discovery.
- Do not hide database pools, clients, or config behind untyped maps.
- Do not translate TypeScript decorators into macros when plain Rust is clearer.
- Do not add request scope by default; use it only when request-specific state is required.

The target is not a Rust clone of NestJS. The target is a Rust framework that
keeps NestJS's organizational clarity while preserving Rust's explicitness,
type checking, and ecosystem tooling.
