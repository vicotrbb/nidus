//! Managed HTTP serving with supervisor-owned connections.

use crate::server::HttpApplication;
use async_trait::async_trait;
use axum::{Extension, extract::ConnectInfo};
use hyper::server::conn::http1::Builder;
use hyper_util::{rt::TokioIo, service::TowerToHyperService};
use nidus_core::{
    Application,
    lifecycle::managed::{ManagedTarget, ManagedTasks, WorkResult},
};
use std::{net::SocketAddr, sync::OnceLock};
use tokio::net::TcpListener;

/// An HTTP target that binds during managed startup and tracks every connection.
/// Address resolution is explicit: pass a socket address to avoid blocking DNS.
pub struct ManagedHttp {
    application: HttpApplication,
    address: SocketAddr,
    bound: OnceLock<SocketAddr>,
    router: OnceLock<axum::Router>,
}
impl ManagedHttp {
    /// Creates a serving target. Binding happens after startup hooks.
    pub fn new(application: HttpApplication, address: SocketAddr) -> Self {
        Self {
            application,
            address,
            bound: OnceLock::new(),
            router: OnceLock::new(),
        }
    }
    /// Returns the serving router, including the managed readiness gate after startup.
    pub fn router(&self) -> &axum::Router {
        self.router
            .get()
            .unwrap_or_else(|| self.application.router())
    }
    /// Returns the actual listening address, including an assigned ephemeral port.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.bound.get().copied()
    }
    /// Returns the composed HTTP application.
    pub fn http_application(&self) -> &HttpApplication {
        &self.application
    }
}
#[async_trait]
impl ManagedTarget for ManagedHttp {
    fn application(&self) -> &Application {
        self.application.application()
    }
    async fn start(&self, tasks: &mut ManagedTasks) -> WorkResult {
        let listener = TcpListener::bind(self.address).await?;
        let _ = self.bound.set(listener.local_addr()?);
        let readiness = tasks.signal();
        let router = self
            .application
            .router()
            .clone()
            .layer(axum::middleware::from_fn(
                move |request: axum::extract::Request, next: axum::middleware::Next| {
                    let readiness = readiness.clone();
                    async move {
                        if request.uri().path() == "/health/ready" && !readiness.is_ready() {
                            return axum::response::IntoResponse::into_response(
                                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                            );
                        }
                        next.run(request).await
                    }
                },
            ));
        let _ = self.router.set(router.clone());
        let mut signal = tasks.signal();
        let spawner = tasks.spawner();
        tasks.spawn("http listener", async move {
            loop {
                let (stream, peer) = tokio::select! {
                    biased;
                    _ = signal.cancelled() => break,
                    accepted = listener.accept() => accepted?,
                };
                let service =
                    TowerToHyperService::new(router.clone().layer(Extension(ConnectInfo(peer))));
                let mut connection_signal = signal.clone();
                if !spawner.spawn(format!("http connection {peer}"), async move {
                    let builder = Builder::new();
                    let connection = builder
                        .serve_connection(TokioIo::new(stream), service)
                        .with_upgrades();
                    tokio::pin!(connection);
                    let result = tokio::select! {
                        result = &mut connection => result,
                        _ = connection_signal.cancelled() => {
                            connection.as_mut().graceful_shutdown();
                            connection.await
                        }
                    };
                    if let Err(error) = result {
                        tracing::debug!(%error, "HTTP connection closed with error");
                    }
                    Ok(())
                }) {
                    break;
                }
            }
            Ok(())
        });
        Ok(())
    }
}
