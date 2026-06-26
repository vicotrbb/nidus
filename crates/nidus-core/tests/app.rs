use std::{future::Future, pin::Pin};

use nidus_core::{
    Container, LifecycleRunner, Module, ModuleBuilder, Nidus, NidusError, ProviderRegistrant,
    Result,
};

struct AppModule;
struct UsersModule;

impl Module for AppModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("AppModule")
            .import("UsersModule")
            .build()
    }
}

impl Module for UsersModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("UsersModule")
            .provider("UsersService")
            .export("UsersService")
            .build()
    }
}

struct TypedAppModule;

impl Module for TypedAppModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("TypedAppModule")
            .import_typed::<UsersModule>()
            .build()
    }
}

#[test]
fn bootstrap_recursively_follows_typed_module_imports() {
    let app = Nidus::bootstrap::<TypedAppModule>().unwrap();

    assert!(app.modules().get("TypedAppModule").is_some());
    assert!(app.modules().get("UsersModule").is_some());
}

#[test]
fn bootstrap_with_modules_validates_explicit_module_graph() {
    let app = Nidus::bootstrap_with_modules::<AppModule, _>([UsersModule::definition()]).unwrap();

    assert!(app.modules().get("AppModule").is_some());
    assert!(app.modules().get("UsersModule").is_some());
}

#[test]
fn bootstrap_with_modules_rejects_missing_explicit_imports() {
    let error = match Nidus::bootstrap_with_modules::<AppModule, _>([]) {
        Ok(_) => panic!("missing module import should fail"),
        Err(error) => error,
    };

    assert!(matches!(error, NidusError::MissingModuleImport { .. }));
    assert!(error.to_string().contains("UsersModule"));
}

#[derive(Debug)]
struct Database(&'static str);

impl ProviderRegistrant for Database {
    fn register_provider(container: &mut Container) -> Result<()> {
        container.register_singleton(Database("from-module"))
    }
}

struct ProviderAppModule;

impl Module for ProviderAppModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("ProviderAppModule")
            .provider_typed::<Database>()
            .build()
    }
}

#[test]
fn bootstrap_registers_typed_module_providers() {
    let app = Nidus::bootstrap::<ProviderAppModule>().unwrap();

    let database = app.container().resolve::<Database>().unwrap();

    assert_eq!(database.0, "from-module");
}

fn initialize_database(
    container: &mut Container,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
    Box::pin(async move {
        container.register_singleton(Database("async-initialized"))?;
        Ok(())
    })
}

struct AsyncProviderAppModule;

impl Module for AsyncProviderAppModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("AsyncProviderAppModule")
            .async_initializer(initialize_database)
            .build()
    }
}

#[tokio::test]
async fn bootstrap_with_lifecycle_runs_async_provider_initializers() {
    let app = Nidus::bootstrap_with_lifecycle::<AsyncProviderAppModule>(LifecycleRunner::new())
        .await
        .unwrap();

    let database = app.container().resolve::<Database>().unwrap();

    assert_eq!(database.0, "async-initialized");
}
