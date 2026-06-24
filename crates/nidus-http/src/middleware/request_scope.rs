use std::{
    sync::Arc,
    task::{Context, Poll},
};

use http::Request;
use nidus_core::{Container, RequestScope, SharedRequestScope};
use tower::{Layer, Service};

/// Creates a request-scope layer backed by a shared dependency container.
pub fn request_scope_layer(container: Arc<Container>) -> RequestScopeLayer {
    RequestScopeLayer::new(container)
}

/// Tower layer that creates one dependency request scope per HTTP request.
#[derive(Clone)]
pub struct RequestScopeLayer {
    container: Arc<Container>,
}

impl RequestScopeLayer {
    /// Creates a request-scope layer backed by a shared dependency container.
    pub fn new(container: Arc<Container>) -> Self {
        Self { container }
    }
}

impl<S> Layer<S> for RequestScopeLayer {
    type Service = RequestScopeService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestScopeService {
            inner,
            container: Arc::clone(&self.container),
        }
    }
}

/// Service produced by [`RequestScopeLayer`].
#[derive(Clone)]
pub struct RequestScopeService<S> {
    inner: S,
    container: Arc<Container>,
}

impl<S, RequestBody> Service<Request<RequestBody>> for RequestScopeService<S>
where
    S: Service<Request<RequestBody>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    RequestBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<RequestBody>) -> Self::Future {
        let scope: SharedRequestScope = Arc::new(RequestScope::from_shared_container(Arc::clone(
            &self.container,
        )));
        request.extensions_mut().insert(scope);
        self.inner.call(request)
    }
}
