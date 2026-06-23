use axum::Router;
use nidus::prelude::{Controller, Json, Path, RouteDefinition};
use serde::Serialize;

#[derive(Serialize)]
struct UserDto {
    id: i64,
    email: &'static str,
}

fn app() -> Router {
    Controller::new("/users")
        .route(RouteDefinition::get("/:id", find_user))
        .into_router()
}

async fn find_user(Path(id): Path<i64>) -> Json<UserDto> {
    Json(UserDto {
        id,
        email: "user@nidus.dev",
    })
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app()).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use nidus_testing::TestApp;
    use serde_json::json;

    #[tokio::test]
    async fn rest_api_returns_user_by_id() {
        let response = TestApp::from_router(app()).get("/users/42").send().await;

        response
            .assert_json(json!({
                "id": 42,
                "email": "user@nidus.dev",
            }))
            .await;
    }
}
