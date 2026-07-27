use std::{
    future::Future,
    pin::Pin,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

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

struct SharedExplicitDependencyModule;

impl Module for SharedExplicitDependencyModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("SharedExplicitDependencyModule")
            .provider_typed::<SharedExplicitProvider>()
            .async_initializer(initialize_shared_explicit_dependency)
            .build()
    }
}

#[derive(Debug)]
struct SharedExplicitProvider;

static SHARED_EXPLICIT_PROVIDER_REGISTRATIONS: AtomicUsize = AtomicUsize::new(0);

impl ProviderRegistrant for SharedExplicitProvider {
    fn register_provider(container: &mut Container) -> Result<()> {
        SHARED_EXPLICIT_PROVIDER_REGISTRATIONS.fetch_add(1, Ordering::SeqCst);
        container.register_singleton(SharedExplicitProvider)
    }
}

#[derive(Debug)]
struct SharedExplicitAsyncReady;

static EXPLICIT_INITIALIZATION_ORDER: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

fn initialize_shared_explicit_dependency(
    container: &mut Container,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
    Box::pin(async move {
        EXPLICIT_INITIALIZATION_ORDER.lock().unwrap().push("shared");
        container.register_singleton(SharedExplicitAsyncReady)
    })
}

struct FirstExplicitFeatureModule;

impl Module for FirstExplicitFeatureModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("FirstExplicitFeatureModule")
            .import_typed::<SharedExplicitDependencyModule>()
            .async_initializer(initialize_first_explicit_feature)
            .build()
    }
}

fn initialize_first_explicit_feature(
    container: &mut Container,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
    Box::pin(async move {
        container.resolve::<SharedExplicitAsyncReady>()?;
        EXPLICIT_INITIALIZATION_ORDER.lock().unwrap().push("first");
        Ok(())
    })
}

struct SecondExplicitFeatureModule;

impl Module for SecondExplicitFeatureModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("SecondExplicitFeatureModule")
            .import_typed::<SharedExplicitDependencyModule>()
            .async_initializer(initialize_second_explicit_feature)
            .build()
    }
}

fn initialize_second_explicit_feature(
    container: &mut Container,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
    Box::pin(async move {
        container.resolve::<SharedExplicitAsyncReady>()?;
        EXPLICIT_INITIALIZATION_ORDER.lock().unwrap().push("second");
        Ok(())
    })
}

struct ExplicitDiamondAppModule;

impl Module for ExplicitDiamondAppModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("ExplicitDiamondAppModule")
            .import("FirstExplicitFeatureModule")
            .import("SecondExplicitFeatureModule")
            .build()
    }
}

#[tokio::test]
async fn bootstrap_with_modules_follows_shared_typed_dependencies_once() {
    SHARED_EXPLICIT_PROVIDER_REGISTRATIONS.store(0, Ordering::SeqCst);
    EXPLICIT_INITIALIZATION_ORDER.lock().unwrap().clear();

    let app = Nidus::bootstrap_with_modules_and_lifecycle::<ExplicitDiamondAppModule, _>(
        [
            FirstExplicitFeatureModule::definition(),
            SecondExplicitFeatureModule::definition(),
        ],
        LifecycleRunner::new(),
    )
    .await
    .unwrap();

    assert!(
        app.modules()
            .get("SharedExplicitDependencyModule")
            .is_some()
    );
    assert_eq!(app.modules().modules().count(), 4);
    assert_eq!(
        SHARED_EXPLICIT_PROVIDER_REGISTRATIONS.load(Ordering::SeqCst),
        1
    );
    assert_eq!(
        *EXPLICIT_INITIALIZATION_ORDER.lock().unwrap(),
        ["shared", "first", "second"]
    );
}

#[test]
fn bootstrap_with_modules_still_rejects_duplicate_explicit_definitions() {
    let error = match Nidus::bootstrap_with_modules::<AppModule, _>([
        UsersModule::definition(),
        UsersModule::definition(),
    ]) {
        Ok(_) => panic!("duplicate explicit modules should fail"),
        Err(error) => error,
    };

    assert!(matches!(
        error,
        NidusError::DuplicateModule { ref module } if module == "UsersModule"
    ));
}

#[test]
fn bootstrap_with_modules_rejects_explicit_definition_already_followed_from_root() {
    let error =
        match Nidus::bootstrap_with_modules::<TypedAppModule, _>([UsersModule::definition()]) {
            Ok(_) => panic!("typed and explicit duplicate modules should fail"),
            Err(error) => error,
        };

    assert!(matches!(
        error,
        NidusError::DuplicateModule { ref module } if module == "UsersModule"
    ));
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
