//! Application server helpers built on Axum and Tokio.

use std::{io, net::ToSocketAddrs};

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

    /// Serves the application until the Axum server exits.
    pub async fn listen<A>(self, address: A) -> io::Result<()>
    where
        A: ToSocketAddrs,
    {
        let listener = self.bind(address).await?;
        axum::serve(listener, self.router).await
    }
}
