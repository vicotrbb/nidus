//! An explicit lifecycle owner for applications and cooperative background tasks.
//!
//! The supervisor owns startup and cleanup independently of waiting callers.
//! Dropping all handles requests shutdown; async cleanup still requires a live
//! Tokio runtime. Always await `shutdown` before dropping the runtime. Blocking
//! code cannot be forcibly terminated and must not run on runtime worker threads.

use crate::{Application, Container, LifecycleRunner};
use async_trait::async_trait;
use std::{error::Error, future::Future, pin::Pin, sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, mpsc, oneshot, watch},
    task::{JoinHandle, JoinSet},
    time::{Instant, timeout_at},
};

/// Error source retained by a managed startup, task or cleanup failure.
pub type BoxError = Box<dyn Error + Send + Sync>;
/// Result returned by managed workers and serving adapters.
pub type WorkResult = std::result::Result<(), BoxError>;

/// Observable managed application phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    /// The supervisor has been created.
    Built,
    /// Composition and startup hooks are executing.
    Starting,
    /// The application accepts work and is ready.
    Running,
    /// Readiness is withdrawn and cooperative work is draining.
    Draining,
    /// Work has drained or been aborted; resources are closing.
    Stopping,
    /// Cleanup has been attempted and the final report is available.
    Stopped,
}

/// One failure with its original source and lifecycle context.
#[derive(Debug, thiserror::Error)]
#[error("{stage}: {source}")]
pub struct Failure {
    /// Participant or phase that failed.
    pub stage: String,
    /// Original error, retained for downcasting and error chains.
    #[source]
    pub source: BoxError,
}

/// Final outcome shared by every shutdown waiter.
#[derive(Debug, Default)]
pub struct Report {
    /// Startup, worker and cleanup failures, in observed order.
    pub failures: Vec<Failure>,
    /// Whether the supervisor's drain or cleanup deadline expired.
    /// Composition rollback runs before the target is transferred to this owner;
    /// its timeout errors are retained in the composition failure's rollback errors.
    pub deadline_expired: bool,
}
impl Report {
    /// Whether all phases completed successfully within their deadlines.
    pub fn is_success(&self) -> bool {
        self.failures.is_empty() && !self.deadline_expired
    }
}

/// Shutdown policy. Total cooperative shutdown budget is drain plus cleanup.
#[derive(Clone, Copy, Debug)]
pub struct ShutdownPolicy {
    /// Maximum time for HTTP and worker draining before aborting tracked tasks.
    pub drain_timeout: Duration,
    /// Total resource cleanup budget, divided among remaining hooks so every hook is attempted.
    pub cleanup_timeout: Duration,
}
impl Default for ShutdownPolicy {
    fn default() -> Self {
        Self {
            drain_timeout: Duration::from_secs(30),
            cleanup_timeout: Duration::from_secs(10),
        }
    }
}

/// Cloneable cooperative drain signal and readiness observer.
#[derive(Clone)]
pub struct DrainSignal {
    state: watch::Receiver<State>,
}
impl DrainSignal {
    /// Current phase; only `Running` indicates readiness.
    pub fn state(&self) -> State {
        *self.state.borrow()
    }
    /// Whether the managed application is ready to accept work.
    pub fn is_ready(&self) -> bool {
        self.state() == State::Running
    }
    /// Waits until readiness has been withdrawn for shutdown.
    pub async fn cancelled(&mut self) {
        while !matches!(
            self.state(),
            State::Draining | State::Stopping | State::Stopped
        ) {
            if self.state.changed().await.is_err() {
                break;
            }
        }
    }
}

/// Task owner handed to serving adapters during startup.
/// Dropping this owner aborts tracked tasks; managed shutdown aborts and joins them.
pub struct ManagedTasks {
    pending: mpsc::UnboundedReceiver<(String, WorkFuture)>,
    spawner: ManagedSpawner,
    tasks: JoinSet<(String, WorkResult)>,
    signal: DrainSignal,
}
type WorkFuture = Pin<Box<dyn Future<Output = WorkResult> + Send>>;

/// Registers child tasks with the same application supervisor and deadline.
#[derive(Clone)]
pub struct ManagedSpawner {
    sender: mpsc::UnboundedSender<(String, WorkFuture)>,
}
impl ManagedSpawner {
    /// Transfers a child future to the owner. Rejected futures are dropped immediately.
    pub fn spawn(
        &self,
        name: impl Into<String>,
        work: impl Future<Output = WorkResult> + Send + 'static,
    ) -> bool {
        self.sender.send((name.into(), Box::pin(work))).is_ok()
    }
}

impl ManagedTasks {
    /// A sender for registering connection tasks without detaching their ownership.
    pub fn spawner(&self) -> ManagedSpawner {
        self.spawner.clone()
    }

    /// Returns a cooperative signal for the task being registered.
    pub fn signal(&self) -> DrainSignal {
        self.signal.clone()
    }
    /// Registers a future whose completion, cancellation and joining are supervised.
    pub fn spawn(
        &mut self,
        name: impl Into<String>,
        work: impl Future<Output = WorkResult> + Send + 'static,
    ) {
        let name = name.into();
        self.tasks.spawn(async move { (name, work.await) });
    }
}

/// A composed application that can install managed serving tasks.
#[async_trait]
pub trait ManagedTarget: Send + Sync + 'static {
    /// Returns the application that owns providers and lifecycle hooks.
    fn application(&self) -> &Application;
    /// Installs serving tasks after lifecycle startup and before readiness.
    async fn start(&self, _tasks: &mut ManagedTasks) -> WorkResult {
        Ok(())
    }
}
#[async_trait]
impl ManagedTarget for Application {
    fn application(&self) -> &Application {
        self
    }
}

type Worker = Box<
    dyn FnOnce(Arc<Container>, DrainSignal) -> Pin<Box<dyn Future<Output = WorkResult> + Send>>
        + Send,
>;

/// Managed startup options, including worker-only application support.
#[derive(Default)]
pub struct ManagedOptions {
    /// Shutdown deadlines applied to draining and cleanup.
    pub shutdown: ShutdownPolicy,
    workers: Vec<(String, Worker)>,
}
impl ManagedOptions {
    /// Registers a background worker. An error triggers application shutdown.
    /// Successful finite workers may finish while the application remains running.
    pub fn worker<F, Fut>(mut self, name: impl Into<String>, worker: F) -> Self
    where
        F: FnOnce(Arc<Container>, DrainSignal) -> Fut + Send + 'static,
        Fut: Future<Output = WorkResult> + Send + 'static,
    {
        self.workers.push((
            name.into(),
            Box::new(move |container, signal| Box::pin(worker(container, signal))),
        ));
        self
    }
}

struct Shared {
    spawner: ManagedSpawner,
    request: watch::Sender<bool>,
    signal: DrainSignal,
    report: watch::Receiver<Option<Arc<Report>>>,
    // The async mutex serializes joining. Cancellation leaves the JoinHandle in
    // place for the next waiter; holding this guard across await is intentional.
    supervisor: Mutex<Option<JoinHandle<()>>>,
}

/// Cloneable lifecycle owner. Repeated/concurrent shutdown returns the same report.
pub struct Managed<T> {
    target: Arc<T>,
    shared: Arc<Shared>,
}
impl<T> Clone for Managed<T> {
    fn clone(&self) -> Self {
        Self {
            target: self.target.clone(),
            shared: self.shared.clone(),
        }
    }
}
impl<T: ManagedTarget> Managed<T> {
    /// Starts composition in an owned supervisor task.
    /// Cancelling this caller does not cancel startup midway: the supervisor finishes
    /// startup, then rolls back when it cannot deliver the handle. A hanging initializer
    /// must supply its own timeout; no runtime can recover arbitrary blocking code.
    pub async fn start(
        build: impl Future<Output = crate::Result<T>> + Send + 'static,
        options: ManagedOptions,
    ) -> std::result::Result<Self, Arc<Report>> {
        let (request, request_rx) = watch::channel(false);
        let (state, state_rx) = watch::channel(State::Built);
        let signal = DrainSignal { state: state_rx };
        let (report_tx, report) = watch::channel(None);
        let (ready_tx, ready_rx) = oneshot::channel();
        let (sender, pending) = mpsc::unbounded_channel();
        let spawner = ManagedSpawner { sender };
        let tasks = ManagedTasks {
            tasks: JoinSet::new(),
            signal: signal.clone(),
            pending,
            spawner: spawner.clone(),
        };
        let supervisor = tokio::spawn(async move {
            supervise(
                build, options, request_rx, state, tasks, report_tx, ready_tx,
            )
            .await;
        });
        let shared = Arc::new(Shared {
            spawner,
            request,
            signal,
            report,
            supervisor: Mutex::new(Some(supervisor)),
        });
        match ready_rx.await {
            Ok(Ok(target)) => Ok(Self { target, shared }),
            _ => {
                // All error paths publish a report before closing startup delivery.
                let report = wait_report(&shared).await;
                join_supervisor(&shared).await;
                Err(report)
            }
        }
    }
    /// Registers child work with this application's drain and join ownership.
    pub fn spawner(&self) -> ManagedSpawner {
        self.shared.spawner.clone()
    }

    /// Returns the composed application or serving adapter.
    pub fn target(&self) -> &T {
        &self.target
    }
    /// Observes readiness and shutdown transitions.
    pub fn signal(&self) -> DrainSignal {
        self.shared.signal.clone()
    }
    /// Requests shutdown synchronously; cleanup belongs to the supervisor.
    pub fn request_shutdown(&self) {
        self.shared.request.send_replace(true);
    }
    /// Requests shutdown, waits for cleanup and joins the supervisor.
    /// Cancelling this waiter never cancels cleanup.
    pub async fn shutdown(&self) -> Arc<Report> {
        self.request_shutdown();
        let report = wait_report(&self.shared).await;
        join_supervisor(&self.shared).await;
        report
    }
}

async fn wait_report(shared: &Shared) -> Arc<Report> {
    let mut report = shared.report.clone();
    loop {
        if let Some(report) = report.borrow().clone() {
            return report;
        }
        if report.changed().await.is_err() {
            return Arc::new(Report {
                failures: vec![Failure {
                    stage: "supervisor".into(),
                    source: "supervisor exited without a report".into(),
                }],
                deadline_expired: false,
            });
        }
    }
}
async fn join_supervisor(shared: &Shared) {
    let mut task = shared.supervisor.lock().await;
    if let Some(task) = task.as_mut() {
        let _ = task.await;
    }
    task.take();
}

async fn supervise<T: ManagedTarget>(
    build: impl Future<Output = crate::Result<T>>,
    options: ManagedOptions,
    mut request: watch::Receiver<bool>,
    state: watch::Sender<State>,
    mut tasks: ManagedTasks,
    report_tx: watch::Sender<Option<Arc<Report>>>,
    ready_tx: oneshot::Sender<std::result::Result<Arc<T>, ()>>,
) {
    state.send_replace(State::Starting);
    let mut report = Report::default();
    let target = match crate::lifecycle::catch_hook(build).await {
        Ok(target) => Arc::new(target),
        Err(error) => {
            report.failures.push(Failure {
                stage: "composition".into(),
                source: Box::new(error),
            });
            state.send_replace(State::Stopped);
            report_tx.send_replace(Some(Arc::new(report)));
            let _ = ready_tx.send(Err(()));
            return;
        }
    };
    let lifecycle = target.application().lifecycle();
    let mut started = 0;
    for (index, hook) in lifecycle.hooks.iter().enumerate() {
        if let Err(error) = crate::lifecycle::catch_hook(async { hook.on_startup().await }).await {
            report.failures.push(Failure {
                stage: format!("startup hook {index}"),
                source: Box::new(error),
            });
            break;
        }
        started += 1;
    }
    if report.is_success()
        && let Err(error) = catch_work(async { target.start(&mut tasks).await }).await
    {
        report.failures.push(Failure {
            stage: "serving startup".into(),
            source: error,
        });
    }
    let mut ready_tx = Some(ready_tx);
    if report.is_success() {
        for (name, worker) in options.workers {
            let container = target.application().shared_container();
            let signal = tasks.signal();
            tasks.spawn(name, async move { worker(container, signal).await });
        }
        state.send_replace(State::Running);
        if ready_tx
            .take()
            .expect("startup sender")
            .send(Ok(target.clone()))
            .is_ok()
        {
            loop {
                if *request.borrow() {
                    break;
                }
                tokio::select! {
                    _ = request.changed() => break,
                    Some((name, work)) = tasks.pending.recv() => tasks.spawn(name, work),
                    result = tasks.tasks.join_next(), if !tasks.tasks.is_empty() => {
                        if let Some(result) = result { record_task(result, &mut report); }
                        if !report.is_success() { break; }
                    }
                }
            }
        }
    }
    state.send_replace(State::Draining);
    let drain_deadline = Instant::now() + options.shutdown.drain_timeout;
    // Close task admission first. Every queued child is now known and joined.
    tasks.pending.close();
    while let Some((name, work)) = tasks.pending.recv().await {
        tasks.spawn(name, work);
    }
    while !tasks.tasks.is_empty() {
        match timeout_at(drain_deadline, tasks.tasks.join_next()).await {
            Ok(Some(result)) => record_task(result, &mut report),
            Ok(None) => break,
            Err(_) => {
                report.deadline_expired = true;
                tasks.tasks.abort_all();
                while let Some(result) = tasks.tasks.join_next().await {
                    if !matches!(&result, Err(error) if error.is_cancelled()) {
                        record_task(result, &mut report);
                    }
                }
                break;
            }
        }
    }
    state.send_replace(State::Stopping);
    cleanup(
        &lifecycle,
        started,
        options.shutdown.cleanup_timeout,
        &mut report,
    )
    .await;
    state.send_replace(State::Stopped);
    report_tx.send_replace(Some(Arc::new(report)));
    if let Some(ready_tx) = ready_tx {
        let _ = ready_tx.send(Err(()));
    }
}

async fn cleanup(
    lifecycle: &LifecycleRunner,
    started: usize,
    budget: Duration,
    report: &mut Report,
) {
    let deadline = Instant::now() + budget;
    for index in (0..started).rev() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let hook_deadline = Instant::now() + remaining / (index as u32 + 1);
        match timeout_at(
            hook_deadline,
            crate::lifecycle::catch_hook(async { lifecycle.hooks[index].on_shutdown().await }),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => report.failures.push(Failure {
                stage: format!("shutdown hook {index}"),
                source: Box::new(error),
            }),
            Err(error) => {
                report.deadline_expired = true;
                report.failures.push(Failure {
                    stage: format!("shutdown hook {index}"),
                    source: Box::new(error),
                });
            }
        }
    }
}
fn record_task(
    result: std::result::Result<(String, WorkResult), tokio::task::JoinError>,
    report: &mut Report,
) {
    match result {
        Ok((stage, Err(source))) => report.failures.push(Failure { stage, source }),
        Err(error) => report.failures.push(Failure {
            stage: "managed task".into(),
            source: Box::new(error),
        }),
        Ok((_, Ok(()))) => {}
    }
}

async fn catch_work(future: impl Future<Output = WorkResult>) -> WorkResult {
    let mut future = Box::pin(future);
    std::future::poll_fn(move |cx| {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| future.as_mut().poll(cx))) {
            Ok(result) => result,
            Err(payload) => {
                std::task::Poll::Ready(Err(crate::lifecycle::panic_error(payload).into()))
            }
        }
    })
    .await
}
