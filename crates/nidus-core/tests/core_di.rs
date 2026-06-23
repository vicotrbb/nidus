use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nidus_core::{Container, Inject, ModuleBuilder, ModuleGraph, NidusError, ProviderLifetime};

#[derive(Debug, PartialEq, Eq)]
struct Database(&'static str);

#[derive(Debug)]
struct UsersRepository {
    database: Inject<Database>,
}

#[test]
fn container_resolves_typed_singletons_without_string_tokens() {
    let mut container = Container::new();
    container.register_singleton(Database("primary")).unwrap();
    let database = container.resolve::<Database>().unwrap();

    assert_eq!(database.0, "primary");
}

#[test]
fn container_can_build_injected_provider_from_typed_dependency() {
    let mut container = Container::new();
    container.register_singleton(Database("primary")).unwrap();
    container
        .register_factory::<UsersRepository, _>(ProviderLifetime::Singleton, |container| {
            Ok(UsersRepository {
                database: container.inject::<Database>()?,
            })
        })
        .unwrap();

    let repository = container.resolve::<UsersRepository>().unwrap();

    assert_eq!(repository.database.0, "primary");
}

#[test]
fn singleton_factories_reuse_one_instance() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut container = Container::new();
    container
        .register_factory::<Database, _>(ProviderLifetime::Singleton, {
            let calls = Arc::clone(&calls);
            move |_container| {
                let call = calls.fetch_add(1, Ordering::SeqCst);
                Ok(Database(if call == 0 { "first" } else { "second" }))
            }
        })
        .unwrap();

    let first = container.resolve::<Database>().unwrap();
    let second = container.resolve::<Database>().unwrap();

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn transient_factories_create_each_resolution() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut container = Container::new();
    container
        .register_factory::<Database, _>(ProviderLifetime::Transient, {
            let calls = Arc::clone(&calls);
            move |_container| {
                let call = calls.fetch_add(1, Ordering::SeqCst);
                Ok(Database(if call == 0 { "first" } else { "second" }))
            }
        })
        .unwrap();

    let first = container.resolve::<Database>().unwrap();
    let second = container.resolve::<Database>().unwrap();

    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(first.0, "first");
    assert_eq!(second.0, "second");
}

#[test]
fn request_factories_reuse_within_scope_but_not_across_scopes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut container = Container::new();
    container
        .register_factory::<Database, _>(ProviderLifetime::Request, {
            let calls = Arc::clone(&calls);
            move |_container| {
                let call = calls.fetch_add(1, Ordering::SeqCst);
                Ok(Database(if call == 0 { "first" } else { "second" }))
            }
        })
        .unwrap();

    let first_scope = container.request_scope();
    let first = first_scope.resolve::<Database>().unwrap();
    let first_again = first_scope.resolve::<Database>().unwrap();
    let second_scope = container.request_scope();
    let second = second_scope.resolve::<Database>().unwrap();

    assert!(Arc::ptr_eq(&first, &first_again));
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(first.0, "first");
    assert_eq!(second.0, "second");
}

#[test]
fn container_rejects_duplicate_providers() {
    let mut container = Container::new();
    container.register_singleton(Database("primary")).unwrap();
    let error = container
        .register_singleton(Database("replica"))
        .unwrap_err();

    assert!(matches!(error, NidusError::DuplicateProvider { .. }));
}

#[test]
fn container_reports_missing_provider_type_name() {
    let container = Container::new();
    let error = container.resolve::<Database>().unwrap_err();

    assert!(matches!(error, NidusError::MissingProvider { .. }));
    assert!(error.to_string().contains("Database"));
}

#[test]
fn module_builder_records_explicit_imports_providers_controllers_and_exports() {
    let definition = ModuleBuilder::new("UsersModule")
        .import("DatabaseModule")
        .provider("UsersRepository")
        .provider("UsersService")
        .controller("UsersController")
        .export("UsersService")
        .build();

    assert_eq!(definition.name(), "UsersModule");
    assert_eq!(definition.imports(), ["DatabaseModule"]);
    assert_eq!(definition.providers(), ["UsersRepository", "UsersService"]);
    assert_eq!(definition.controllers(), ["UsersController"]);
    assert_eq!(definition.exports(), ["UsersService"]);
}

#[test]
fn module_graph_detects_circular_imports() {
    let users = ModuleBuilder::new("UsersModule")
        .import("BillingModule")
        .build();
    let billing = ModuleBuilder::new("BillingModule")
        .import("UsersModule")
        .build();

    let error = ModuleGraph::from_modules([users, billing]).unwrap_err();

    assert!(matches!(error, NidusError::CircularModuleImport { .. }));
}
