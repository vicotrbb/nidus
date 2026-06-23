# Testing

`nidus-testing` provides in-memory request helpers around Axum routers.

```rust
let response = TestApp::from_router(router).get("/health").send().await;
response.assert_status(http::StatusCode::OK);
response.assert_text("ok").await;
```

Provider and config overrides are configured through the builder:

```rust
let app = TestApp::builder(router)
    .provider(UsersRepository::new())?
    .override_provider(MockUsersRepository::new())?
    .config(test_config)
    .build();
```

Lifecycle hooks can be started and shut down inside tests:

```rust
let app = TestApp::builder(router)
    .lifecycle_hook(DatabaseTestHook::new())
    .build_started()
    .await?;

app.shutdown().await?;
```
