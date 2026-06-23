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

The default provider lifetime is expected to be singleton. Request-scoped providers should remain opt-in because they add request path overhead.

