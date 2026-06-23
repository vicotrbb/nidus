use async_trait::async_trait;
use axum::{Router, routing::get};
use nidus_auth::{Guard, GuardContext, GuardError};

struct ApiKeyGuard;

#[async_trait]
impl Guard<()> for ApiKeyGuard {
    async fn check(&self, ctx: GuardContext<()>) -> Result<(), GuardError> {
        if ctx.route_label() == "profile" {
            Ok(())
        } else {
            Err(GuardError::forbidden("invalid route"))
        }
    }
}

#[tokio::main]
async fn main() {
    ApiKeyGuard
        .check(GuardContext::new((), "profile"))
        .await
        .unwrap();
    let app = Router::new().route("/me", get(|| async { "authorized" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
