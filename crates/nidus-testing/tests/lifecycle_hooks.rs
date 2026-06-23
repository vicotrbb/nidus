use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use nidus_core::LifecycleHook;
use nidus_testing::TestApp;

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

#[tokio::test]
async fn test_app_builder_runs_lifecycle_hooks() {
    let events = Arc::new(Mutex::new(Vec::new()));

    let app = TestApp::builder(Router::new())
        .lifecycle_hook(RecordingHook {
            name: "database",
            events: Arc::clone(&events),
        })
        .lifecycle_hook(RecordingHook {
            name: "server",
            events: Arc::clone(&events),
        })
        .build_started()
        .await
        .unwrap();

    app.shutdown().await.unwrap();

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
