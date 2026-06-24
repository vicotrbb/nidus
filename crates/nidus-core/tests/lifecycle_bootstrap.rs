use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use nidus_core::{LifecycleHook, LifecycleRunner, Module, ModuleBuilder, Nidus, NidusError};

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
struct ModularAppModule;
struct UsersModule;

impl Module for AppModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("AppModule").build()
    }
}

impl Module for ModularAppModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("ModularAppModule")
            .import("UsersModule")
            .build()
    }
}

impl Module for UsersModule {
    fn definition() -> nidus_core::ModuleDefinition {
        ModuleBuilder::new("UsersModule")
            .provider("UsersService")
            .export("UsersService")
            .build()
    }
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

#[tokio::test]
async fn bootstrap_with_modules_and_lifecycle_validates_graph_and_runs_startup_hooks() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let runner = LifecycleRunner::new().hook(RecordingHook {
        name: "modular-app",
        events: Arc::clone(&events),
    });

    let app = Nidus::bootstrap_with_modules_and_lifecycle::<ModularAppModule, _>(
        [UsersModule::definition()],
        runner,
    )
    .await
    .unwrap();

    assert!(app.modules().get("ModularAppModule").is_some());
    assert!(app.modules().get("UsersModule").is_some());
    assert_eq!(*events.lock().unwrap(), ["modular-app:startup"]);
}

#[tokio::test]
async fn bootstrap_with_modules_and_lifecycle_validates_before_startup_hooks() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let runner = LifecycleRunner::new().hook(RecordingHook {
        name: "modular-app",
        events: Arc::clone(&events),
    });

    let error = match Nidus::bootstrap_with_modules_and_lifecycle::<ModularAppModule, _>([], runner)
        .await
    {
        Ok(_) => panic!("missing module import should fail"),
        Err(error) => error,
    };

    assert!(matches!(error, NidusError::MissingModuleImport { .. }));
    assert!(events.lock().unwrap().is_empty());
}
