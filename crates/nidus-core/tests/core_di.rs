use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nidus_core::{Container, Factory, Inject, Lazy, NidusError, ProviderLifetime};

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
fn lazy_dependency_resolves_only_when_requested() {
    let calls = Arc::new(AtomicUsize::new(0));
    let lazy = Lazy::new({
        let calls = Arc::clone(&calls);
        move || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(Inject::new(Arc::new(Database("lazy"))))
        }
    });

    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let database = lazy.get().unwrap();

    assert_eq!(database.0, "lazy");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn lazy_dependency_preserves_resolution_errors() {
    let lazy = Lazy::<Database>::new(|| {
        Err(NidusError::MissingProvider {
            type_name: "Database",
        })
    });

    let error = lazy.get().unwrap_err();

    assert!(matches!(error, NidusError::MissingProvider { .. }));
    assert!(error.to_string().contains("Database"));
}

#[test]
fn factory_dependency_creates_fresh_values() {
    let calls = Arc::new(AtomicUsize::new(0));
    let factory = Factory::new({
        let calls = Arc::clone(&calls);
        move || {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            Ok(Database(if call == 0 { "first" } else { "second" }))
        }
    });

    let first = factory.create().unwrap();
    let second = factory.create().unwrap();

    assert_eq!(first.0, "first");
    assert_eq!(second.0, "second");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn container_resolves_optional_dependency_when_present() {
    let mut container = Container::new();
    container.register_singleton(Database("primary")).unwrap();

    let database = container.optional::<Database>().unwrap();

    assert!(database.is_some());
    assert_eq!(database.as_ref().unwrap().0, "primary");
}

#[test]
fn container_resolves_optional_dependency_as_none_when_missing() {
    let container = Container::new();

    let database = container.optional::<Database>().unwrap();

    assert!(database.is_none());
    assert!(database.into_option().is_none());
}

#[test]
fn optional_dependency_resolution_preserves_factory_errors() {
    let mut container = Container::new();
    container
        .register_factory::<UsersRepository, _>(ProviderLifetime::Transient, |container| {
            Ok(UsersRepository {
                database: container.inject::<Database>()?,
            })
        })
        .unwrap();

    let error = container.optional::<UsersRepository>().unwrap_err();

    assert!(matches!(error, NidusError::ProviderFactory { .. }));
    assert!(error.to_string().contains("UsersRepository"));
}

#[test]
fn singleton_factories_reuse_one_instance() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut container = Container::new();
    container
        .register_singleton_factory::<Database, _>({
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
        .register_transient::<Database, _>({
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
        .register_request::<Database, _>({
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
fn request_scope_resolves_scoped_wrapper() {
    let mut container = Container::new();
    container
        .register_request::<Database, _>(|_container| Ok(Database("request")))
        .unwrap();
    let scope = container.request_scope();

    let database = scope.scoped::<Database>().unwrap();

    assert_eq!(database.0, "request");
}

#[test]
fn request_scope_optional_reuses_request_scoped_instances() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut container = Container::new();
    container
        .register_factory::<Database, _>(ProviderLifetime::Request, {
            let calls = Arc::clone(&calls);
            move |_container| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(Database("request"))
            }
        })
        .unwrap();
    let scope = container.request_scope();

    let first = scope.optional::<Database>().unwrap().into_option().unwrap();
    let second = scope.optional::<Database>().unwrap().into_option().unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(Arc::ptr_eq(&first.into_inner(), &second.into_inner()));
}

#[test]
fn request_scoped_factories_resolve_dependencies_through_same_scope() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut container = Container::new();
    container
        .register_request::<Database, _>({
            let calls = Arc::clone(&calls);
            move |_container| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(Database("request"))
            }
        })
        .unwrap();
    container
        .register_request_scoped::<UsersRepository, _>(|scope| {
            Ok(UsersRepository {
                database: scope.inject::<Database>()?,
            })
        })
        .unwrap();

    let scope = container.request_scope();
    let repository = scope.resolve::<UsersRepository>().unwrap();
    let database = scope.resolve::<Database>().unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(Arc::ptr_eq(
        &repository.database.clone().into_inner(),
        &database
    ));
}

#[test]
fn request_factories_require_explicit_request_scope() {
    let mut container = Container::new();
    container
        .register_factory::<Database, _>(ProviderLifetime::Request, |_container| {
            Ok(Database("request"))
        })
        .unwrap();

    let error = container.resolve::<Database>().unwrap_err();

    assert!(matches!(error, NidusError::RequestScopeRequired { .. }));
    assert!(error.to_string().contains("Database"));
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
fn provider_factory_errors_include_provider_context() {
    let mut container = Container::new();
    container
        .register_factory::<UsersRepository, _>(ProviderLifetime::Transient, |container| {
            Ok(UsersRepository {
                database: container.inject::<Database>()?,
            })
        })
        .unwrap();

    let error = container.resolve::<UsersRepository>().unwrap_err();

    let NidusError::ProviderFactory { type_name, source } = error else {
        panic!("expected provider factory error");
    };
    assert!(type_name.contains("UsersRepository"));
    assert!(matches!(*source, NidusError::MissingProvider { .. }));
    assert!(source.to_string().contains("Database"));
}
