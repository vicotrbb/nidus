use axum::Router;
use nidus::prelude::*;

struct AppModule;

impl Module for AppModule {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("AppModule").build()
    }
}

fn main() {
    let _server = Nidus::bootstrap::<AppModule>()
        .unwrap()
        .with_router(Router::new())
        .listen("127.0.0.1:0");
}
