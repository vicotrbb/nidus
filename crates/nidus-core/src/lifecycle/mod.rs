//! Application lifecycle hooks.

pub mod managed;

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
#[derive(Clone, Default)]
pub struct LifecycleRunner {
    pub(super) hooks: Vec<std::sync::Arc<dyn LifecycleHook>>,
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
        self.hooks.push(std::sync::Arc::new(hook));
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

    /// Attempts every hook in reverse order within a total cleanup budget.
    /// Each remaining hook receives an equal share of the remaining time. Errors
    /// and panics are collected; a timeout drops only that hook future.
    pub async fn shutdown_bounded(&self, budget: std::time::Duration) -> Vec<NidusError> {
        let deadline = tokio::time::Instant::now() + budget;
        let mut errors = Vec::new();
        for (index, hook) in self.hooks.iter().enumerate().rev() {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(
                remaining / (index as u32 + 1),
                catch_hook(async { hook.on_shutdown().await }),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => errors.push(error),
                Err(error) => errors.push(NidusError::ApplicationBuild {
                    message: format!("shutdown hook {index} exceeded deadline: {error}"),
                }),
            }
        }
        errors
    }

    pub(crate) fn push_shared(&mut self, hook: std::sync::Arc<dyn LifecycleHook>) {
        self.hooks.push(hook);
    }

    /// Appends another runner in registration order.
    pub fn append(mut self, other: Self) -> Self {
        self.hooks.extend(other.hooks);
        self
    }

    pub(crate) fn empty() -> Self {
        Self::new()
    }
}

/// Poll a hook inside an unwind boundary without changing its cancellation behavior.
pub(crate) async fn catch_hook<T>(
    future: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    let mut future = Box::pin(future);
    std::future::poll_fn(move |cx| {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| future.as_mut().poll(cx))) {
            Ok(result) => result,
            Err(payload) => std::task::Poll::Ready(Err(panic_error(payload))),
        }
    })
    .await
}

pub(crate) fn panic_error(payload: Box<dyn std::any::Any + Send>) -> NidusError {
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_else(|| "non-string panic payload".to_owned());
    NidusError::ApplicationBuild {
        message: format!("participant panicked: {message}"),
    }
}
