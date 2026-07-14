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
Use `GuardError::unauthorized(...)`, `GuardError::forbidden(...)`, or
`GuardError::new(status, reason)` for a custom authorization failure status.

Use `GuardExt` to compose guards when multiple checks should be reusable:

```rust
let guard = AuthGuard.and(RoleGuard::new("admin"));
let fallback = ApiKeyGuard.or(SessionGuard);
```

`and` requires both guards to pass. `or` succeeds when either guard passes; when
both guards fail, it returns the first guard error so the primary authorization
failure stays visible.

## Module-Declared Guards

`#[guard(AuthGuard)]` is executable when a controller is composed through a
Nidus module. Register the injectable guard as a provider visible to the
controller's module; the generated controller registrant resolves it from the
container and checks it before calling the handler. Guard failures bypass the
handler and become the guard's HTTP response.

The direct `controller.into_router()` path has no provider container. For that
standalone path, apply `guard_layer` explicitly as shown below. This distinction
keeps direct Axum composition explicit while making module-declared guards
enforceable rather than documentation-only metadata.

Use `guard_layer` to enforce a guard on an Axum route or router layer:

```rust
let app = Router::new()
    .route("/admin", get(admin_handler))
    .layer(guard_layer(app_state, "admin:index", AuthGuard));
```

The layer creates a typed `GuardContext` from the provided state and route
label. Failed checks return the default JSON `GuardError` response without
calling the protected service.

## Header Helpers

`GuardContext` exposes focused helpers for common HTTP guard code:

```rust
#[derive(Clone)]
struct ApiKeyGuard;

#[async_trait::async_trait]
impl Guard<()> for ApiKeyGuard {
    async fn check(&self, ctx: GuardContext<()>) -> Result<(), GuardError> {
        match ctx.api_key("x-api-key")? {
            Some(key) if key == expected_key_from_config() => Ok(()),
            _ => Err(GuardError::unauthorized("missing or invalid api key")),
        }
    }
}
```

- `header_str(name)` returns a UTF-8 header value or `Ok(None)` when missing.
- `bearer_token()` parses `Authorization: Bearer <token>`.
- `api_key(name)` reads an explicit API-key header.

The helpers parse headers only. Keep secret storage and comparison policy in
application code; avoid hardcoded production secrets.
