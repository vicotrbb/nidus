use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use nidus::prelude::*;
use tower::ServiceExt;

#[injectable]
struct GreetingService;

impl GreetingService {
    fn greeting(&self) -> &'static str {
        "hello from module DI"
    }
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
