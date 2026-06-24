# Providers

Providers are normal Rust types registered with the container.

Use providers for application services, repositories, clients, caches, and other
shared dependencies that should be resolved by type instead of by string token.

```rust
use nidus::prelude::*;

#[derive(Clone)]
struct DatabasePool;

#[injectable]
struct UsersRepository {
    database: Inject<DatabasePool>,
}
```

`#[injectable]` generates a provider registration helper for supported fields.
Required fields use `Inject<T>`. Optional fields use `Optional<T>`. If a type
needs literal configuration, custom construction, or fallible setup beyond typed
container lookup, prefer an explicit factory registration instead of hiding that
logic in a macro.

```rust
let mut container = Container::new();
container.register_singleton(DatabasePool)?;
container.register_singleton_factory(|container| {
    Ok(UsersRepository {
        database: container.inject::<DatabasePool>()?,
    })
})?;
```

## Lifetimes

- `Singleton`: created once and reused.
- `Transient`: created each time it is requested.
- `Request`: created once per explicit `RequestScope`.

Singleton is the default. Use transient providers for cheap values that should
not be cached. Use request-scoped providers only when values genuinely depend on
request state, because they add request-path work and must be resolved through a
request scope.

```rust
container.register_transient::<CorrelationId, _>(|_container| Ok(CorrelationId::new()))?;
container.register_request::<RequestId, _>(|_container| Ok(RequestId::new()))?;
```

Use `register_request_scoped` when a request provider depends on another
request provider and should reuse values from the same scope.

## Design Guidance

- Keep providers small and cohesive.
- Prefer constructor dependencies over service locators.
- Prefer typed wrappers over string tokens or unstructured maps.
- Keep external resource setup explicit at application startup.
- Avoid per-request provider resolution unless the provider is request-specific.
- Keep mockable behavior behind ordinary Rust traits when tests need alternate implementations.
