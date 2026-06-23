use nidus_http::router::RouteMetadata;
use nidus_openapi::{OpenApiDocument, OpenApiRoute};
use nidus_testing::TestApp;

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct UserDto {
    id: i32,
    email: String,
}

#[test]
fn openapi_document_records_routes_and_serves_json() {
    let document = OpenApiDocument::new("Nidus API", "0.1.0")
        .route(OpenApiRoute::get("/users/{id}").summary("Find user by ID"));

    let json = document.to_json_value();

    assert_eq!(json["info"]["title"], "Nidus API");
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["summary"],
        "Find user by ID"
    );
}

#[test]
fn openapi_document_registers_utoipa_schemas() {
    let document = OpenApiDocument::new("Nidus API", "0.1.0").schema::<UserDto>();

    let json = document.to_json_value();
    assert!(json["components"]["schemas"]["UserDto"].is_object());
    assert_eq!(
        json["components"]["schemas"]["UserDto"]["properties"]["email"]["type"],
        "string"
    );
}

#[test]
fn openapi_document_can_be_generated_from_route_metadata() {
    let routes = [RouteMetadata::with_annotations(
        "GET",
        "/users/:id",
        Some("Find user by ID"),
        &["AuthGuard"],
        &["ValidationPipe"],
        true,
    )];

    let document = OpenApiDocument::from_route_metadata("Nidus API", "0.1.0", &routes);

    let json = document.to_json_value();
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["summary"],
        "Find user by ID"
    );
}

#[tokio::test]
async fn openapi_document_serves_json_and_docs_routes() {
    let router = OpenApiDocument::new("Nidus API", "0.1.0")
        .route(OpenApiRoute::get("/users/{id}").summary("Find user by ID"))
        .into_router();
    let app = TestApp::from_router(router);

    let json = app.get("/openapi.json").send().await;
    json.assert_status(http::StatusCode::OK);
    json.assert_json(serde_json::json!({
        "info": {
            "title": "Nidus API",
            "version": "0.1.0"
        },
        "openapi": "3.1.0",
        "paths": {
            "/users/{id}": {
                "get": {
                    "responses": {
                        "200": {
                            "description": "Success"
                        }
                    },
                    "summary": "Find user by ID"
                }
            }
        }
    }))
    .await;

    let docs = app.get("/docs").send().await;
    docs.assert_status(http::StatusCode::OK);
    docs.assert_text("Nidus API docs are available at /openapi.json")
        .await;
}
