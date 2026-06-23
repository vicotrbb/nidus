# Dependency Injection

The Nidus container is keyed by Rust types, not strings.

```rust
let mut container = nidus_core::Container::new();
container.register_singleton(DatabasePool::new())?;
let pool = container.resolve::<DatabasePool>()?;
```

Factories can resolve dependencies through the same typed API:

```rust
container.register_factory(ProviderLifetime::Singleton, |container| {
    Ok(UsersRepository::new(container.inject::<DatabasePool>()?))
})?;
```

Factory failures are reported with the provider type that failed and preserve
the underlying source error.

The default provider lifetime is expected to be singleton. Request-scoped
providers are opt-in and must be resolved through an explicit request scope
because they add request path overhead:

```rust
let scope = container.request_scope();
let request_state = scope.resolve::<RequestState>()?;
```

Resolving a request-scoped provider through the root container returns a
`RequestScopeRequired` error instead of silently behaving like a transient
provider.
