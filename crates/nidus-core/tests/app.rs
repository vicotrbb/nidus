use nidus_core::{Module, ModuleBuilder, Nidus, NidusError};

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
