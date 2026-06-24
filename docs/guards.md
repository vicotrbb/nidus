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

Use `GuardExt` to compose guards when multiple checks should be reusable:

```rust
let guard = AuthGuard.and(RoleGuard::new("admin"));
let fallback = ApiKeyGuard.or(SessionGuard);
```

`and` requires both guards to pass. `or` succeeds when either guard passes; when
both guards fail, it returns the first guard error so the primary authorization
failure stays visible.
