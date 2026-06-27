//! Panic-catching middleware that preserves the response body type.
//!
//! Unlike `tower_http::catch_panic`, this layer keeps the response as
//! `Response<axum::body::Body>` so it composes with [`crate::error::ErrorEnvelopeLayer`].
//! Place it inside the envelope (as [`crate::middleware::ApiDefaults::production`] does)
//! so a handler panic surfaces as a structured `500` envelope with a request id and
//! metrics, instead of aborting the connection.

use std::{
    future::Future,
    panic::{AssertUnwindSafe, catch_unwind},
    pin::Pin,
    task::{Context, Poll},
};

use axum::{body::Body, extract::Request};
use futures_util::FutureExt;
use http::{Response, StatusCode};
use tower::{Layer, Service};

/// Creates a layer that catches panics from the inner service and maps them to a
/// `500 Internal Server Error` response.
///
/// See [`CatchPanicLayer`] for details.
pub fn catch_panic_layer() -> CatchPanicLayer {
    CatchPanicLayer
}

/// Tower layer that catches panics from the inner service.
///
/// On a panic the inner service's future is abandoned, the panic payload is
/// logged via `tracing::error!`, and a bare `500` response is returned. When
/// layered inside [`crate::error::ErrorEnvelopeLayer`] that `500` is rendered as
/// the production error envelope.
#[derive(Clone, Copy, Debug, Default)]
pub struct CatchPanicLayer;

impl<S> Layer<S> for CatchPanicLayer {
    type Service = CatchPanicService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CatchPanicService { inner }
    }
}

/// Service produced by [`CatchPanicLayer`].
#[derive(Clone, Debug)]
pub struct CatchPanicService<S> {
    inner: S,
}

impl<S> Service<Request> for CatchPanicService<S>
where
    S: Service<Request, Response = Response<Body>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response<Body>, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        // Catch a panic that occurs synchronously while starting the inner
        // service (e.g. inside `call`), then catch a panic that occurs while the
        // inner future is polled.
        match catch_unwind(AssertUnwindSafe(|| self.inner.call(request))) {
            Ok(future) => Box::pin(async move {
                match AssertUnwindSafe(future).catch_unwind().await {
                    Ok(result) => result,
                    Err(payload) => {
                        log_panic(&payload);
                        Ok(internal_server_error())
                    }
                }
            }),
            Err(payload) => Box::pin(async move {
                log_panic(&payload);
                Ok(internal_server_error())
            }),
        }
    }
}

fn log_panic(payload: &Box<dyn std::any::Any + Send + 'static>) {
    if let Some(message) = payload.downcast_ref::<String>() {
        tracing::error!(http.status = 500, panic.message = %message, "request handler panicked");
    } else if let Some(message) = payload.downcast_ref::<&'static str>() {
        tracing::error!(http.status = 500, panic.message = %message, "request handler panicked");
    } else {
        tracing::error!(
            http.status = 500,
            "request handler panicked with non-string payload"
        );
    }
}

fn internal_server_error() -> Response<Body> {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    response
}
