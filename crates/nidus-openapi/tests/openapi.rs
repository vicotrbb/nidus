use nidus_openapi::{OpenApiDocument, OpenApiDocumentError, OpenApiRoute};
use nidus_testing::TestApp;

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct UserDto {
    id: i32,
    email: String,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct CreateUserDto {
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
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["operationId"],
        "get_users_by_id"
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["parameters"],
        serde_json::json!([
            {
                "name": "id",
                "in": "path",
                "required": true,
                "schema": {
                    "type": "string"
                }
            }
        ])
    );
}

#[test]
fn openapi_route_builders_normalize_nidus_params() {
    let document = OpenApiDocument::new("Nidus API", "0.1.0")
        .route(OpenApiRoute::get("/users/:id").summary("Find user by ID"));

    let json = document.to_json_value();
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["summary"],
        "Find user by ID"
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["operationId"],
        "get_users_by_id"
    );
}

#[test]
fn openapi_route_try_builder_rejects_empty_parameter_name() {
    let error = match OpenApiRoute::try_get("/:") {
        Ok(_) => panic!("empty route parameter should fail"),
        Err(error) => error,
    };

    assert_eq!(error.path(), "/:");
}

#[test]
fn openapi_document_rejects_duplicate_operations() {
    let document = OpenApiDocument::new("Nidus API", "0.1.0")
        .try_route(OpenApiRoute::get("/users/:id"))
        .unwrap();

    let error = document
        .try_route(OpenApiRoute::get("/users/{id}"))
        .unwrap_err();

    assert_eq!(
        error,
        OpenApiDocumentError::DuplicateOperation {
            method: "get".to_owned(),
            path: "/users/{id}".to_owned()
        }
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
fn openapi_route_builders_cover_mutation_methods() {
    let document = OpenApiDocument::new("Nidus API", "0.1.0")
        .route(OpenApiRoute::put("/users/{id}").summary("Replace user"))
        .route(OpenApiRoute::patch("/users/{id}").summary("Update user"))
        .route(OpenApiRoute::delete("/users/{id}").summary("Delete user"));

    let json = document.to_json_value();
    assert_eq!(
        json["paths"]["/users/{id}"]["put"]["summary"],
        "Replace user"
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["patch"]["summary"],
        "Update user"
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["delete"]["summary"],
        "Delete user"
    );
}

#[test]
fn openapi_route_can_reference_registered_response_schema() {
    let document = OpenApiDocument::new("Nidus API", "0.1.0")
        .schema::<UserDto>()
        .route(OpenApiRoute::get("/users/{id}").response_schema::<UserDto>());

    let json = document.to_json_value();
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
            ["$ref"],
        "#/components/schemas/UserDto"
    );
}

#[test]
fn openapi_route_can_set_success_response_status() {
    let document = OpenApiDocument::new("Nidus API", "0.1.0")
        .schema::<UserDto>()
        .route(
            OpenApiRoute::post("/users")
                .response_status(http::StatusCode::CREATED)
                .response_schema::<UserDto>(),
        );

    let json = document.to_json_value();
    assert_eq!(
        json["paths"]["/users"]["post"]["responses"]["201"]["content"]["application/json"]["schema"]
            ["$ref"],
        "#/components/schemas/UserDto"
    );
    assert!(json["paths"]["/users"]["post"]["responses"]["200"].is_null());
}

#[test]
fn openapi_route_can_reference_registered_request_schema() {
    let document = OpenApiDocument::new("Nidus API", "0.1.0")
        .schema::<CreateUserDto>()
        .route(OpenApiRoute::post("/users").request_schema::<CreateUserDto>());

    let json = document.to_json_value();
    assert_eq!(
        json["paths"]["/users"]["post"]["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/CreateUserDto"
    );
    assert_eq!(
        json["paths"]["/users"]["post"]["requestBody"]["required"],
        true
    );
    assert!(json["components"]["schemas"]["CreateUserDto"].is_object());
}

#[test]
fn openapi_route_records_operation_tags() {
    let document = OpenApiDocument::new("Nidus API", "0.1.0").route(
        OpenApiRoute::get("/users/{id}")
            .summary("Find user by ID")
            .tag("users")
            .tag("public"),
    );

    let json = document.to_json_value();
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["tags"],
        serde_json::json!(["users", "public"])
    );
}

#[test]
fn openapi_route_omits_absent_optional_operation_metadata() {
    let document = OpenApiDocument::new("Nidus API", "0.1.0").route(OpenApiRoute::get("/health"));

    let json = document.to_json_value();
    assert!(json["paths"]["/health"]["get"]["summary"].is_null());
    assert_eq!(json["paths"]["/health"]["get"]["operationId"], "get_health");
    assert!(json["paths"]["/health"]["get"]["tags"].is_null());
    assert!(json["paths"]["/health"]["get"]["requestBody"].is_null());
    assert!(json["paths"]["/health"]["get"]["parameters"].is_null());
    assert!(json["paths"]["/health"]["get"]["x-nidus-guards"].is_null());
    assert!(json["paths"]["/health"]["get"]["x-nidus-pipes"].is_null());
    assert!(json["paths"]["/health"]["get"]["x-nidus-validates"].is_null());
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
                    "operationId": "get_users_by_id",
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "summary": "Find user by ID"
                }
            }
        }
    }));

    let docs = app.get("/docs").send().await;
    docs.assert_status(http::StatusCode::OK);
    let html = String::from_utf8(docs.body().to_vec()).unwrap();
    assert!(html.contains("<!doctype html>"));
    assert!(html.contains("<title>Nidus API Documentation</title>"));
    assert!(html.contains("https://cdn.jsdelivr.net/npm/swagger-ui-dist/"));
    assert!(html.contains("url: \"/openapi.json\""));
}
