//! Minimal Nidus HTTP server example.

use nidus::prelude::*;

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
    let app = Nidus::create::<AppModule>()
        .start_managed("127.0.0.1:3000".parse()?, Default::default())
        .await
        .map_err(|report| std::io::Error::other(format!("startup failed: {report:?}")))?;
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    let report = app.shutdown().await;
    if !report.is_success() {
        return Err(std::io::Error::other(format!("shutdown failed: {report:?}")).into());
    }
    println!("managed shutdown complete");
    Ok(())
}

#[module]
struct AppModule {
    controllers: [HelloController],
}

#[cfg(test)]
mod tests {
    use super::*;
    use nidus_testing::TestApp;

    #[tokio::test]
    async fn hello_world_responds() {
        let app = TestApp::bootstrap::<AppModule>()
            .unwrap()
            .build_managed(Default::default())
            .await
            .unwrap();
        let response = app.get("/").send().await;

        response.assert_text("hello from nidus");
        app.shutdown().await.unwrap();
    }
}
