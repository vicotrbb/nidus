# Pipes

Pipes transform or validate request data. The validation crate integrates with `validator`.

```rust
let input = ValidationPipe::new().transform(input)?;
```

Validation errors expose field-level context so applications can return useful
client responses:

```rust
let error = ValidationPipe::new().transform(input).unwrap_err();
for field in error.field_errors() {
    println!("{} failed {}", field.field(), field.code());
}
```
