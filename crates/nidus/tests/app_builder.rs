use axum::{
    body::{Body, to_bytes},
    http::Request,
    routing::get,
};
use nidus::prelude::*;
#[cfg(feature = "openapi")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use serde_json::Value;
use std::{future::Future, pin::Pin};
use tower::ServiceExt;

#[injectable]
struct GreetingService;

impl GreetingService {
    fn greeting(&self) -> &'static str {
        "hello from module DI"
    }
}

#[cfg(feature = "openapi")]
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct UserDto {
    id: i64,
    email: String,
}

#[cfg(feature = "openapi")]
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
struct CreateUserDto {
    email: String,
}

#[cfg(feature = "openapi")]
#[controller("/users")]
struct ApiUsersController;

#[cfg(feature = "openapi")]
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

#[cfg(feature = "openapi")]
#[module]
struct ApiUsersModule {
    controllers: [ApiUsersController],
}

#[cfg(feature = "openapi")]
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

#[derive(Debug)]
struct ImportedInitializerReady;

#[derive(Debug)]
struct ImporterInitializerReady;

fn initialize_imported_module(
    container: &mut Container,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
    Box::pin(async move {
        container.register_singleton(ImportedInitializerReady)?;
        Ok(())
    })
}

fn initialize_importer_module(
    container: &mut Container,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
    Box::pin(async move {
        container.resolve::<ImportedInitializerReady>()?;
        container.register_singleton(ImporterInitializerReady)?;
        Ok(())
    })
}

struct ZFacadeImportedModule;

impl Module for ZFacadeImportedModule {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("ZFacadeImportedModule")
            .async_initializer(initialize_imported_module)
            .build()
    }
}

struct AFacadeImporterModule;

impl Module for AFacadeImporterModule {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("AFacadeImporterModule")
            .import_typed::<ZFacadeImportedModule>()
            .async_initializer(initialize_importer_module)
            .build()
    }
}

#[cfg(feature = "auth")]
#[injectable]
struct FirstHeaderGuard;

#[cfg(feature = "auth")]
#[async_trait::async_trait]
impl Guard<()> for FirstHeaderGuard {
    async fn check(&self, ctx: GuardContext<()>) -> std::result::Result<(), GuardError> {
        match ctx.header_str("x-first-guard")? {
            Some("allowed") => Ok(()),
            _ => Err(GuardError::unauthorized("first guard rejected request")),
        }
    }
}

#[cfg(feature = "auth")]
#[injectable]
struct SecondHeaderGuard;

#[cfg(feature = "auth")]
#[async_trait::async_trait]
impl Guard<()> for SecondHeaderGuard {
    async fn check(&self, ctx: GuardContext<()>) -> std::result::Result<(), GuardError> {
        match ctx.header_str("x-second-guard")? {
            Some("allowed") => Ok(()),
            _ => Err(GuardError::forbidden("second guard rejected request")),
        }
    }
}

#[cfg(feature = "auth")]
#[controller("/guarded")]
struct MultiGuardController;

#[cfg(feature = "auth")]
#[routes]
impl MultiGuardController {
    #[get("/")]
    #[guard(FirstHeaderGuard)]
    #[guard(SecondHeaderGuard)]
    async fn guarded(&self) -> &'static str {
        "authorized"
    }
}

#[cfg(feature = "auth")]
#[module]
struct GuardedModule {
    providers: (FirstHeaderGuard, SecondHeaderGuard),
    controllers: [MultiGuardController],
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
async fn builder_merges_manual_router_without_application_http_ext_import() {
    let app = Nidus::create::<AppModule>()
        .with_router(Router::new().route("/manual", get(|| async { "manual route" })))
        .build()
        .await
        .unwrap();
    let router = app.into_router();

    let module_response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/greetings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(module_response.status(), StatusCode::OK);

    let manual_response = router
        .oneshot(
            Request::builder()
                .uri("/manual")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(manual_response.status(), StatusCode::OK);
    let body = to_bytes(manual_response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"manual route");
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

#[tokio::test]
async fn builder_initializes_imported_modules_before_importers() {
    let app = Nidus::create::<AFacadeImporterModule>()
        .build()
        .await
        .unwrap();

    app.application()
        .container()
        .resolve::<ImporterInitializerReady>()
        .unwrap();
}

#[cfg(feature = "auth")]
#[tokio::test]
async fn module_composed_route_executes_each_guard_with_request_headers() {
    let app = Nidus::create::<GuardedModule>().build().await.unwrap();
    let router = app.into_router();

    let rejected = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/guarded")
                .header("x-first-guard", "allowed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);

    let authorized = router
        .oneshot(
            Request::builder()
                .uri("/guarded")
                .header("x-first-guard", "allowed")
                .header("x-second-guard", "allowed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
    let body = to_bytes(authorized.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"authorized");
}

#[cfg(feature = "openapi")]
#[tokio::test]
async fn openapi_builder_auto_registers_schema_metadata() {
    let app = Nidus::create::<ApiModule>()
        .with_openapi("Nidus API", "1.0.0")
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
        .with_openapi("Nidus API", "1.0.0")
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
