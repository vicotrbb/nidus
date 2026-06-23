# Providers

Providers are normal Rust types registered with the container.

Provider lifetimes:

- `Singleton`: created once and reused.
- `Transient`: created each time it is requested.
- `Request`: created once per explicit `RequestScope`.

Use small provider types with clear constructor dependencies. Prefer typed wrappers over runtime lookup or string tokens.
