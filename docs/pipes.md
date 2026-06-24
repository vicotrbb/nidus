# Pipes

Pipes transform or validate request data. The validation crate integrates with `validator`.

```rust
let input = ValidationPipe::new().transform(input)?;
```

Custom pipes implement the typed `Pipe<Input>` trait:

```rust
struct TrimName;

impl Pipe<CreateUser> for TrimName {
    type Output = CreateUser;
    type Error = std::convert::Infallible;

    fn transform(&self, mut input: CreateUser) -> Result<Self::Output, Self::Error> {
        input.name = input.name.trim().to_owned();
        Ok(input)
    }
}
```

Validation errors expose field-level context so applications can return useful
client responses:

```rust
let error = ValidationPipe::new().transform(input).unwrap_err();
for field in error.field_errors() {
    println!("{} failed {}", field.field(), field.code());
}
```

`ValidationPipeError` implements Axum's `IntoResponse`. The default response is
HTTP 422 with a stable `validation_failed` code and deterministic field-level
error details, so route handlers can return `Result<T, ValidationPipeError>`
when the framework JSON shape is acceptable.
