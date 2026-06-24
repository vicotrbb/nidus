# Error Handling

`nidus-http` provides `HttpError` as a default JSON error response for common
HTTP failures.

```rust
use nidus::prelude::*;

async fn find_user() -> Result<&'static str, HttpError> {
    Err(HttpError::not_found("user not found"))
}
```

The response body uses a stable client-facing shape:

```json
{
  "error": {
    "code": "not_found",
    "message": "user not found"
  }
}
```

Use `HttpError::new(status, code, message)` when an application needs a custom
status or machine-readable code. Application-specific error enums can still
implement Axum's `IntoResponse` directly when they need full control.

Common helpers cover default API failures including `bad_request`,
`unauthorized`, `forbidden`, `not_found`, `conflict`,
`unprocessable_entity`, and sanitized `internal_server_error` responses.
