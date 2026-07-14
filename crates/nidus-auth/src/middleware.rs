use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use axum::{
    body::Body,
    response::{IntoResponse, Response},
};
use http::Request;
use tower::{Layer, Service};

use crate::{Guard, GuardContext};

/// Creates a Tower layer that checks a typed guard before calling the inner service.
pub fn guard_layer<S, G>(state: S, route_label: impl Into<String>, guard: G) -> GuardLayer<S, G> {
    GuardLayer::new(state, route_label, guard)
}

/// Tower layer that enforces a Nidus guard for an HTTP route.
#[derive(Clone, Debug)]
pub struct GuardLayer<S, G> {
    state: S,
    route_label: Arc<str>,
    guard: G,
}

impl<S, G> GuardLayer<S, G> {
    /// Creates a guard layer with typed state and a stable route label.
    pub fn new(state: S, route_label: impl Into<String>, guard: G) -> Self {
        Self {
            state,
            route_label: Arc::from(route_label.into()),
            guard,
        }
    }
}

impl<Inner, S, G> Layer<Inner> for GuardLayer<S, G>
where
    S: Clone,
    G: Clone,
{
    type Service = GuardService<Inner, S, G>;

    fn layer(&self, inner: Inner) -> Self::Service {
        GuardService {
            inner,
            state: self.state.clone(),
            route_label: Arc::clone(&self.route_label),
            guard: self.guard.clone(),
        }
    }
}

/// Service produced by [`GuardLayer`].
#[derive(Clone, Debug)]
pub struct GuardService<Inner, S, G> {
    inner: Inner,
    state: S,
    route_label: Arc<str>,
    guard: G,
}

impl<Inner, S, G> Service<Request<Body>> for GuardService<Inner, S, G>
where
    Inner: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    Inner::Future: Send + 'static,
    Inner::Error: Send + 'static,
    S: Clone + Send + Sync + 'static,
    G: Guard<S> + Clone + Send + Sync + 'static,
{
    type Response = Response;
    type Error = Inner::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let state = self.state.clone();
        let route_label = Arc::clone(&self.route_label);
        let guard = self.guard.clone();
        // Move the service that was driven to readiness into the future and
        // leave the fresh clone behind, per the Tower readiness contract.
        let clone = self.inner.clone();
        let inner = std::mem::replace(&mut self.inner, clone);

        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let context = GuardContext::from_shared_route_label(state, route_label)
                .with_headers(parts.headers.clone());

            if let Err(error) = guard.check(context).await {
                return Ok(error.into_response());
            }

            let request = Request::from_parts(parts, body);
            let mut inner = inner;
            inner.call(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::GuardLayer;
    use crate::GuardContext;

    #[test]
    fn cloned_guard_layers_share_their_immutable_route_label() {
        let layer = GuardLayer::new((), "/users/{id}", ());
        let cloned = layer.clone();
        let context = GuardContext::new((), "/users/{id}");
        let cloned_context = context.clone();

        assert!(Arc::ptr_eq(&layer.route_label, &cloned.route_label));
        assert!(Arc::ptr_eq(
            &context.route_label,
            &cloned_context.route_label
        ));
    }
}
