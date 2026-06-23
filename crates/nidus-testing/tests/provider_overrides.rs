use axum::Router;
use nidus_testing::TestApp;

#[derive(Debug, PartialEq, Eq)]
struct UsersRepository(&'static str);

#[test]
fn test_app_builder_overrides_registered_provider() {
    let app = TestApp::builder(Router::new())
        .provider(UsersRepository("real"))
        .unwrap()
        .override_provider(UsersRepository("mock"))
        .unwrap()
        .build();

    let repository = app.resolve::<UsersRepository>().unwrap();

    assert_eq!(repository.0, "mock");
}
