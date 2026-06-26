use std::{net::SocketAddr, time::Duration};

use axum::{Router, extract::ConnectInfo, routing::get};
use nidus_core::{Module, ModuleBuilder, Nidus};
use nidus_http::server::ApplicationHttpExt;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

struct AppModule;

impl Module for AppModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("AppModule").build()
    }
}

#[tokio::test]
async fn application_can_bind_router_before_listening() {
    let app = Nidus::bootstrap::<AppModule>()
        .unwrap()
        .with_router(Router::new().route("/", get(|| async { "ok" })));

    let listener = app.bind("127.0.0.1:0").await.unwrap();

    assert_eq!(listener.local_addr().unwrap().ip().to_string(), "127.0.0.1");
}

#[tokio::test]
async fn serve_populates_connect_info_for_peer_identity() {
    let app = Nidus::bootstrap::<AppModule>()
        .unwrap()
        .with_router(Router::new().route(
            "/",
            get(|ConnectInfo(addr): ConnectInfo<SocketAddr>| async move { addr.ip().to_string() }),
        ));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { app.serve(listener).await });

    let body = http_get(addr).await;
    assert_eq!(body, "127.0.0.1", "serve() must populate ConnectInfo");

    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn serve_with_graceful_shutdown_drains_and_exits_cleanly() {
    let app = Nidus::bootstrap::<AppModule>()
        .unwrap()
        .with_router(Router::new().route("/", get(|| async { "ok" })));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        app.serve_with_graceful_shutdown(listener, async {
            let _ = shutdown_rx.await;
        })
        .await
    });

    assert_eq!(http_get(addr).await, "ok", "request works before shutdown");

    shutdown_tx.send(()).expect("trigger graceful shutdown");
    let result = tokio::time::timeout(Duration::from_secs(2), server).await;
    assert!(result.is_ok(), "graceful shutdown must exit cleanly");
    assert!(
        result.unwrap().unwrap().is_ok(),
        "serve future must complete without error"
    );
}

/// Sends a minimal HTTP/1.1 GET with `Connection: close` and returns the body.
async fn http_get(addr: SocketAddr) -> String {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let text = String::from_utf8_lossy(&buf);
    text.split("\r\n\r\n").nth(1).unwrap_or("").to_string()
}
