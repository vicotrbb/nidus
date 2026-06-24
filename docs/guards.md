# Guards

Guards are typed authorization checks.

```rust
#[async_trait::async_trait]
impl Guard<AppState> for AuthGuard {
    async fn check(&self, ctx: GuardContext<AppState>) -> Result<(), GuardError> {
        Ok(())
    }
}
```

Guard errors carry a typed `http::StatusCode`, a stable code, and a reason.
They implement Axum's `IntoResponse`, so route handlers can return
`Result<T, GuardError>` directly when the default JSON error shape is enough.
