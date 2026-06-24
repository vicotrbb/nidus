use axum::Router;
use nidus_core::{Inject, Module, ModuleBuilder, ModuleDefinition, NidusError};
use nidus_testing::TestApp;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Debug, PartialEq, Eq)]
struct UsersRepository(&'static str);

#[derive(Debug)]
struct RequestUsersRepository {
    request_id: Inject<RequestId>,
}

#[derive(Debug, PartialEq, Eq)]
struct RequestId(usize);

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
fn test_app_resolves_transient_providers_as_fresh_instances() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = TestApp::builder(Router::new())
        .transient_provider::<RequestId, _>({
            let calls = Arc::clone(&calls);
            move |_container| Ok(RequestId(calls.fetch_add(1, Ordering::SeqCst)))
        })
        .unwrap()
        .build();

    let first = app.resolve::<RequestId>().unwrap();
    let second = app.resolve::<RequestId>().unwrap();

    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(first.0, 0);
    assert_eq!(second.0, 1);
}

#[test]
fn test_app_resolves_request_providers_through_request_scope() {
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

#[test]
fn test_app_request_scoped_providers_resolve_dependencies_through_request_scope() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = TestApp::builder(Router::new())
        .request_provider::<RequestId, _>({
            let calls = Arc::clone(&calls);
            move |_container| Ok(RequestId(calls.fetch_add(1, Ordering::SeqCst)))
        })
        .unwrap()
        .request_scoped_provider::<RequestUsersRepository, _>(|scope| {
            Ok(RequestUsersRepository {
                request_id: scope.inject::<RequestId>()?,
            })
        })
        .unwrap()
        .build();

    let scope = app.request_scope();
    let repository = scope.resolve::<RequestUsersRepository>().unwrap();
    let request_id = scope.resolve::<RequestId>().unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(Arc::ptr_eq(
        &repository.request_id.clone().into_inner(),
        &request_id
    ));
}
