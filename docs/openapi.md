# OpenAPI

`nidus-openapi` records route metadata and renders OpenAPI JSON.

```rust
let document = OpenApiDocument::new("Nidus API", "0.1.0")
    .route(OpenApiRoute::get("/users/{id}").summary("Find user by ID"));
```

Future route macros should feed this metadata automatically while keeping generated code inspectable.

