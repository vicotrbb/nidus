use axum::Router;
use nidus_core::{Module, ModuleBuilder, ModuleDefinition, NidusError};
use nidus_testing::TestApp;

#[derive(Debug, PartialEq, Eq)]
struct UsersRepository(&'static str);

struct AppModule;

impl Module for AppModule {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("AppModule").build()
    }
}

struct BrokenModule;

impl Module for BrokenModule {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("BrokenModule")
            .import("MissingModule")
            .build()
    }
}

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

#[test]
fn test_app_bootstrap_validates_module_and_supports_provider_overrides() {
    let app = TestApp::bootstrap::<AppModule>()
        .unwrap()
        .provider(UsersRepository("real"))
        .unwrap()
        .override_provider(UsersRepository("mock"))
        .unwrap()
        .build();

    let repository = app.resolve::<UsersRepository>().unwrap();

    assert_eq!(repository.0, "mock");
}

#[test]
fn test_app_bootstrap_reports_invalid_module_graphs() {
    let error = match TestApp::bootstrap::<BrokenModule>() {
        Ok(_) => panic!("broken module graph should fail"),
        Err(error) => error,
    };

    assert!(matches!(
        error,
        NidusError::MissingModuleImport {
            module,
            import
        } if module == "BrokenModule" && import == "MissingModule"
    ));
}
