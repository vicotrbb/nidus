use async_trait::async_trait;
use nidus_core::lifecycle::managed::{Managed, ManagedOptions, ShutdownPolicy, State};
use nidus_core::{
    Application, Container, LifecycleHook, LifecycleRunner, ModuleBuilder, ModuleGraph, NidusError,
};
use std::{
    future::pending,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Notify;

#[derive(Clone)]
struct Hook {
    name: &'static str,
    log: Arc<Mutex<Vec<String>>>,
    fail_start: bool,
    fail_stop: bool,
    hang_stop: bool,
}
impl Hook {
    fn new(name: &'static str, log: &Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            name,
            log: log.clone(),
            fail_start: false,
            fail_stop: false,
            hang_stop: false,
        }
    }
}
#[async_trait]
impl LifecycleHook for Hook {
    async fn on_startup(&self) -> nidus_core::Result<()> {
        self.log
            .lock()
            .unwrap()
            .push(format!("start:{}", self.name));
        if self.fail_start {
            return Err(NidusError::ApplicationBuild {
                message: self.name.into(),
            });
        }
        Ok(())
    }
    async fn on_shutdown(&self) -> nidus_core::Result<()> {
        self.log.lock().unwrap().push(format!("stop:{}", self.name));
        if self.hang_stop {
            pending::<()>().await;
        }
        if self.fail_stop {
            return Err(NidusError::ApplicationBuild {
                message: self.name.into(),
            });
        }
        Ok(())
    }
}
fn application(lifecycle: LifecycleRunner) -> Application {
    Application::with_lifecycle(
        Container::new(),
        ModuleGraph::from_modules([ModuleBuilder::new("Root").build()]).unwrap(),
        lifecycle,
    )
}

#[tokio::test]
async fn concurrent_shutdown_cancels_and_joins_worker_before_reverse_cleanup() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let worker_ready = Arc::new(Notify::new());
    let options = ManagedOptions::default().worker("worker", {
        let log = log.clone();
        let ready = worker_ready.clone();
        move |_, mut signal| async move {
            ready.notify_one();
            signal.cancelled().await;
            assert_eq!(signal.state(), State::Draining);
            log.lock().unwrap().push("worker:drained".into());
            Ok(())
        }
    });
    let app = application(
        LifecycleRunner::new()
            .hook(Hook::new("telemetry", &log))
            .hook(Hook::new("database", &log)),
    );
    let managed = Managed::start(async { Ok(app) }, options).await.unwrap();
    worker_ready.notified().await;
    assert!(managed.signal().is_ready());
    let (first, second) = tokio::join!(managed.shutdown(), managed.shutdown());
    assert!(Arc::ptr_eq(&first, &second));
    assert!(first.is_success());
    assert_eq!(managed.signal().state(), State::Stopped);
    assert_eq!(
        *log.lock().unwrap(),
        [
            "start:telemetry",
            "start:database",
            "worker:drained",
            "stop:database",
            "stop:telemetry"
        ]
    );
    assert!(Arc::ptr_eq(&first, &managed.shutdown().await));
}

#[tokio::test]
async fn failed_start_rolls_back_successes_and_preserves_cleanup_errors() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut first = Hook::new("first", &log);
    first.fail_stop = true;
    let mut second = Hook::new("second", &log);
    second.fail_start = true;
    let app = application(
        LifecycleRunner::new()
            .hook(first)
            .hook(second)
            .hook(Hook::new("never", &log)),
    );
    let report = match Managed::start(async { Ok(app) }, ManagedOptions::default()).await {
        Err(report) => report,
        Ok(_) => panic!("startup should fail"),
    };
    assert_eq!(report.failures.len(), 2);
    assert_eq!(
        *log.lock().unwrap(),
        ["start:first", "start:second", "stop:first"]
    );
    assert!(
        report.failures[0]
            .source
            .downcast_ref::<NidusError>()
            .is_some()
    );
}

#[tokio::test(start_paused = true)]
async fn hanging_hook_does_not_prevent_later_cleanup_or_telemetry_flush() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut hang = Hook::new("hang", &log);
    hang.hang_stop = true;
    let app = application(
        LifecycleRunner::new()
            .hook(Hook::new("telemetry", &log))
            .hook(hang),
    );
    let mut options = ManagedOptions::default();
    options.shutdown = ShutdownPolicy {
        drain_timeout: Duration::from_secs(1),
        cleanup_timeout: Duration::from_secs(2),
    };
    let managed = Managed::start(async { Ok(app) }, options).await.unwrap();
    let report = managed.shutdown().await;
    assert!(report.deadline_expired);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(log.lock().unwrap().last().unwrap(), "stop:telemetry");
}

#[derive(Clone)]
struct ControlledHook {
    entered: Arc<Notify>,
    release: Arc<Notify>,
    stopped: Arc<Notify>,
    block_start: bool,
}
#[async_trait]
impl LifecycleHook for ControlledHook {
    async fn on_startup(&self) -> nidus_core::Result<()> {
        if self.block_start {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(())
    }
    async fn on_shutdown(&self) -> nidus_core::Result<()> {
        if !self.block_start {
            self.entered.notify_one();
            self.release.notified().await;
        }
        self.stopped.notify_one();
        Ok(())
    }
}

#[tokio::test]
async fn cancelled_startup_caller_still_rolls_back_owned_application() {
    let hook = ControlledHook {
        entered: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
        stopped: Arc::new(Notify::new()),
        block_start: true,
    };
    let app = application(LifecycleRunner::new().hook(hook.clone()));
    let caller = tokio::spawn(Managed::start(async { Ok(app) }, ManagedOptions::default()));
    hook.entered.notified().await;
    caller.abort();
    assert!(matches!(caller.await, Err(error) if error.is_cancelled()));
    hook.release.notify_one();
    hook.stopped.notified().await;
}

#[tokio::test]
async fn cancelled_shutdown_waiter_leaves_cleanup_running() {
    let hook = ControlledHook {
        entered: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
        stopped: Arc::new(Notify::new()),
        block_start: false,
    };
    let app = application(LifecycleRunner::new().hook(hook.clone()));
    let managed = Managed::start(async { Ok(app) }, ManagedOptions::default())
        .await
        .unwrap();
    let caller = tokio::spawn({
        let managed = managed.clone();
        async move { managed.shutdown().await }
    });
    hook.entered.notified().await;
    caller.abort();
    assert!(matches!(caller.await, Err(error) if error.is_cancelled()));
    assert_eq!(managed.signal().state(), State::Stopping);
    hook.release.notify_one();
    assert!(managed.shutdown().await.is_success());
}

#[tokio::test(start_paused = true)]
async fn noncooperative_async_worker_is_aborted_joined_and_dropped_before_cleanup() {
    struct Dropped(Arc<Mutex<Vec<String>>>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.lock().unwrap().push("worker:dropped".into());
        }
    }
    let log = Arc::new(Mutex::new(Vec::new()));
    let ready = Arc::new(Notify::new());
    let options = ManagedOptions::default().worker("hang", {
        let guard = Dropped(log.clone());
        let ready = ready.clone();
        move |_, _| async move {
            let _guard = guard;
            ready.notify_one();
            pending().await
        }
    });
    let app = application(LifecycleRunner::new().hook(Hook::new("resource", &log)));
    let managed = Managed::start(async { Ok(app) }, options).await.unwrap();
    ready.notified().await;
    assert!(managed.shutdown().await.deadline_expired);
    assert_eq!(
        *log.lock().unwrap(),
        ["start:resource", "worker:dropped", "stop:resource"]
    );
}

#[tokio::test]
async fn panicking_worker_factory_is_joined_and_resources_close() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let entered = Arc::new(Notify::new());
    let options = ManagedOptions::default().worker("factory", {
        let entered = entered.clone();
        move |_, _| {
            entered.notify_one();
            panic!("worker factory panic");
            #[allow(unreachable_code)]
            async {
                Ok(())
            }
        }
    });
    let app = application(LifecycleRunner::new().hook(Hook::new("resource", &log)));
    let managed = Managed::start(async { Ok(app) }, options).await.unwrap();
    entered.notified().await;
    let report = managed.shutdown().await;
    assert!(!report.is_success());
    assert_eq!(log.lock().unwrap().last().unwrap(), "stop:resource");
}

#[tokio::test]
async fn panicking_cleanup_does_not_skip_remaining_hooks() {
    struct Panic;
    #[async_trait]
    impl LifecycleHook for Panic {
        async fn on_shutdown(&self) -> nidus_core::Result<()> {
            panic!("cleanup panic")
        }
    }
    let log = Arc::new(Mutex::new(Vec::new()));
    let app = application(
        LifecycleRunner::new()
            .hook(Hook::new("telemetry", &log))
            .hook(Panic),
    );
    let managed = Managed::start(async { Ok(app) }, ManagedOptions::default())
        .await
        .unwrap();
    let report = managed.shutdown().await;
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].to_string().contains("cleanup panic"));
    assert_eq!(log.lock().unwrap().last().unwrap(), "stop:telemetry");
}
