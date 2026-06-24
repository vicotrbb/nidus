use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use nidus::prelude::*;
use tower::ServiceExt;

#[controller("/users")]
struct UsersController {
    suffix: &'static str,
}

#[routes]
impl UsersController {
    #[get("/:id")]
    async fn find_one(&self, Path(id): Path<String>) -> String {
        format!("{id}-{}", self.suffix)
    }
}

#[tokio::test]
async fn routes_macro_builds_executable_router() {
    let router = UsersController { suffix: "ready" }.into_router();

    let response = router
        .oneshot(
            Request::builder()
                .uri("/users/42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    assert_eq!(&body[..], b"42-ready");
}
