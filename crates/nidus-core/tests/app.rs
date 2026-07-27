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

#[derive(Debug)]
struct ImportedSyncProvider;

#[derive(Debug)]
struct ImporterSyncProvider;

impl ProviderRegistrant for ImportedSyncProvider {
    fn register_provider(container: &mut Container) -> Result<()> {
        container.register_singleton(ImportedSyncProvider)
    }
}

impl ProviderRegistrant for ImporterSyncProvider {
    fn register_provider(container: &mut Container) -> Result<()> {
        container.resolve::<ImportedSyncProvider>()?;
        container.register_singleton(ImporterSyncProvider)
    }
}

struct ZImportedSyncProviderModule;

impl Module for ZImportedSyncProviderModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("ZImportedSyncProviderModule")
            .provider_typed::<ImportedSyncProvider>()
            .build()
    }
}

struct AImporterSyncProviderModule;

impl Module for AImporterSyncProviderModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("AImporterSyncProviderModule")
            .import_typed::<ZImportedSyncProviderModule>()
            .provider_typed::<ImporterSyncProvider>()
            .build()
    }
}

#[test]
fn bootstrap_registers_imported_providers_before_importers() {
    let app = Nidus::bootstrap::<AImporterSyncProviderModule>().unwrap();

    app.container().resolve::<ImporterSyncProvider>().unwrap();
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

#[derive(Debug)]
struct ImportedDependencyReady;

#[derive(Debug)]
struct ImporterReady;

fn initialize_imported_dependency(
    container: &mut Container,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
    Box::pin(async move {
        container.register_singleton(ImportedDependencyReady)?;
        Ok(())
    })
}

fn initialize_importer(
    container: &mut Container,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
    Box::pin(async move {
        container.resolve::<ImportedDependencyReady>()?;
        container.register_singleton(ImporterReady)?;
        Ok(())
    })
}

struct ZImportedDependencyModule;

impl Module for ZImportedDependencyModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("ZImportedDependencyModule")
            .async_initializer(initialize_imported_dependency)
            .build()
    }
}

struct AImporterModule;

impl Module for AImporterModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("AImporterModule")
            .import_typed::<ZImportedDependencyModule>()
            .async_initializer(initialize_importer)
            .build()
    }
}

#[tokio::test]
async fn bootstrap_initializes_imported_modules_before_importers() {
    let app = Nidus::bootstrap_with_lifecycle::<AImporterModule>(LifecycleRunner::new())
        .await
        .unwrap();

    app.container().resolve::<ImporterReady>().unwrap();
}

fn fail_provider_initialization(
    _container: &mut Container,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
    Box::pin(async {
        Err(NidusError::ApplicationBuild {
            message: "initializer failed".to_owned(),
        })
    })
}

struct FailingInitializerModule;

impl Module for FailingInitializerModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("FailingInitializerModule")
            .async_initializer(fail_provider_initialization)
            .build()
    }
}

#[tokio::test]
async fn bootstrap_preserves_async_initializer_errors() {
    let error =
        match Nidus::bootstrap_with_lifecycle::<FailingInitializerModule>(LifecycleRunner::new())
            .await
        {
            Ok(_) => panic!("failing initializer should stop bootstrap"),
            Err(error) => error,
        };

    assert!(matches!(error, NidusError::ApplicationBuild { .. }));
    assert_eq!(
        error.to_string(),
        "application build failed: initializer failed"
    );
}
