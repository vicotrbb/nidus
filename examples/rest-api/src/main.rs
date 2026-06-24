//! REST API example built from a Nidus controller and Axum JSON response.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::Router;
use nidus::prelude::{
    Container, Controller, Inject, Json, Path, RequestScoped, RouteDefinition, injectable,
    request_scope_layer,
};
use serde::Serialize;

#[derive(Serialize)]
struct UserDto {
    id: i64,
    email: &'static str,
    request_id: usize,
}

#[derive(Debug)]
struct RequestId(usize);

#[injectable(request)]
#[derive(Debug)]
struct RequestContext {
    request_id: Inject<RequestId>,
}

fn app() -> Router {
    let mut container = Container::new();
    let request_ids = Arc::new(AtomicUsize::new(0));
    container
        .register_request::<RequestId, _>({
            let request_ids = Arc::clone(&request_ids);
            move |_container| Ok(RequestId(request_ids.fetch_add(1, Ordering::SeqCst)))
        })
        .expect("request id provider should register");
    RequestContext::register_provider(&mut container).expect("request context should register");

    Controller::new("/users")
        .route(RouteDefinition::get("/:id", find_user))
        .into_router()
        .layer(request_scope_layer(Arc::new(container)))
}

async fn find_user(Path(id): Path<i64>, context: RequestScoped<RequestContext>) -> Json<UserDto> {
    Json(UserDto {
        id,
        email: "user@nidus.dev",
        request_id: context.request_id.0,
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
                "request_id": 0,
            }))
            .await;
    }

    #[tokio::test]
    async fn rest_api_allocates_request_context_per_request() {
        let app = TestApp::from_router(app());

        app.get("/users/1")
            .send()
            .await
            .assert_json(json!({
                "id": 1,
                "email": "user@nidus.dev",
                "request_id": 0,
            }))
            .await;
        app.get("/users/2")
            .send()
            .await
            .assert_json(json!({
                "id": 2,
                "email": "user@nidus.dev",
                "request_id": 1,
            }))
            .await;
    }
}
