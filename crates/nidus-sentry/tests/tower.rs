use std::sync::Arc;

use axum::{Router, body::Body, extract::Path, routing::get};
use http::{Request, StatusCode};
use nidus_sentry::SentryTowerLayer;
use sentry::{Client, ClientOptions, Hub, Level};
use tower::ServiceExt;

async fn capture(Path(id): Path<String>) -> &'static str {
    sentry::configure_scope(|scope| scope.set_tag("request.id", &id));
    sentry::capture_message("request handled", Level::Error);
    tokio::task::yield_now().await;
    "ok"
}

#[tokio::test]
async fn isolates_concurrent_requests_and_uses_matched_route_transactions() {
    let transport = sentry::test::TestTransport::new();
    let options = ClientOptions::new()
        .dsn("https://public@sentry.invalid/1")
        .traces_sample_rate(1.0)
        .send_default_pii(false)
        .transport(transport.clone());
    let client = Arc::new(Client::from(options));
    let hub = Arc::new(Hub::new(Some(client), Arc::new(Default::default())));
    let app = Router::new()
        .route("/users/{id}", get(capture))
        .layer(SentryTowerLayer::<Body>::new(hub));

    let first = app.clone().oneshot(
        Request::builder()
            .uri("/users/one?token=secret")
            .header("authorization", "Bearer secret")
            .body(Body::empty())
            .unwrap(),
    );
    let second = app.oneshot(
        Request::builder()
            .uri("/users/two")
            .body(Body::empty())
            .unwrap(),
    );
    let (first, second) = tokio::join!(first, second);
    assert_eq!(first.unwrap().status(), StatusCode::OK);
    assert_eq!(second.unwrap().status(), StatusCode::OK);

    let envelopes = transport.fetch_and_clear_envelopes();
    let mut events = Vec::new();
    let mut transactions = Vec::new();
    for item in envelopes.into_iter().flat_map(sentry::Envelope::into_items) {
        match item {
            sentry::protocol::EnvelopeItem::Event(event) => events.push(event),
            sentry::protocol::EnvelopeItem::Transaction(transaction) => {
                transactions.push(transaction.name)
            }
            _ => {}
        }
    }
    events.sort_by(|left, right| left.tags["request.id"].cmp(&right.tags["request.id"]));
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0].tags.get("request.id").map(String::as_str),
        Some("one")
    );
    assert_eq!(
        events[1].tags.get("request.id").map(String::as_str),
        Some("two")
    );
    transactions.sort();
    assert_eq!(
        transactions,
        vec![
            Some("GET /users/{id}".to_owned()),
            Some("GET /users/{id}".to_owned())
        ]
    );
    assert!(events.iter().all(|event| {
        event
            .request
            .as_ref()
            .is_some_and(|request| !request.headers.contains_key("authorization"))
    }));
}
