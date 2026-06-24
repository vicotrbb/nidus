//! Application lifecycle hooks.

use async_trait::async_trait;

use crate::{NidusError, Result};

/// Application lifecycle hook.
#[async_trait]
pub trait LifecycleHook: Send + Sync + 'static {
    /// Runs during application startup.
    async fn on_startup(&self) -> Result<()> {
        Ok(())
    }

    /// Runs during application shutdown.
    async fn on_shutdown(&self) -> Result<()> {
        Ok(())
    }
}

/// Ordered lifecycle hook runner.
#[derive(Default)]
pub struct LifecycleRunner {
    hooks: Vec<Box<dyn LifecycleHook>>,
}

impl LifecycleRunner {
    /// Creates an empty lifecycle runner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a lifecycle hook.
    pub fn hook<H>(mut self, hook: H) -> Self
    where
        H: LifecycleHook,
    {
        self.hooks.push(Box::new(hook));
        self
    }

    /// Runs startup hooks in registration order.
    pub async fn startup(&self) -> Result<()> {
        let mut started: Vec<usize> = Vec::new();

        tracing::debug!(hook_count = self.hooks.len(), "lifecycle startup begin");
        for (index, hook) in self.hooks.iter().enumerate() {
            tracing::debug!(hook_index = index, "lifecycle startup hook begin");
            if let Err(source) = hook.on_startup().await {
                tracing::error!(
                    hook_index = index,
                    error = %source,
                    "lifecycle startup hook failed"
                );
                let mut rollback_errors = Vec::new();
                for started_index in started.into_iter().rev() {
                    tracing::debug!(
                        hook_index = started_index,
                        "lifecycle startup rollback hook begin"
                    );
                    if let Err(error) = self.hooks[started_index].on_shutdown().await {
                        tracing::error!(
                            hook_index = started_index,
                            error = %error,
                            "lifecycle startup rollback hook failed"
                        );
                        rollback_errors.push(error);
                    } else {
                        tracing::debug!(
                            hook_index = started_index,
                            "lifecycle startup rollback hook complete"
                        );
                    }
                }

                return Err(NidusError::LifecycleStartup {
                    source: Box::new(source),
                    rollback_errors,
                });
            }
            tracing::debug!(hook_index = index, "lifecycle startup hook complete");
            started.push(index);
        }
        tracing::debug!(hook_count = self.hooks.len(), "lifecycle startup complete");
        Ok(())
    }

    /// Runs shutdown hooks in reverse registration order.
    pub async fn shutdown(&self) -> Result<()> {
        tracing::debug!(hook_count = self.hooks.len(), "lifecycle shutdown begin");
        for (index, hook) in self.hooks.iter().enumerate().rev() {
            tracing::debug!(hook_index = index, "lifecycle shutdown hook begin");
            hook.on_shutdown().await?;
            tracing::debug!(hook_index = index, "lifecycle shutdown hook complete");
        }
        tracing::debug!(hook_count = self.hooks.len(), "lifecycle shutdown complete");
        Ok(())
    }

    pub(crate) fn empty() -> Self {
        Self::new()
    }
}
