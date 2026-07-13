use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use nidus_core::{LifecycleHook, LifecycleRunner, NidusError};
use tracing::Level;
use tracing_subscriber::{Layer, fmt::MakeWriter, layer::SubscriberExt};

static TRACING_CAPTURE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Default)]
struct SharedLogWriter {
    output: Arc<Mutex<Vec<u8>>>,
}

impl SharedLogWriter {
    fn contents(&self) -> String {
        String::from_utf8(self.output.lock().unwrap().clone()).unwrap()
    }

    fn clear(&self) {
        self.output.lock().unwrap().clear();
    }
}

impl<'writer> MakeWriter<'writer> for SharedLogWriter {
    type Writer = SharedLogGuard;

    fn make_writer(&'writer self) -> Self::Writer {
        SharedLogGuard {
            output: Arc::clone(&self.output),
        }
    }
}

struct SharedLogGuard {
    output: Arc<Mutex<Vec<u8>>>,
}

impl std::io::Write for SharedLogGuard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.output.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
struct RecordingHook {
    name: &'static str,
    events: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone)]
struct FailingStartupHook {
    name: &'static str,
    events: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone)]
struct FailingShutdownHook {
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

#[async_trait]
impl LifecycleHook for FailingStartupHook {
    async fn on_startup(&self) -> nidus_core::Result<()> {
        self.events
            .lock()
            .unwrap()
            .push(format!("{}:startup", self.name));
        Err(NidusError::MissingProvider {
            type_name: self.name,
        })
    }
}

#[async_trait]
impl LifecycleHook for FailingShutdownHook {
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
        Err(NidusError::DuplicateProvider {
            type_name: self.name,
        })
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
async fn lifecycle_runner_attempts_remaining_shutdown_hooks_after_failure() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let runner = LifecycleRunner::new()
        .hook(RecordingHook {
            name: "database",
            events: Arc::clone(&events),
        })
        .hook(FailingShutdownHook {
            name: "cache",
            events: Arc::clone(&events),
        });

    let error = runner.shutdown().await.unwrap_err();

    assert!(matches!(
        error,
        NidusError::DuplicateProvider { type_name: "cache" }
    ));
    assert_eq!(
        *events.lock().unwrap(),
        ["cache:shutdown", "database:shutdown"]
    );
}

#[test]
fn lifecycle_runner_emits_startup_and_shutdown_debug_logs() {
    let _capture_guard = TRACING_CAPTURE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let writer = SharedLogWriter::default();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer.clone())
            .with_ansi(false)
            .with_target(false)
            .with_filter(tracing_subscriber::filter::LevelFilter::from_level(
                Level::DEBUG,
            )),
    );
    let events = Arc::new(Mutex::new(Vec::new()));
    let runner = LifecycleRunner::new().hook(RecordingHook {
        name: "database",
        events,
    });

    tracing::subscriber::with_default(subscriber, || {
        for _ in 0..16 {
            writer.clear();
            tracing_core::callsite::rebuild_interest_cache();
            runtime.block_on(async {
                runner.startup().await.unwrap();
                runner.shutdown().await.unwrap();
            });

            let logs = writer.contents();
            if logs.contains("lifecycle startup begin")
                && logs.contains("lifecycle startup hook begin")
                && logs.contains("lifecycle startup hook complete")
                && logs.contains("lifecycle shutdown hook begin")
                && logs.contains("lifecycle shutdown hook complete")
                && logs.contains("lifecycle shutdown complete")
            {
                return;
            }
            std::thread::yield_now();
        }
    });

    let logs = writer.contents();
    assert!(logs.contains("lifecycle startup begin"), "{logs}");
    assert!(logs.contains("lifecycle startup hook begin"), "{logs}");
    assert!(logs.contains("lifecycle startup hook complete"), "{logs}");
    assert!(logs.contains("lifecycle shutdown hook begin"), "{logs}");
    assert!(logs.contains("lifecycle shutdown hook complete"), "{logs}");
    assert!(logs.contains("lifecycle shutdown complete"), "{logs}");
}

#[tokio::test]
async fn lifecycle_runner_rolls_back_started_hooks_when_startup_fails() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let runner = LifecycleRunner::new()
        .hook(RecordingHook {
            name: "database",
            events: Arc::clone(&events),
        })
        .hook(FailingStartupHook {
            name: "server",
            events: Arc::clone(&events),
        });

    let error = runner.startup().await.unwrap_err();

    let NidusError::LifecycleStartup {
        source,
        rollback_errors,
    } = error
    else {
        panic!("expected lifecycle startup error");
    };
    assert!(matches!(*source, NidusError::MissingProvider { .. }));
    assert!(rollback_errors.is_empty());
    assert_eq!(
        *events.lock().unwrap(),
        ["database:startup", "server:startup", "database:shutdown"]
    );
}

#[test]
fn lifecycle_runner_emits_failure_and_rollback_logs() {
    let _capture_guard = TRACING_CAPTURE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let writer = SharedLogWriter::default();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer.clone())
            .with_ansi(false)
            .with_target(false)
            .with_filter(tracing_subscriber::filter::LevelFilter::from_level(
                Level::DEBUG,
            )),
    );
    let events = Arc::new(Mutex::new(Vec::new()));
    let runner = LifecycleRunner::new()
        .hook(RecordingHook {
            name: "database",
            events: Arc::clone(&events),
        })
        .hook(FailingStartupHook {
            name: "server",
            events,
        });

    tracing::subscriber::with_default(subscriber, || {
        for _ in 0..16 {
            writer.clear();
            tracing_core::callsite::rebuild_interest_cache();
            let error = runtime.block_on(async { runner.startup().await.unwrap_err() });
            assert!(matches!(error, NidusError::LifecycleStartup { .. }));

            let logs = writer.contents();
            if logs.contains("lifecycle startup hook failed")
                && logs.contains("lifecycle startup rollback hook begin")
                && logs.contains("lifecycle startup rollback hook complete")
            {
                return;
            }
            std::thread::yield_now();
        }
    });

    let logs = writer.contents();
    assert!(logs.contains("lifecycle startup hook failed"), "{logs}");
    assert!(
        logs.contains("lifecycle startup rollback hook begin"),
        "{logs}"
    );
    assert!(
        logs.contains("lifecycle startup rollback hook complete"),
        "{logs}"
    );
}

#[tokio::test]
async fn lifecycle_runner_reports_rollback_errors() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let runner = LifecycleRunner::new()
        .hook(FailingShutdownHook {
            name: "cache",
            events: Arc::clone(&events),
        })
        .hook(FailingStartupHook {
            name: "server",
            events: Arc::clone(&events),
        });

    let error = runner.startup().await.unwrap_err();

    let NidusError::LifecycleStartup {
        source,
        rollback_errors,
    } = error
    else {
        panic!("expected lifecycle startup error");
    };
    assert!(matches!(*source, NidusError::MissingProvider { .. }));
    assert_eq!(rollback_errors.len(), 1);
    assert!(matches!(
        rollback_errors[0],
        NidusError::DuplicateProvider { .. }
    ));
    assert_eq!(
        *events.lock().unwrap(),
        ["cache:startup", "server:startup", "cache:shutdown"]
    );
}
