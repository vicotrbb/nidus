use axum::Router;
use nidus_core::{Module, ModuleBuilder, ModuleDefinition, NidusError};
use nidus_testing::TestApp;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

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

struct ModularAppModule;
struct UsersModule;

impl Module for ModularAppModule {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("ModularAppModule")
            .import("UsersModule")
            .build()
    }
}

impl Module for UsersModule {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("UsersModule")
            .provider("UsersRepository")
            .export("UsersRepository")
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

#[test]
fn test_app_bootstrap_with_modules_validates_explicit_module_graph() {
    let app = TestApp::bootstrap_with_modules::<ModularAppModule, _>([UsersModule::definition()])
        .unwrap()
        .provider(UsersRepository("real"))
        .unwrap()
        .build();

    let repository = app.resolve::<UsersRepository>().unwrap();

    assert_eq!(repository.0, "real");
}

#[test]
fn test_app_resolves_request_providers_through_request_scope() {
    #[derive(Debug, PartialEq, Eq)]
    struct RequestId(usize);

    let calls = Arc::new(AtomicUsize::new(0));
    let app = TestApp::builder(Router::new())
        .request_provider::<RequestId, _>({
            let calls = Arc::clone(&calls);
            move |_container| Ok(RequestId(calls.fetch_add(1, Ordering::SeqCst)))
        })
        .unwrap()
        .build();

    assert!(matches!(
        app.resolve::<RequestId>().unwrap_err(),
        NidusError::RequestScopeRequired { .. }
    ));

    let first_scope = app.request_scope();
    let first = first_scope.resolve::<RequestId>().unwrap();
    let first_again = first_scope.resolve::<RequestId>().unwrap();
    let second_scope = app.request_scope();
    let second = second_scope.resolve::<RequestId>().unwrap();

    assert!(Arc::ptr_eq(&first, &first_again));
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(first.0, 0);
    assert_eq!(second.0, 1);
}
