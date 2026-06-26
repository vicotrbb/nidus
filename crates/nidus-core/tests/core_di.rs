use std::{
    any::type_name,
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use nidus_core::{Container, Factory, Inject, Lazy, NidusError, ProviderLifetime};

#[derive(Debug, PartialEq, Eq)]
struct Database(&'static str);

#[derive(Debug)]
struct UsersRepository {
    database: Inject<Database>,
}

#[derive(Debug)]
struct SelfReferential;

#[derive(Debug)]
struct CircularA {
    _b: Inject<CircularB>,
}

#[derive(Debug)]
struct CircularB {
    _a: Inject<CircularA>,
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
fn singleton_factory_runs_once_under_concurrent_first_resolution() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut container = Container::new();
    container
        .register_singleton_factory::<Database, _>({
            let calls = Arc::clone(&calls);
            move |_container| {
                calls.fetch_add(1, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(25));
                Ok(Database("primary"))
            }
        })
        .unwrap();
    let container = Arc::new(container);
    let ready = Arc::new(Barrier::new(8));

    let handles = (0..8)
        .map(|_| {
            let container = Arc::clone(&container);
            let ready = Arc::clone(&ready);
            thread::spawn(move || {
                ready.wait();
                container.resolve::<Database>().unwrap()
            })
        })
        .collect::<Vec<_>>();

    let instances = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();

    for instance in &instances[1..] {
        assert!(Arc::ptr_eq(&instances[0], instance));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn singleton_factory_reports_self_circular_resolution() {
    let mut container = Container::new();
    container
        .register_singleton_factory::<SelfReferential, _>(|container| {
            let _self_reference = container.inject::<SelfReferential>()?;
            Ok(SelfReferential)
        })
        .unwrap();

    let error = container.resolve::<SelfReferential>().unwrap_err();

    assert_circular_provider_error(error, type_name::<SelfReferential>());
}

#[test]
fn singleton_factory_reports_indirect_circular_resolution() {
    let mut container = Container::new();
    container
        .register_singleton_factory::<CircularA, _>(|container| {
            Ok(CircularA {
                _b: container.inject::<CircularB>()?,
            })
        })
        .unwrap();
    container
        .register_singleton_factory::<CircularB, _>(|container| {
            Ok(CircularB {
                _a: container.inject::<CircularA>()?,
            })
        })
        .unwrap();

    let error = container.resolve::<CircularA>().unwrap_err();

    assert_circular_provider_error(error, type_name::<CircularA>());
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

#[test]
fn singleton_factory_recovers_after_panic() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let mut container = Container::new();
    container
        .register_singleton_factory::<Database, _>({
            let attempts = Arc::clone(&attempts);
            move |_container| {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                if attempt == 0 {
                    panic!("singleton factory panicked on first construction");
                }
                Ok(Database("recovered"))
            }
        })
        .unwrap();
    let container = Arc::new(container);

    // First resolution panics; the panic must propagate out of resolution.
    let first = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = container.resolve::<Database>();
    }));
    assert!(first.is_err(), "first resolution should panic");
    assert_eq!(attempts.load(Ordering::SeqCst), 1);

    // After the panic the provider must be re-resolvable instead of permanently
    // stuck in `Initializing`. Resolve on a background thread with a timeout so a
    // regression (permanent deadlock) fails the test quickly instead of hanging.
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let container = Arc::clone(&container);
        thread::spawn(move || {
            let _ = tx.send(container.resolve::<Database>());
        });
    }
    let recovered = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("provider resolution deadlocked after a panicking factory")
        .expect("provider should be re-resolvable after a panicking factory");
    assert_eq!(recovered.0, "recovered");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

fn assert_circular_provider_error(error: NidusError, expected_type_name: &'static str) {
    match error {
        NidusError::ProviderFactory { source, .. } => {
            assert_circular_provider_error(*source, expected_type_name);
        }
        NidusError::CircularProviderResolution { type_name } => {
            assert_eq!(type_name, expected_type_name);
        }
        error => panic!("expected circular provider resolution error, got {error:?}"),
    }
}
