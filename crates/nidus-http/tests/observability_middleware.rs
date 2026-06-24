use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{Router, body::Body, routing::get};
use http::{Method, Request, Response, StatusCode};
use nidus_http::middleware::{
    HttpMetricsHook, RouteMakeSpan, route_metrics_layer, route_trace_layer,
};
use tower::{ServiceBuilder, ServiceExt, service_fn};
use tower_http::trace::MakeSpan;

#[derive(Clone, Default)]
struct RecordingMetrics {
    events: Arc<Mutex<Vec<String>>>,
}

impl RecordingMetrics {
    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }
}

impl HttpMetricsHook for RecordingMetrics {
    fn on_request(&self, method: &Method, route: Option<&str>) {
        self.events
            .lock()
            .unwrap()
            .push(format!("request {method} {}", route.unwrap_or("<unknown>")));
    }

    fn on_response(
        &self,
        method: &Method,
        route: Option<&str>,
        status: StatusCode,
        _latency: Duration,
    ) {
        self.events.lock().unwrap().push(format!(
            "response {method} {} {status}",
            route.unwrap_or("<unknown>")
        ));
    }

    fn on_error(&self, method: &Method, route: Option<&str>, _latency: Duration) {
        self.events
            .lock()
            .unwrap()
            .push(format!("error {method} {}", route.unwrap_or("<unknown>")));
    }
}

#[tokio::test]
async fn route_metrics_layer_records_request_and_response() {
    let metrics = RecordingMetrics::default();
    let service = ServiceBuilder::new()
        .layer(route_metrics_layer("/users/{id}", metrics.clone()))
        .service(service_fn(|_request: Request<()>| async {
            Ok::<_, Infallible>(
                Response::builder()
                    .status(StatusCode::CREATED)
                    .body(())
                    .unwrap(),
            )
        }));

    let response = service
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/users/42")
                .body(())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        metrics.events(),
        [
            "request POST /users/{id}",
            "response POST /users/{id} 201 Created"
        ]
    );
}

#[tokio::test]
async fn route_metrics_layer_records_inner_service_errors() {
    let metrics = RecordingMetrics::default();
    let service = ServiceBuilder::new()
        .layer(route_metrics_layer("/users/{id}", metrics.clone()))
        .service(service_fn(|_request: Request<()>| async {
            Err::<Response<()>, _>("database unavailable")
        }));

    let error = service
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/users/42")
                .body(())
                .unwrap(),
        )
        .await
        .unwrap_err();

    assert_eq!(error, "database unavailable");
    assert_eq!(
        metrics.events(),
        ["request GET /users/{id}", "error GET /users/{id}"]
    );
}

#[test]
fn route_make_span_records_route_label_field() {
    let mut make_span = RouteMakeSpan::new("/users/{id}");
    let request = Request::builder()
        .method(Method::GET)
        .uri("/users/42")
        .body(())
        .unwrap();

    let span = make_span.make_span(&request);
    let metadata = span.metadata().unwrap();

    assert_eq!(metadata.name(), "request");
    assert!(metadata.fields().field("method").is_some());
    assert!(metadata.fields().field("uri").is_some());
    assert!(metadata.fields().field("route").is_some());
}

#[tokio::test]
async fn route_trace_layer_preserves_http_responses() {
    let app = Router::new()
        .route("/users/42", get(|| async { "ok" }))
        .layer(route_trace_layer("/users/{id}"));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/users/42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
