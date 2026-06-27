//! Minimal Nidus HTTP server example.

use nidus::prelude::*;

fn app() -> Router {
    HelloController.into_router()
}

#[controller("/")]
struct HelloController;

#[routes]
impl HelloController {
    #[get("/")]
    async fn hello(&self) -> &'static str {
        "hello from nidus"
    }
}

#[nidus::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    Nidus::bootstrap::<AppModule>()?
        .with_router(app())
        .listen("127.0.0.1:3000")
        .await?;
    Ok(())
}

#[module]
struct AppModule;

#[cfg(test)]
mod tests {
    use super::*;
    use nidus_testing::TestApp;

    #[tokio::test]
    async fn hello_world_responds() {
        let response = TestApp::from_router(app()).get("/").send().await;

        response.assert_text("hello from nidus");
    }
}
