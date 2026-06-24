# Dependency Injection

The Nidus container is keyed by Rust types, not strings.

```rust
let mut container = nidus_core::Container::new();
container.register_singleton(DatabasePool::new())?;
let pool = container.resolve::<DatabasePool>()?;
```

Factories can resolve dependencies through the same typed API:

```rust
container.register_singleton_factory(|container| {
    Ok(UsersRepository::new(container.inject::<DatabasePool>()?))
})?;
```

Factory failures are reported with the provider type that failed and preserve
the underlying source error.

Optional dependencies can be resolved without turning missing providers into
startup failures:

```rust
let cache = container.optional::<CacheClient>()?;
if let Some(cache) = cache.as_ref() {
    cache.warm();
}
```

Only missing providers become `None`; registered providers that fail to build
still return their original construction error.

`Lazy<T>` defers resolution until the dependency is actually needed:

```rust
let container = Arc::new(container);
let lazy_pool = Lazy::new({
    let container = Arc::clone(&container);
    move || container.inject::<DatabasePool>()
});
let pool = lazy_pool.get()?;
```

`Factory<T>` creates a fresh value on every call and preserves any construction
error:

```rust
let ids = Factory::new(|| Ok(RequestId::new()));
let first = ids.create()?;
let second = ids.create()?;
```

The default provider lifetime is expected to be singleton. Request-scoped
providers are opt-in and must be resolved through an explicit request scope
because they add request path overhead:

```rust
container.register_transient::<RequestId, _>(|_container| Ok(RequestId::new()))?;
container.register_request::<RequestState, _>(|container| {
    Ok(RequestState::new(container.inject::<RequestId>()?))
})?;

let scope = container.request_scope();
let request_state = scope.resolve::<RequestState>()?;
let scoped_state = scope.scoped::<RequestState>()?;
```

Resolving a request-scoped provider through the root container returns a
`RequestScopeRequired` error instead of silently behaving like a transient
provider.
