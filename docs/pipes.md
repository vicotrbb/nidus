# Pipes

Pipes transform or validate request data. The validation crate integrates with `validator`.

```rust
let input = ValidationPipe::new().transform(input)?;
```

Validation errors should preserve field-level context so applications can return useful client responses.

