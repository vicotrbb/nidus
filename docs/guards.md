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

Use `guard_layer` to enforce a guard on an Axum route or router layer:

```rust
let app = Router::new()
    .route("/admin", get(admin_handler))
    .layer(guard_layer(app_state, "admin:index", AuthGuard));
```

The layer creates a typed `GuardContext` from the provided state and route
label. Failed checks return the default JSON `GuardError` response without
calling the protected service.
