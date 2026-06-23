//! Application lifecycle hooks.

use async_trait::async_trait;

use crate::Result;

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
        for hook in &self.hooks {
            hook.on_startup().await?;
        }
        Ok(())
    }

    /// Runs shutdown hooks in reverse registration order.
    pub async fn shutdown(&self) -> Result<()> {
        for hook in self.hooks.iter().rev() {
            hook.on_shutdown().await?;
        }
        Ok(())
    }

    pub(crate) fn empty() -> Self {
        Self::new()
    }
}
