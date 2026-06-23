use axum::{Router, routing::get};
use nidus_core::{Module, ModuleBuilder, Nidus};
use nidus_http::server::ApplicationHttpExt;

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
