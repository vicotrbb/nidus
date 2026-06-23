# Testing

`nidus-testing` provides in-memory request helpers around Axum routers.

```rust
let response = TestApp::from_router(router).get("/health").send().await;
response.assert_status(http::StatusCode::OK);
response.assert_text("ok").await;
```

Request helpers are available for `GET`, `POST`, `PUT`, `PATCH`, and `DELETE`.

Use `TestApp::bootstrap::<AppModule>()` when a test should validate the Nidus
module graph before applying overrides:

```rust
let app = TestApp::bootstrap::<AppModule>()?
    .override_provider(MockUsersRepository::new())?
    .build();
```

Provider and config overrides are configured through the builder:

```rust
let app = TestApp::builder(router)
    .provider(UsersRepository::new())?
    .override_provider(MockUsersRepository::new())?
    .config(test_config)
    .build();
```

Request-lifetime providers can be registered with a factory and resolved through an explicit request scope:

```rust
let app = TestApp::builder(router)
    .request_provider::<RequestContext, _>(|_container| Ok(RequestContext::new()))?
    .build();

let scope = app.request_scope();
let context = scope.resolve::<RequestContext>()?;
```

Lifecycle hooks can be started and shut down inside tests:

```rust
let app = TestApp::builder(router)
    .lifecycle_hook(DatabaseTestHook::new())
    .build_started()
    .await?;

app.shutdown().await?;
```
