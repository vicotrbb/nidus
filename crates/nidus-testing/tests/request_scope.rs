use axum::{Router, routing::get};
use http::StatusCode;
use nidus_http::RequestScoped;
use nidus_testing::TestApp;

#[derive(Debug)]
struct Greeting(&'static str);

async fn show_greeting(greeting: RequestScoped<Greeting>) -> &'static str {
    (*greeting).0
}

#[tokio::test]
async fn with_request_scope_enables_request_scoped_extractors() {
    // T-1: a TestApp built with `with_request_scope` installs the request scope
    // layer, so `RequestScoped<T>` extractors resolve in integration tests.
    let app = TestApp::builder(Router::new().route("/greet", get(show_greeting)))
        .request_provider::<Greeting, _>(|_container| Ok(Greeting("hello")))
        .expect("register greeting provider")
        .with_request_scope()
        .build();

    let response = app.get("/greet").send().await;
    response.assert_status(StatusCode::OK);
    response.assert_text("hello").await;
}

#[tokio::test]
async fn without_request_scope_rejects_request_scoped_extractors() {
    // Without `with_request_scope`, the request scope extension is absent and
    // the extractor rejects with 500 `request_scope_unavailable`.
    let app = TestApp::builder(Router::new().route("/greet", get(show_greeting)))
        .request_provider::<Greeting, _>(|_container| Ok(Greeting("hello")))
        .expect("register greeting provider")
        .build();

    let response = app.get("/greet").send().await;
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}
