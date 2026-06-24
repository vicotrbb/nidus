use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nidus_core::{Container, Inject, NidusError, ProviderLifetime};

#[derive(Debug, PartialEq, Eq)]
struct Database(&'static str);

#[derive(Debug)]
struct UsersRepository {
    database: Inject<Database>,
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
