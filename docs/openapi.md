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
        OpenApiRoute::get("/users/{id}")
            .summary("Find user by ID")
            .response_schema::<UserDto>(),
    );
```

Future route macros should feed this metadata automatically while keeping generated code inspectable.
