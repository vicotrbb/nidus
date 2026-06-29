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
`too_many_requests`, `unprocessable_entity`, and sanitized
`internal_server_error` responses.

## Production Envelopes

`ErrorEnvelopeLayer` can wrap error responses at the HTTP boundary with a
production client-facing shape:

```json
{
  "error": {
    "statusCode": 404,
    "code": "not_found",
    "message": "user not found",
    "details": null,
    "timestamp": "2026-06-24T12:00:00Z",
    "path": "/users/42",
    "requestId": "018f4ad7-56ce-4f6a-a759-29f4438d8d78"
  }
}
```

The layer reads `RequestContext` from request extensions when present, masks
server-error messages, and preserves the ability for applications to return
custom Axum responses by simply not applying the layer.

`not_found_fallback` is the first-party unmatched-route helper. Production
defaults install it automatically:

```rust
let app = ApiDefaults::production("users-api").apply(router);
```

Missing routes then return the same production envelope as
`HttpError::not_found(...)`, with `code: "not_found"` and `message:
"route not found"`. Use `without_not_found_fallback()` when an application
needs a custom Axum fallback.
