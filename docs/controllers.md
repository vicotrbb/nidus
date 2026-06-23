# Controllers

Controllers group HTTP routes under a prefix and compile to Axum routers.

```rust
use nidus_http::{controller::Controller, router::RouteDefinition};

let router = Controller::new("/users")
    .route(RouteDefinition::get("/:id", || async { "ok" }))
    .route(RouteDefinition::put("/:id", || async { "updated" }))
    .into_router();
```

`RouteDefinition` supports `get`, `post`, `put`, `patch`, and `delete`.
Nidus accepts Nest-style `:id` route parameters and normalizes them to Axum-compatible `{id}` paths. Parameter segments must include a non-empty name after `:`.
Each route method must declare exactly one HTTP method attribute.
Route metadata attributes such as `#[guard]`, `#[pipe]`, `#[validate]`, and `#[openapi]` must be attached to a method with an HTTP method attribute.

The `#[controller("/prefix")]` macro preserves the struct and generates a
`controller_prefix()` accessor so route inspection and documentation tooling can
compose controller and route metadata explicitly.
Use `RouteMetadata::full_path(controller_prefix)` when tooling needs the
Axum-normalized full route path.
