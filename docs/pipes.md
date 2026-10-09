# Pipes

Pipes transform or validate request data. The validation crate integrates with `garde`.

```rust
let input = ValidationPipe::new().transform(input)?;
```

Custom transformations use ordinary functions. Call them explicitly before validation:

```rust
fn trim_name(mut input: CreateUser) -> CreateUser {
    input.name = input.name.trim().to_owned();
    input
}

let input = ValidationPipe::new().transform(trim_name(input))?;
```

The `#[pipe(Type)]` attribute records route metadata; it does not execute a
transformation. Apply custom transformations in the handler or an Axum extractor.

Validation errors expose field-level context so applications can return useful
client responses:

```rust
let error = ValidationPipe::new().transform(input).unwrap_err();
for field in error.field_errors() {
    println!("{} failed {}", field.field(), field.code());
}
```

Nested validation errors are flattened into deterministic field paths such as
`profile.display_name` or `members[0].email`.

`ValidationPipeError` implements Axum's `IntoResponse`. The default response is
HTTP 422 with a stable `validation_failed` code and deterministic field-level
error details, so route handlers can return `Result<T, ValidationPipeError>`
when the framework JSON shape is acceptable.

For JSON request bodies, use `ValidatedJson<T>` to deserialize with Axum and
validate with `ValidationPipe` before the handler runs:

```rust
async fn create(ValidatedJson(input): ValidatedJson<CreateUser>) -> Json<UserDto> {
    Json(create_user(input))
}
```
