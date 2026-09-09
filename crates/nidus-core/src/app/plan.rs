use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc};

use crate::{
    Application, Container, LifecycleHook, LifecycleRunner, ModuleGraph, NidusError, Result,
};
use async_trait::async_trait;

/// An asynchronously initialized, application-owned provider.
///
/// Initialization must clean up its own partial failure. Once it returns successfully,
/// the application owns shutdown. Implementations must be cancellation-safe and must
/// move blocking operations to a blocking executor. Overrides are externally owned.
#[async_trait]
pub trait Resource: Send + Sync + Sized + 'static {
    /// Creates the resource using already registered dependencies.
    async fn initialize(container: &Container) -> Result<Self>;
    /// Closes the resource; dependencies are still available during this call.
    async fn shutdown(&self) -> Result<()>;
}

pub(crate) type ResourceCleanup = fn(&Container) -> Result<Arc<dyn LifecycleHook>>;

pub(crate) fn initialize_resource<R: Resource>(
    container: &mut Container,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
    Box::pin(async move {
        if container.contains::<R>() {
            return Err(NidusError::DuplicateProvider {
                type_name: std::any::type_name::<R>(),
            });
        }
        let resource = R::initialize(container).await?;
        container.register_singleton(resource)
    })
}

struct ResourceHook<R>(Arc<R>);
#[async_trait]
impl<R: Resource> LifecycleHook for ResourceHook<R> {
    async fn on_shutdown(&self) -> Result<()> {
        self.0.shutdown().await
    }
}

pub(crate) fn resource_cleanup<R: Resource>(
    container: &Container,
) -> Result<Arc<dyn LifecycleHook>> {
    Ok(Arc::new(ResourceHook(container.resolve::<R>()?)))
}

/// Shared graph/provider composition used by HTTP production and module tests.
///
/// Construct and validate HTTP metadata before calling `initialize`. Explicit
/// override names refer to concrete Rust types, not module display names.
/// Opaque initializers cannot be inferred or selectively replaced.
pub struct ApplicationPlan {
    graph: ModuleGraph,
    container: Container,
    overrides: BTreeSet<&'static str>,
    resources: LifecycleRunner,
    initialized: bool,
}

impl ApplicationPlan {
    /// Registers module factories around explicit pre-initialization overrides.
    pub fn new(
        graph: ModuleGraph,
        mut container: Container,
        overrides: BTreeSet<&'static str>,
    ) -> Result<Self> {
        let mut declared = BTreeSet::new();
        for (_, name, _) in graph.initializers() {
            if let Some(name) = name
                && (!declared.insert(name)
                    || (!overrides.contains(name)
                        && container
                            .provider_type_names()
                            .any(|registered| registered == name)))
            {
                return Err(NidusError::DuplicateProvider { type_name: name });
            }
        }
        graph.register_providers_except(&mut container, &overrides)?;
        for name in declared {
            if !overrides.contains(name)
                && container
                    .provider_type_names()
                    .any(|registered| registered == name)
            {
                return Err(NidusError::DuplicateProvider { type_name: name });
            }
        }
        Ok(Self {
            graph,
            container,
            overrides,
            resources: LifecycleRunner::new(),
            initialized: false,
        })
    }

    /// Registers synchronous provider callbacks on Tokio's blocking executor.
    /// Async composition uses this entry point so an eager user registrar cannot
    /// block an async runtime worker. The blocking callback must eventually return.
    pub async fn prepare(
        graph: ModuleGraph,
        container: Container,
        overrides: BTreeSet<&'static str>,
    ) -> Result<Self> {
        tokio::task::spawn_blocking(move || Self::new(graph, container, overrides))
            .await
            .map_err(|error| NidusError::ApplicationBuild {
                message: format!("provider registration task failed: {error}"),
            })?
    }

    /// Returns the validated graph.
    pub fn graph(&self) -> &ModuleGraph {
        &self.graph
    }
    /// Returns the composition container.
    pub fn container(&self) -> &Container {
        &self.container
    }

    /// Initializes providers in deterministic dependency order.
    /// On failure all successfully initialized resources are rolled back.
    /// Legacy opaque initializers retain responsibility for their own resources.
    pub async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Err(NidusError::ApplicationBuild {
                message: "application plan initialization already attempted".to_owned(),
            });
        }
        self.initialized = true;
        for (initialize, name, cleanup) in self.graph.initializers() {
            if name.is_some_and(|name| self.overrides.contains(name)) {
                continue;
            }
            if let Err(source) =
                crate::lifecycle::catch_hook(async { initialize(&mut self.container).await }).await
            {
                let rollback_errors = self
                    .resources
                    .shutdown_bounded(std::time::Duration::from_secs(10))
                    .await;
                self.resources = LifecycleRunner::new();
                return Err(if rollback_errors.is_empty() {
                    source
                } else {
                    NidusError::LifecycleStartup {
                        source: Box::new(source),
                        rollback_errors,
                    }
                });
            }
            if let Some(cleanup) = cleanup {
                self.resources.push_shared(cleanup(&self.container)?);
            }
        }
        Ok(())
    }

    /// Rolls back owned resources after a subsequent composition failure.
    pub async fn rollback(&mut self, source: NidusError) -> NidusError {
        let rollback_errors = self
            .resources
            .shutdown_bounded(std::time::Duration::from_secs(10))
            .await;
        self.resources = LifecycleRunner::new();
        if rollback_errors.is_empty() {
            source
        } else {
            NidusError::LifecycleStartup {
                source: Box::new(source),
                rollback_errors,
            }
        }
    }

    /// Transfers the container and resource cleanup to an application.
    /// Additional hooks shut down before resources, in reverse registration order.
    pub fn finish(self, lifecycle: LifecycleRunner) -> Application {
        Application::with_lifecycle(self.container, self.graph, self.resources.append(lifecycle))
    }
}
