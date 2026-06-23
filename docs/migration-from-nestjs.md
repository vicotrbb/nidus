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
