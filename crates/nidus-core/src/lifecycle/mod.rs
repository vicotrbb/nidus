//! Application lifecycle hooks.

use crate::{NidusError, Result};
use async_trait::async_trait;

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
    #[tracing::instrument(
        name = "lifecycle.startup",
        skip_all,
        fields(hook_count = self.hooks.len())
    )]
    pub async fn startup(&self) -> Result<()> {
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
                // Startup is sequential, so every earlier index completed successfully.
                for started_index in (0..index).rev() {
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
        }
        tracing::debug!(hook_count = self.hooks.len(), "lifecycle startup complete");
        Ok(())
    }

    /// Runs shutdown hooks in reverse registration order.
    ///
    /// Every hook is attempted even when an earlier hook fails. The first
    /// failure in shutdown order is returned after the remaining hooks run.
    #[tracing::instrument(
        name = "lifecycle.shutdown",
        skip_all,
        fields(hook_count = self.hooks.len())
    )]
    pub async fn shutdown(&self) -> Result<()> {
        tracing::debug!(hook_count = self.hooks.len(), "lifecycle shutdown begin");
        let mut first_error = None;
        let mut error_count = 0_usize;
        for (index, hook) in self.hooks.iter().enumerate().rev() {
            tracing::debug!(hook_index = index, "lifecycle shutdown hook begin");
            if let Err(error) = hook.on_shutdown().await {
                error_count += 1;
                tracing::error!(
                    hook_index = index,
                    error = %error,
                    "lifecycle shutdown hook failed"
                );
                if first_error.is_none() {
                    first_error = Some(error);
                }
            } else {
                tracing::debug!(hook_index = index, "lifecycle shutdown hook complete");
            }
        }
        if let Some(error) = first_error {
            tracing::error!(
                hook_count = self.hooks.len(),
                error_count,
                "lifecycle shutdown completed with errors"
            );
            return Err(error);
        }
        tracing::debug!(hook_count = self.hooks.len(), "lifecycle shutdown complete");
        Ok(())
    }

    pub(crate) fn empty() -> Self {
        Self::new()
    }
}
