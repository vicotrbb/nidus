use async_trait::async_trait;
use nidus_core::{
    ApplicationPlan, Container, LifecycleRunner, ModuleBuilder, ModuleGraph, NidusError, Resource,
    Result,
};
use std::{
    collections::BTreeSet,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Events(Mutex<Vec<&'static str>>);
struct First(Arc<Events>);
struct Second(Arc<Events>);
#[async_trait]
impl Resource for First {
    async fn initialize(container: &Container) -> Result<Self> {
        let events = container.resolve::<Events>()?;
        events.0.lock().unwrap().push("first:start");
        Ok(Self(events))
    }
    async fn shutdown(&self) -> Result<()> {
        self.0.0.lock().unwrap().push("first:stop");
        Err(NidusError::ApplicationBuild {
            message: "first close error".into(),
        })
    }
}
#[async_trait]
impl Resource for Second {
    async fn initialize(container: &Container) -> Result<Self> {
        let first = container.resolve::<First>()?;
        first.0.0.lock().unwrap().push("second:start");
        Ok(Self(first.0.clone()))
    }
    async fn shutdown(&self) -> Result<()> {
        self.0.0.lock().unwrap().push("second:stop");
        Err(NidusError::ApplicationBuild {
            message: "second close error".into(),
        })
    }
}
fn fail(_: &mut Container) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
    Box::pin(async {
        Err(NidusError::MissingProvider {
            type_name: "original failure",
        })
    })
}
#[tokio::test]
async fn rollback_retains_original_error_and_every_cleanup_failure_in_reverse_order() {
    let graph = ModuleGraph::from_modules([
        ModuleBuilder::new("Dependency").resource::<First>().build(),
        ModuleBuilder::new("Root")
            .import("Dependency")
            .resource::<Second>()
            .async_initializer(fail)
            .build(),
    ])
    .unwrap();
    let mut container = Container::new();
    container.register_singleton(Events::default()).unwrap();
    let events = container.resolve::<Events>().unwrap();
    let mut plan = ApplicationPlan::prepare(graph, container, BTreeSet::new())
        .await
        .unwrap();
    let error = plan.initialize().await.unwrap_err();
    let NidusError::LifecycleStartup {
        source,
        rollback_errors,
    } = error
    else {
        panic!("expected rollback errors")
    };
    assert!(matches!(*source, NidusError::MissingProvider { .. }));
    assert_eq!(rollback_errors.len(), 2);
    assert!(
        rollback_errors[0]
            .to_string()
            .contains("second close error")
    );
    assert!(rollback_errors[1].to_string().contains("first close error"));
    assert_eq!(
        *events.0.lock().unwrap(),
        ["first:start", "second:start", "second:stop", "first:stop"]
    );
    plan.finish(LifecycleRunner::new())
        .shutdown()
        .await
        .unwrap();
}

fn opaque(container: &mut Container) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
    Box::pin(async { container.register_singleton(First(container.resolve::<Events>()?)) })
}
#[tokio::test]
async fn opaque_registration_is_detected_before_constructing_a_duplicate_resource() {
    let graph = ModuleGraph::from_modules([ModuleBuilder::new("Root")
        .async_initializer(opaque)
        .resource::<First>()
        .build()])
    .unwrap();
    let mut container = Container::new();
    container.register_singleton(Events::default()).unwrap();
    let events = container.resolve::<Events>().unwrap();
    let mut plan = ApplicationPlan::prepare(graph, container, BTreeSet::new())
        .await
        .unwrap();
    assert!(matches!(
        plan.initialize().await,
        Err(NidusError::DuplicateProvider { .. })
    ));
    assert!(events.0.lock().unwrap().is_empty());
}

struct Registrar;
impl nidus_core::ProviderRegistrant for Registrar {
    fn register_provider(container: &mut Container) -> Result<()> {
        container.register_singleton(First(Arc::new(Events::default())))
    }
}
#[tokio::test]
async fn registrar_resource_collision_fails_before_any_resource_initializer() {
    let graph = ModuleGraph::from_modules([ModuleBuilder::new("Root")
        .provider_typed::<Registrar>()
        .resource::<First>()
        .build()])
    .unwrap();
    assert!(matches!(
        ApplicationPlan::prepare(graph, Container::new(), BTreeSet::new()).await,
        Err(NidusError::DuplicateProvider { .. })
    ));
}

struct EventsRegistrar;
impl nidus_core::ProviderRegistrant for EventsRegistrar {
    fn register_provider(container: &mut Container) -> Result<()> {
        container.register_singleton(Events::default())
    }
}
struct BootstrapRoot;
impl nidus_core::Module for BootstrapRoot {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("BootstrapRoot")
            .provider_typed::<EventsRegistrar>()
            .resource::<First>()
            .build()
    }
}
#[tokio::test]
async fn core_async_bootstrap_retains_declared_resource_ownership() {
    let app = nidus_core::Nidus::bootstrap_with_lifecycle::<BootstrapRoot>(LifecycleRunner::new())
        .await
        .unwrap();
    let events = app.container().resolve::<Events>().unwrap();
    assert!(
        app.shutdown()
            .await
            .unwrap_err()
            .to_string()
            .contains("first close error")
    );
    assert_eq!(*events.0.lock().unwrap(), ["first:start", "first:stop"]);
}
