//! Application server helpers built on Axum and Tokio.

use std::{future::Future, io, net::SocketAddr, net::ToSocketAddrs};

use axum::Router;
use nidus_core::Application;
use tokio::net::TcpListener;

/// Extension methods for attaching HTTP routing to a bootstrapped application.
pub trait ApplicationHttpExt: Sized {
    /// Attaches an Axum router to the application.
    fn with_router(self, router: Router) -> HttpApplication;
}

impl ApplicationHttpExt for Application {
    fn with_router(self, router: Router) -> HttpApplication {
        HttpApplication {
            application: self,
            router,
        }
    }
}

/// A bootstrapped Nidus application with an Axum router ready to serve.
///
/// All serving methods ([`Self::listen`], [`Self::serve`], and their
/// `*_with_graceful_shutdown` variants) wrap the router with Axum's
/// `into_make_service_with_connect_info::<SocketAddr>()`. This populates the
/// [`axum::extract::ConnectInfo<SocketAddr>`] extension for every connection, so
/// handlers and identity extractors such as
/// [`crate::context::client_ip_identity`] classify clients by their real peer
/// address instead of falling through to the spoofable `X-Forwarded-For` header
/// or a shared `"anonymous"` bucket.
pub struct HttpApplication {
    application: Application,
    router: Router,
}

impl HttpApplication {
    /// Returns the underlying bootstrapped application.
    pub const fn application(&self) -> &Application {
        &self.application
    }

    /// Returns the composed Axum router.
    pub const fn router(&self) -> &Router {
        &self.router
    }

    /// Transforms the composed Axum router while preserving the bootstrapped application.
    pub fn map_router(self, map: impl FnOnce(Router) -> Router) -> Self {
        Self {
            application: self.application,
            router: map(self.router),
        }
    }

    /// Consumes this HTTP application and returns its composed router.
    pub fn into_router(self) -> Router {
        self.router
    }

    /// Binds a TCP listener for this application without starting the server.
    pub async fn bind<A>(&self, address: A) -> io::Result<TcpListener>
    where
        A: ToSocketAddrs,
    {
        let address = address
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "address resolved empty"))?;
        TcpListener::bind(address).await
    }

    /// Binds `address` and serves until the server exits.
    ///
    /// The router is served with [`axum::extract::ConnectInfo<SocketAddr>`]
    /// populated for every connection. This method does **not** install a
    /// graceful-shutdown signal: the server runs until the process is killed.
    /// For in-flight request draining on a shutdown signal, use
    /// [`Self::listen_with_graceful_shutdown`] (or [`Self::serve`] /
    /// [`Self::serve_with_graceful_shutdown`] with a pre-bound listener).
    pub async fn listen<A>(self, address: A) -> io::Result<()>
    where
        A: ToSocketAddrs,
    {
        let listener = self.bind(address).await?;
        axum::serve(
            listener,
            self.router
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
    }

    /// Binds `address` and serves until `shutdown` completes, draining in-flight
    /// requests before returning.
    ///
    /// `shutdown` is any future that signals termination (for example a
    /// `SIGTERM`/`Ctrl+C` handler in production). While it is pending the server
    /// keeps accepting requests; once it resolves Axum stops accepting and waits
    /// for active connections to finish. [`axum::extract::ConnectInfo`] is
    /// populated for every connection.
    pub async fn listen_with_graceful_shutdown<A, F>(
        self,
        address: A,
        shutdown: F,
    ) -> io::Result<()>
    where
        A: ToSocketAddrs,
        F: Future<Output = ()> + Send + 'static,
    {
        let listener = self.bind(address).await?;
        axum::serve(
            listener,
            self.router
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown)
        .await
    }

    /// Serves the application on a previously bound listener.
    ///
    /// Prefer this over [`Self::listen`] when you need to control the bind
    /// yourself (for example to read the assigned port, set `SO_REUSEPORT`, or
    /// share a listener). [`axum::extract::ConnectInfo<SocketAddr>`] is
    /// populated for every connection. Like [`Self::listen`], no shutdown signal
    /// is installed.
    pub async fn serve(self, listener: TcpListener) -> io::Result<()> {
        axum::serve(
            listener,
            self.router
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
    }

    /// Serves on a previously bound listener until `shutdown` completes, draining
    /// in-flight requests before returning.
    ///
    /// See [`Self::listen_with_graceful_shutdown`] for the shutdown semantics;
    /// see [`Self::serve`] for why a pre-bound listener is useful.
    /// [`axum::extract::ConnectInfo<SocketAddr>`] is populated for every
    /// connection.
    pub async fn serve_with_graceful_shutdown<F>(
        self,
        listener: TcpListener,
        shutdown: F,
    ) -> io::Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        axum::serve(
            listener,
            self.router
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown)
        .await
    }
}
