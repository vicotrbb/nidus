use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use nidus::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tower::ServiceExt;

#[injectable]
struct GreetingService;

impl GreetingService {
    fn greeting(&self) -> &'static str {
        "hello from module DI"
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
struct UserDto {
    id: i64,
    email: String,
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
struct CreateUserDto {
    email: String,
}

#[controller("/users")]
struct ApiUsersController;

#[routes]
impl ApiUsersController {
    #[post("/")]
    #[openapi(
        summary = "Create user",
        tags = ["users"],
        status = 201,
        request = CreateUserDto,
        response = UserDto
    )]
    async fn create_user(&self, Json(input): Json<CreateUserDto>) -> (StatusCode, Json<UserDto>) {
        (
            StatusCode::CREATED,
            Json(UserDto {
                id: 1,
                email: input.email,
            }),
        )
    }
}

#[module]
struct ApiUsersModule {
    controllers: [ApiUsersController],
}

#[module]
struct ApiModule {
    imports: [ApiUsersModule],
}

#[controller("/greetings")]
struct GreetingController {
    service: Inject<GreetingService>,
}

#[routes]
impl GreetingController {
    #[get("/")]
    async fn greet(&self) -> String {
        self.service.greeting().to_owned()
    }
}

#[module]
struct GreetingModule {
    providers: [GreetingService],
    controllers: [GreetingController],
}

#[module]
struct AppModule {
    imports: [GreetingModule],
}

#[module]
struct MissingProviderModule {
    controllers: [GreetingController],
}

#[tokio::test]
async fn root_module_builds_provider_backed_controller_routes() {
    let app = Nidus::create::<AppModule>().build().await.unwrap();
    let router = app.into_router();

    let response = router
        .oneshot(
            Request::builder()
                .uri("/greetings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"hello from module DI");
}

#[tokio::test]
async fn controller_dependency_errors_surface_during_build() {
    let error = match Nidus::create::<MissingProviderModule>().build().await {
        Ok(_) => panic!("missing controller dependency should fail"),
        Err(error) => error,
    };

    assert!(matches!(error, NidusError::MissingProvider { .. }));
    assert!(error.to_string().contains("GreetingService"));
}

#[cfg(feature = "openapi")]
#[tokio::test]
async fn openapi_builder_auto_registers_schema_metadata() {
    let app = Nidus::create::<ApiModule>()
        .with_openapi("Nidus API", "0.1.0")
        .build()
        .await
        .unwrap();
    let router = app.into_router();

    let response = router
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let openapi: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(openapi["paths"]["/users"]["post"]["summary"], "Create user");
    assert_eq!(
        openapi["paths"]["/users"]["post"]["requestBody"]["content"]["application/json"]["schema"]
            ["$ref"],
        "#/components/schemas/CreateUserDto"
    );
    assert_eq!(
        openapi["paths"]["/users"]["post"]["responses"]["201"]["content"]["application/json"]["schema"]
            ["$ref"],
        "#/components/schemas/UserDto"
    );
    assert!(openapi["components"]["schemas"]["CreateUserDto"].is_object());
    assert!(openapi["components"]["schemas"]["UserDto"].is_object());
}

#[cfg(feature = "openapi")]
#[tokio::test]
async fn openapi_builder_preserves_fallback_schema_registrations() {
    let app = Nidus::create::<ApiModule>()
        .with_openapi("Nidus API", "0.1.0")
        .with_schema::<CreateUserDto>()
        .build()
        .await
        .unwrap();
    let router = app.into_router();

    let response = router
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let openapi: Value = serde_json::from_slice(&body).unwrap();
    let schemas = openapi["components"]["schemas"]
        .as_object()
        .expect("schemas should be an object");
    assert_eq!(schemas.len(), 2);
    assert!(schemas.contains_key("CreateUserDto"));
    assert!(schemas.contains_key("UserDto"));
}
