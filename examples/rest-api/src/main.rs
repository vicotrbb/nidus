use axum::{Json, Router, routing::get};
use serde::Serialize;

#[derive(Serialize)]
struct UserDto {
    id: u64,
    email: &'static str,
}

#[tokio::main]
async fn main() {
    let app = Router::new().route(
        "/users/1",
        get(|| async {
            Json(UserDto {
                id: 1,
                email: "user@nidus.dev",
            })
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
