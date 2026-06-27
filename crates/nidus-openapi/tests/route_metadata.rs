use nidus_http::router::RouteMetadata;
use nidus_openapi::{OpenApiDocument, OpenApiDocumentError};

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
fn openapi_document_emits_error_responses_for_guarded_validating_routes() {
    // O-1: guarded/validating routes must advertise the error statuses they can
    // return (401/403 for guards, 422 for validation), so clients can discover
    // them from the spec instead of only seeing the success response.
    let routes = [RouteMetadata::with_openapi_annotations(
        "POST",
        "/users",
        Some("Create user"),
        &["users"],
        &["AuthGuard"],
        &["ValidationPipe"],
        true,
    )];
    let document = OpenApiDocument::from_route_metadata("Nidus API", "0.1.0", &routes);

    let json = document.to_json_value();
    let responses = &json["paths"]["/users"]["post"]["responses"];
    assert_eq!(responses["401"]["description"], "Unauthorized");
    assert_eq!(responses["403"]["description"], "Forbidden");
    assert_eq!(responses["422"]["description"], "Validation failed");
    // Success response is still present and untouched.
    assert!(responses["200"].is_object());
}

#[test]
fn openapi_document_omits_error_responses_for_plain_routes() {
    // A route with no guards and no validation advertises only its success
    // response (behavior unchanged by O-1).
    let routes = [RouteMetadata::with_openapi_annotations(
        "GET",
        "/users/:id",
        Some("Find user by ID"),
        &["users"],
        &[],
        &[],
        false,
    )];
    let document = OpenApiDocument::from_route_metadata("Nidus API", "0.1.0", &routes);

    let json = document.to_json_value();
    let responses = &json["paths"]["/users/{id}"]["get"]["responses"];
    assert!(responses["200"].is_object());
    assert!(responses["401"].is_null());
    assert!(responses["403"].is_null());
    assert!(responses["422"].is_null());
}

#[test]
fn openapi_document_can_be_generated_from_route_metadata() {
    let routes = [RouteMetadata::with_openapi_annotations(
        "GET",
        "/users/:id",
        Some("Find user by ID"),
        &["users", "read"],
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
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["tags"],
        serde_json::json!(["users", "read"])
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["parameters"][0]["name"],
        "id"
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["x-nidus-guards"],
        serde_json::json!(["AuthGuard"])
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["x-nidus-pipes"],
        serde_json::json!(["ValidationPipe"])
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["x-nidus-validates"],
        true
    );
}

#[test]
fn openapi_document_registers_schemas_from_route_metadata() {
    let routes = [RouteMetadata::with_openapi_annotations(
        "POST",
        "/users",
        Some("Create user"),
        &["users"],
        &[],
        &[],
        true,
    )
    .with_openapi_schemas(Some("CreateUserDto"), Some("UserDto"))
    .with_openapi_schema_registrars(
        Some(register_schema::<CreateUserDto>),
        Some(register_schema::<UserDto>),
    )];

    let document = OpenApiDocument::from_route_metadata("Nidus API", "0.1.0", &routes)
        .schemas_from_route_metadata(&routes);

    let json = document.to_json_value();
    assert_eq!(
        json["paths"]["/users"]["post"]["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/CreateUserDto"
    );
    assert_eq!(
        json["paths"]["/users"]["post"]["responses"]["200"]["content"]["application/json"]["schema"]
            ["$ref"],
        "#/components/schemas/UserDto"
    );
    assert!(json["components"]["schemas"]["CreateUserDto"].is_object());
    assert!(json["components"]["schemas"]["UserDto"].is_object());
}

#[test]
fn openapi_document_dedupes_route_schemas() {
    let routes = [
        RouteMetadata::with_openapi_annotations(
            "GET",
            "/users/:id",
            Some("Find one"),
            &["users"],
            &[],
            &[],
            true,
        )
        .with_openapi_schemas(Some("UserDto"), Some("UserDto"))
        .with_openapi_schema_registrars(
            Some(register_schema::<UserDto>),
            Some(register_schema::<UserDto>),
        ),
        RouteMetadata::with_openapi_annotations(
            "POST",
            "/users",
            Some("Create one"),
            &["users"],
            &[],
            &[],
            true,
        )
        .with_openapi_schemas(None, Some("UserDto"))
        .with_openapi_schema_registrars(None, Some(register_schema::<UserDto>)),
    ];

    let document = OpenApiDocument::from_route_metadata("Nidus API", "0.1.0", &routes)
        .schema::<UserDto>()
        .schemas_from_route_metadata(&routes);

    let json = document.to_json_value();
    let schemas = json["components"]["schemas"]
        .as_object()
        .expect("components.schemas should be an object");
    assert_eq!(schemas.len(), 1);
    assert!(schemas.contains_key("UserDto"));
}

fn register_schema<T: utoipa::ToSchema>(schemas: &mut Vec<(String, serde_json::Value)>) {
    let mut entries = vec![(
        T::name().to_string(),
        <T as utoipa::PartialSchema>::schema(),
    )];
    <T as utoipa::ToSchema>::schemas(&mut entries);
    schemas.extend(
        entries
            .into_iter()
            .map(|(name, schema)| {
                (
                    name,
                    serde_json::to_value(schema)
                        .expect("utoipa schema serialization should not fail"),
                )
            })
            .collect::<Vec<_>>(),
    );
}

#[test]
fn openapi_document_uses_schema_refs_from_route_metadata() {
    let routes = [RouteMetadata::with_openapi_annotations(
        "POST",
        "/users",
        Some("Create user"),
        &["users"],
        &[],
        &[],
        true,
    )
    .with_openapi_schemas(Some("CreateUserDto"), Some("UserDto"))];

    let document = OpenApiDocument::from_route_metadata("Nidus API", "0.1.0", &routes)
        .schema::<CreateUserDto>()
        .schema::<UserDto>();

    let json = document.to_json_value();
    assert_eq!(
        json["paths"]["/users"]["post"]["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/CreateUserDto"
    );
    assert_eq!(
        json["paths"]["/users"]["post"]["responses"]["200"]["content"]["application/json"]["schema"]
            ["$ref"],
        "#/components/schemas/UserDto"
    );
}

#[test]
fn openapi_document_uses_response_status_from_route_metadata() {
    let routes = [RouteMetadata::with_openapi_annotations(
        "POST",
        "/users",
        Some("Create user"),
        &["users"],
        &[],
        &[],
        true,
    )
    .with_openapi_status(Some(http::StatusCode::CREATED))
    .with_openapi_schemas(Some("CreateUserDto"), Some("UserDto"))];

    let document = OpenApiDocument::from_route_metadata("Nidus API", "0.1.0", &routes)
        .schema::<CreateUserDto>()
        .schema::<UserDto>();

    let json = document.to_json_value();
    assert_eq!(
        json["paths"]["/users"]["post"]["responses"]["201"]["content"]["application/json"]["schema"]
            ["$ref"],
        "#/components/schemas/UserDto"
    );
    assert!(json["paths"]["/users"]["post"]["responses"]["200"].is_null());
}

#[test]
fn openapi_document_try_from_route_metadata_rejects_invalid_route_path() {
    let routes = [RouteMetadata::new("GET", "/:")];

    let error = match OpenApiDocument::try_from_route_metadata("Nidus API", "0.1.0", &routes) {
        Ok(_) => panic!("empty route parameter should fail"),
        Err(error) => error,
    };

    let OpenApiDocumentError::RoutePath(error) = error else {
        panic!("expected route path error");
    };
    assert_eq!(error.path(), "/:");
}

#[test]
fn openapi_document_try_from_route_metadata_rejects_duplicate_operations() {
    let routes = [
        RouteMetadata::with_summary("GET", "/users/:id", "Find user"),
        RouteMetadata::with_summary("GET", "/users/{id}", "Find same user"),
    ];

    let error =
        OpenApiDocument::try_from_route_metadata("Nidus API", "0.1.0", &routes).unwrap_err();

    assert_eq!(
        error,
        OpenApiDocumentError::DuplicateOperation {
            method: "get".to_owned(),
            path: "/users/{id}".to_owned()
        }
    );
}

#[test]
fn openapi_document_can_be_generated_from_controller_route_metadata() {
    let routes = [RouteMetadata::with_summary(
        "GET",
        "/:id",
        "Find user by ID",
    )];

    let document = OpenApiDocument::from_controller_routes("Nidus API", "0.1.0", "/users", &routes);

    let json = document.to_json_value();
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["summary"],
        "Find user by ID"
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["parameters"][0]["name"],
        "id"
    );
}

#[test]
fn openapi_document_builder_adds_controller_route_metadata() {
    let user_routes = [RouteMetadata::with_summary("GET", "/:id", "Find user")];
    let admin_routes = [RouteMetadata::with_summary(
        "GET",
        "/health",
        "Admin health",
    )];

    let document = OpenApiDocument::new("Nidus API", "0.1.0")
        .controller_routes("/users", &user_routes)
        .controller_routes("/admin", &admin_routes);

    let json = document.to_json_value();
    assert_eq!(json["paths"]["/users/{id}"]["get"]["summary"], "Find user");
    assert_eq!(
        json["paths"]["/admin/health"]["get"]["summary"],
        "Admin health"
    );
}

#[test]
fn openapi_document_try_controller_routes_rejects_invalid_prefix() {
    let routes = [RouteMetadata::new("GET", "/")];

    let error =
        match OpenApiDocument::new("Nidus API", "0.1.0").try_controller_routes("/:", &routes) {
            Ok(_) => panic!("empty route parameter should fail"),
            Err(error) => error,
        };

    let OpenApiDocumentError::RoutePath(error) = error else {
        panic!("expected route path error");
    };
    assert_eq!(error.path(), "/:");
}

#[test]
fn openapi_document_try_from_controller_routes_rejects_invalid_route_path() {
    let routes = [RouteMetadata::new("GET", "/:")];

    let error = match OpenApiDocument::try_from_controller_routes(
        "Nidus API",
        "0.1.0",
        "/users",
        &routes,
    ) {
        Ok(_) => panic!("empty route parameter should fail"),
        Err(error) => error,
    };

    let OpenApiDocumentError::RoutePath(error) = error else {
        panic!("expected route path error");
    };
    assert_eq!(error.path(), "/:");
}

/// Audit O-2: the generated document must expose exactly the declared routes
/// (no missing, no extra, correct methods) so the spec and the router built
/// from the same `RouteMetadata` can never silently diverge.
#[test]
fn openapi_document_paths_match_declared_route_metadata_exactly() {
    let routes = [
        RouteMetadata::new("GET", "/users"),
        RouteMetadata::new("POST", "/users"),
        RouteMetadata::new("GET", "/users/:id"),
        RouteMetadata::new("DELETE", "/users/:id"),
        RouteMetadata::new("PATCH", "/users/:id/tasks/:task_id"),
    ];

    let document = OpenApiDocument::from_route_metadata("Nidus API", "0.1.0", &routes);
    let json = document.to_json_value();
    let paths = json["paths"].as_object().expect("paths must be an object");

    // Every declared route is present in its braced (OpenAPI) form.
    let mut path_keys: Vec<&String> = paths.keys().collect();
    path_keys.sort();
    assert_eq!(
        path_keys,
        vec!["/users", "/users/{id}", "/users/{id}/tasks/{task_id}"],
        "paths must match declared routes exactly: {paths:?}"
    );

    // Method parity per path (collected and sorted to be order-independent).
    let sorted_methods = |path: &str| -> Vec<&str> {
        let mut methods: Vec<&str> = paths[path]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        methods.sort();
        methods
    };

    assert_eq!(sorted_methods("/users"), vec!["get", "post"]);
    assert_eq!(sorted_methods("/users/{id}"), vec!["delete", "get"]);
    assert_eq!(sorted_methods("/users/{id}/tasks/{task_id}"), vec!["patch"]);

    // A declared operationId must exist for every (path, method) so nothing is
    // silently dropped during the metadata -> spec conversion.
    for (path, ops) in paths {
        for (method, operation) in ops.as_object().unwrap() {
            assert!(
                operation.get("operationId").is_some(),
                "missing operationId for {method} {path}: {operation:?}"
            );
        }
    }
}

/// Audit O-2 (controller prefix): the prefixed builder must surface exactly the
/// prefixed routes, guarding against prefix-joining divergence.
#[test]
fn openapi_controller_routes_paths_match_declared_metadata_exactly() {
    let routes = [
        RouteMetadata::new("GET", "/"),
        RouteMetadata::new("POST", "/"),
        RouteMetadata::new("GET", "/:id"),
    ];

    let document =
        OpenApiDocument::from_controller_routes("Nidus API", "0.1.0", "/projects", &routes);
    let json = document.to_json_value();
    let paths = json["paths"].as_object().expect("paths must be an object");

    let mut path_keys: Vec<&String> = paths.keys().collect();
    path_keys.sort();
    assert_eq!(
        path_keys,
        vec!["/projects", "/projects/{id}"],
        "prefixed paths must match declared routes exactly: {paths:?}"
    );

    let sorted_methods = |path: &str| -> Vec<&str> {
        let mut methods: Vec<&str> = paths[path]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        methods.sort();
        methods
    };
    assert_eq!(sorted_methods("/projects"), vec!["get", "post"]);
    assert_eq!(sorted_methods("/projects/{id}"), vec!["get"]);
}
