# Testing

`nidus-testing` provides in-memory request helpers around Axum routers.

```rust
let response = TestApp::from_router(router).get("/health").send().await;
response.assert_status(http::StatusCode::OK);
response.assert_text("ok").await;
```

Testing support should grow to include provider overrides, mock injection, config overrides, app lifecycle hooks, and example app tests.

