use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use nidus_core::{LifecycleHook, LifecycleRunner, Module, ModuleBuilder, Nidus};

#[derive(Clone)]
struct RecordingHook {
    name: &'static str,
    events: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl LifecycleHook for RecordingHook {
    async fn on_startup(&self) -> nidus_core::Result<()> {
        self.events
            .lock()
            .unwrap()
            .push(format!("{}:startup", self.name));
        Ok(())
    }

    async fn on_shutdown(&self) -> nidus_core::Result<()> {
        self.events
            .lock()
            .unwrap()
            .push(format!("{}:shutdown", self.name));
        Ok(())
    }
}

struct AppModule;

impl Module for AppModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("AppModule").build()
    }
}

#[tokio::test]
async fn lifecycle_runner_starts_in_order_and_shuts_down_in_reverse_order() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let runner = LifecycleRunner::new()
        .hook(RecordingHook {
            name: "database",
            events: Arc::clone(&events),
        })
        .hook(RecordingHook {
            name: "server",
            events: Arc::clone(&events),
        });

    runner.startup().await.unwrap();
    runner.shutdown().await.unwrap();

    assert_eq!(
        *events.lock().unwrap(),
        [
            "database:startup",
            "server:startup",
            "server:shutdown",
            "database:shutdown"
        ]
    );
}

#[tokio::test]
async fn bootstrap_with_lifecycle_runs_startup_hooks() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let runner = LifecycleRunner::new().hook(RecordingHook {
        name: "app",
        events: Arc::clone(&events),
    });

    let app = Nidus::bootstrap_with_lifecycle::<AppModule>(runner)
        .await
        .unwrap();

    assert!(app.modules().get("AppModule").is_some());
    assert_eq!(*events.lock().unwrap(), ["app:startup"]);
}
