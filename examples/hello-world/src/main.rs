use axum::Router;
use nidus::prelude::*;

fn app() -> Router {
    Controller::new("/")
        .route(RouteDefinition::get("/", || async { "hello from nidus" }))
        .into_router()
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    Nidus::bootstrap::<AppModule>()?
        .with_router(app())
        .listen("127.0.0.1:3000")
        .await?;
    Ok(())
}

struct AppModule;

impl Module for AppModule {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("AppModule").build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nidus_testing::TestApp;

    #[tokio::test]
    async fn hello_world_responds() {
        let response = TestApp::from_router(app()).get("/").send().await;

        response.assert_text("hello from nidus").await;
    }
}
