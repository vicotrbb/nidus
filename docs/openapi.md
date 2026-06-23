# OpenAPI

`nidus-openapi` records route metadata and renders OpenAPI JSON.

```rust
#[derive(utoipa::ToSchema)]
struct UserDto {
    id: i32,
    email: String,
}

let document = OpenApiDocument::new("Nidus API", "0.1.0")
    .schema::<UserDto>()
    .route(
        OpenApiRoute::get("/users/:id")
            .summary("Find user by ID")
            .tag("users")
            .response_schema::<UserDto>(),
    );
```

Use `.tag("...")` to group operations in generated OpenAPI tooling. Multiple
tags can be attached to the same route.
Manual OpenAPI route builders accept Nidus-style `:id` parameters and normalize
them to OpenAPI `{id}` path parameters. Use `try_get`, `try_post`, `try_put`,
`try_patch`, or `try_delete` when invalid paths should return a `RoutePathError`.

Route macros can emit the same operation tags through `#[openapi]` metadata:

```rust
#[get("/:id")]
#[openapi(summary = "Find user by ID", tags = ["users", "read"])]
async fn find_one(&self) {}
```

Generated route metadata can also seed a document directly:

```rust
let document = OpenApiDocument::from_controller_routes(
    "Nidus API",
    "0.1.0",
    UsersController::controller_prefix(),
    &UsersController::routes(),
);
```

For multiple controllers, use the builder form:

```rust
let document = OpenApiDocument::new("Nidus API", "0.1.0")
    .controller_routes(UsersController::controller_prefix(), &UsersController::routes())
    .controller_routes(AdminController::controller_prefix(), &AdminController::routes());
```

This keeps controller prefixes and route paths explicit while reusing the same
normalization rules as the HTTP router.
Use `try_from_controller_routes` or `try_controller_routes` when composing
metadata from generated or external sources where invalid paths should return a
`RoutePathError` instead of panicking.

Serve the generated OpenAPI document and interactive documentation from an
Axum router:

```rust
let router = document.into_router();
```

This exposes `/openapi.json` and `/docs`. The docs page loads Swagger UI in the
browser and points it at the local OpenAPI JSON route.
