# Testing

`nidus-testing` provides in-memory request helpers around Axum routers.

```rust
let response = TestApp::from_router(router).get("/health").send().await;
response.assert_status(http::StatusCode::OK);
response.assert_header("content-type", "text/plain; charset=utf-8");
response.assert_text("ok").await;
```

Request helpers are available for `GET`, `POST`, `PUT`, `PATCH`, and `DELETE`.
Use `request(Method, path)` for other HTTP methods. Requests can set JSON, text,
or raw byte bodies and custom headers:

```rust
let response = app
    .post("/users")
    .header("x-api-key", "secret")
    .json(&CreateUser { name: "Ada".to_owned() })
    .send()
    .await;

let head = app.request(http::Method::HEAD, "/health").send().await;
```

Use `query()` to append URL-encoded query parameters from any serializable test
value:

```rust
let response = app
    .get("/users")
    .query(&ListUsers { page: 2 })
    .send()
    .await;
```

Use `try_header()` when tests should assert invalid header names or values
without panicking:

```rust
let error = app
    .get("/health")
    .try_header("bad header", "secret")
    .unwrap_err();
```

Use `try_json()` when tests should assert JSON serialization failures without
panicking.
Use `try_send()` when tests should assert request construction failures without
panicking.

Responses expose `status()`, `headers()`, `header(name)`, fallible
`header_str(name)`, `body()`, `text()`, typed `json()`, fallible `try_json()`,
and assertion helpers for status, headers, text, and JSON.

Use `TestApp::bootstrap::<AppModule>()` when a test should validate the Nidus
module graph before applying overrides:

```rust
let app = TestApp::bootstrap::<AppModule>()?
    .override_provider(MockUsersRepository::new())?
    .build();
```

Use `bootstrap_with_router()` when the same test should validate the module graph
and exercise HTTP routes:

```rust
let app = TestApp::bootstrap_with_router::<AppModule>(router)?
    .override_provider(MockUsersRepository::new())?
    .build();
```

For modular apps with imports, pass the imported module definitions explicitly:

```rust
let app = TestApp::bootstrap_with_modules::<AppModule, _>([
    UsersModule::definition(),
])?
.build();
```

Use `bootstrap_with_modules_and_router()` for imported module graphs that also
need HTTP route coverage.

Provider and config overrides are configured through the builder:

```rust
let app = TestApp::builder(router)
    .provider(UsersRepository::new())?
    .transient_provider::<RequestId, _>(|_container| Ok(RequestId::new()))?
    .override_provider(MockUsersRepository::new())?
    .config(test_config)
    .build();
```

Built test apps attach the test `Container` as an Axum `Extension<Arc<Container>>`,
so route handlers can resolve overridden providers from the same container used
by `app.resolve::<T>()`.

Request-lifetime providers can be registered with a factory and resolved through an explicit request scope:

```rust
let app = TestApp::builder(router)
    .request_provider::<RequestContext, _>(|_container| Ok(RequestContext::new()))?
    .build();

let scope = app.request_scope();
let context = scope.resolve::<RequestContext>()?;
```

Use `request_scoped_provider()` when a request-lifetime test provider depends on
another request-lifetime provider:

```rust
let app = TestApp::builder(router)
    .request_provider::<RequestId, _>(|_container| Ok(RequestId::new()))?
    .request_scoped_provider::<RequestContext, _>(|scope| {
        Ok(RequestContext::new(scope.inject::<RequestId>()?))
    })?
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
