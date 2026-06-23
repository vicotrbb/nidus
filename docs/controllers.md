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
Nidus accepts Nest-style `:id` route parameters and normalizes them to Axum-compatible `{id}` paths.
