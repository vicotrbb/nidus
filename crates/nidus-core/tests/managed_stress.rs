use async_trait::async_trait;
use nidus_core::lifecycle::managed::{
    Managed, ManagedOptions, ManagedTarget, ManagedTasks, ShutdownPolicy, State, WorkResult,
};
use nidus_core::{
    Application, ApplicationPlan, Container, LifecycleHook, LifecycleRunner, ModuleBuilder,
    ModuleGraph, NidusError, Resource,
};
use std::{
    future::pending,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
    },
    time::Duration,
};
use tokio::sync::{Barrier, Notify};

#[derive(Default)]
struct Events {
    clock: AtomicUsize,
    closed: AtomicUsize,
    closes: AtomicUsize,
    done: Notify,
}
struct Cleanup(Arc<Events>, bool);
#[async_trait]
impl LifecycleHook for Cleanup {
    async fn on_shutdown(&self) -> nidus_core::Result<()> {
        self.0.closes.fetch_add(1, SeqCst);
        self.0
            .closed
            .store(self.0.clock.fetch_add(1, SeqCst) + 1, SeqCst);
        self.0.done.notify_one();
        if self.1 {
            return Err(NidusError::ApplicationBuild {
                message: "cleanup failure retained".into(),
            });
        }
        Ok(())
    }
}
fn app(events: Arc<Events>, fail_cleanup: bool) -> Application {
    Application::with_lifecycle(
        Container::new(),
        ModuleGraph::from_modules([ModuleBuilder::new("Root").build()]).unwrap(),
        LifecycleRunner::new().hook(Cleanup(events, fail_cleanup)),
    )
}
#[derive(Default)]
struct Record {
    started: AtomicBool,
    dropped: AtomicUsize,
}
struct Ticket(Arc<Record>, Arc<Events>);
impl Drop for Ticket {
    fn drop(&mut self) {
        self.0
            .dropped
            .store(self.1.clock.fetch_add(1, SeqCst) + 1, SeqCst);
    }
}

async fn admission_campaign(rounds: usize) {
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut accepted_count = 0;
        let mut rejected_count = 0;
        for round in 0..rounds {
            let events = Arc::new(Events::default());
            let application = app(events.clone(), false);
            let managed = Managed::start(async { Ok(application) }, ManagedOptions::default())
                .await
                .unwrap();
            let seeded = Arc::new(Record::default());
            let ticket = Ticket(seeded.clone(), events.clone());
            let started = Arc::new(Notify::new());
            let ready = started.clone();
            let mut signal = managed.signal();
            assert!(managed.spawner().spawn("known admitted", async move {
                ticket.0.started.store(true, SeqCst);
                ready.notify_one();
                signal.cancelled().await;
                drop(ticket);
                Ok(())
            }));
            accepted_count += 1;
            started.notified().await;
            let barrier = Arc::new(Barrier::new(9));
            let mut producers = tokio::task::JoinSet::new();
            for producer in 0..8 {
                let barrier = barrier.clone();
                let spawner = managed.spawner();
                let signal = managed.signal();
                let events = events.clone();
                producers.spawn(async move {
                    barrier.wait().await;
                    let mut records = Vec::new();
                    for index in 0..32 {
                        let record = Arc::new(Record::default());
                        let ticket = Ticket(record.clone(), events.clone());
                        let mut signal = signal.clone();
                        let accepted = spawner.spawn("racing admission", async move {
                            ticket.0.started.store(true, SeqCst);
                            signal.cancelled().await;
                            drop(ticket);
                            Ok(())
                        });
                        records.push((accepted, record));
                        if (index + producer + round) % 3 == 0 {
                            tokio::task::yield_now().await;
                        }
                    }
                    records
                });
            }
            barrier.wait().await;
            if round % 2 == 0 {
                tokio::task::yield_now().await;
            }
            managed.request_shutdown();
            let (one, two, three) =
                tokio::join!(managed.shutdown(), managed.shutdown(), managed.shutdown());
            assert!(one.is_success(), "{one:?}");
            assert!(Arc::ptr_eq(&one, &two) && Arc::ptr_eq(&two, &three));
            while let Some(records) = producers.join_next().await {
                for (accepted, record) in records.unwrap() {
                    let dropped = record.dropped.load(SeqCst);
                    assert_ne!(dropped, 0, "an admitted or rejected future survived");
                    if accepted {
                        accepted_count += 1;
                        assert!(dropped < events.closed.load(SeqCst));
                    } else {
                        rejected_count += 1;
                        assert!(!record.started.load(SeqCst));
                    }
                }
            }
            assert!(seeded.started.load(SeqCst));
            assert_ne!(seeded.dropped.load(SeqCst), 0);
            assert!(seeded.dropped.load(SeqCst) < events.closed.load(SeqCst));
            let rejected = Arc::new(Record::default());
            let ticket = Ticket(rejected.clone(), events.clone());
            assert!(!managed.spawner().spawn("known rejected", async move {
                ticket.0.started.store(true, SeqCst);
                drop(ticket);
                Ok(())
            }));
            rejected_count += 1;
            assert!(!rejected.started.load(SeqCst));
            assert_ne!(rejected.dropped.load(SeqCst), 0);
            assert_eq!(events.closes.load(SeqCst), 1);
            assert_eq!(managed.signal().state(), State::Stopped);
        }
        assert!(accepted_count >= rounds && rejected_count >= rounds);
        println!("admission campaign: {accepted_count} accepted, {rejected_count} rejected");
    })
    .await
    .expect("admission campaign exceeded watchdog");
}
#[tokio::test]
async fn single_thread_admission_shutdown_conserves_owned_futures() {
    admission_campaign(16).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn multithread_admission_shutdown_conserves_owned_futures() {
    admission_campaign(64).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn last_owner_drop_closes_admission_and_completes_cleanup() {
    tokio::time::timeout(Duration::from_secs(5), async {
        for _ in 0..32 {
            let events = Arc::new(Events::default());
            let application = app(events.clone(), false);
            let managed = Managed::start(async { Ok(application) }, ManagedOptions::default())
                .await
                .unwrap();
            let clone = managed.clone();
            let spawner = managed.spawner();
            let mut signal = managed.signal();
            drop(managed);
            assert!(signal.is_ready());
            drop(clone);
            signal.cancelled().await;
            events.done.notified().await;
            while signal.state() != State::Stopped {
                tokio::task::yield_now().await;
            }
            assert!(!spawner.spawn("rejected after last drop", async { panic!("must not run") }));
            assert_eq!(events.closes.load(SeqCst), 1);
        }
    })
    .await
    .expect("last-owner cleanup exceeded watchdog");
}

struct PartialTarget {
    application: Application,
    events: Arc<Events>,
    record: Arc<Record>,
    panic: bool,
}
#[async_trait]
impl ManagedTarget for PartialTarget {
    fn application(&self) -> &Application {
        &self.application
    }
    async fn start(&self, tasks: &mut ManagedTasks) -> WorkResult {
        let ticket = Ticket(self.record.clone(), self.events.clone());
        tasks.spawn("partially installed serving task", async move {
            ticket.0.started.store(true, SeqCst);
            pending::<()>().await;
            drop(ticket);
            Ok(())
        });
        if self.panic {
            panic!("serving startup panic retained");
        }
        Err(std::io::Error::other("serving startup error retained").into())
    }
}
#[tokio::test(start_paused = true)]
async fn partial_serving_error_and_panic_join_tasks_before_failing_cleanup() {
    for panic in [false, true] {
        let events = Arc::new(Events::default());
        let record = Arc::new(Record::default());
        let target = PartialTarget {
            application: app(events.clone(), true),
            events: events.clone(),
            record: record.clone(),
            panic,
        };
        let mut options = ManagedOptions::default();
        options.shutdown = ShutdownPolicy {
            drain_timeout: Duration::from_millis(10),
            cleanup_timeout: Duration::from_secs(1),
        };
        let report = match Managed::start(async { Ok(target) }, options).await {
            Ok(_) => panic!("partial serving startup unexpectedly succeeded"),
            Err(report) => report,
        };
        assert!(report.deadline_expired);
        let errors = format!("{report:?}");
        assert!(errors.contains(if panic {
            "serving startup panic retained"
        } else {
            "serving startup error retained"
        }));
        assert!(errors.contains("cleanup failure retained"));
        assert!(record.dropped.load(SeqCst) > 0);
        assert!(record.dropped.load(SeqCst) < events.closed.load(SeqCst));
        assert_eq!(events.closes.load(SeqCst), 1);
    }
}

#[derive(Default)]
struct Composition {
    log: Mutex<Vec<&'static str>>,
    entered: Notify,
    release: Notify,
    closed: Notify,
}
struct First(Arc<Composition>);
struct Second(Arc<Composition>);
#[async_trait]
impl Resource for First {
    async fn initialize(container: &Container) -> nidus_core::Result<Self> {
        let state = container.resolve::<Composition>()?;
        state.log.lock().unwrap().push("first:start");
        Ok(Self(state))
    }
    async fn shutdown(&self) -> nidus_core::Result<()> {
        self.0.log.lock().unwrap().push("first:stop");
        self.0.closed.notify_one();
        Ok(())
    }
}
#[async_trait]
impl Resource for Second {
    async fn initialize(container: &Container) -> nidus_core::Result<Self> {
        let state = container.resolve::<Composition>()?;
        state.log.lock().unwrap().push("second:entered");
        state.entered.notify_one();
        state.release.notified().await;
        state.log.lock().unwrap().push("second:start");
        Ok(Self(state))
    }
    async fn shutdown(&self) -> nidus_core::Result<()> {
        self.0.log.lock().unwrap().push("second:stop");
        Ok(())
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_composition_caller_finishes_initialization_then_reverses_cleanup() {
    tokio::time::timeout(Duration::from_secs(5), async {
        for _ in 0..32 {
            let mut container = Container::new();
            container
                .register_singleton(Composition::default())
                .unwrap();
            let state = container.resolve::<Composition>().unwrap();
            let graph = ModuleGraph::from_modules([ModuleBuilder::new("Resources")
                .resource::<First>()
                .resource::<Second>()
                .build()])
            .unwrap();
            let caller = tokio::spawn(Managed::start(
                async move {
                    let mut plan =
                        ApplicationPlan::prepare(graph, container, Default::default()).await?;
                    plan.initialize().await?;
                    Ok(plan.finish(LifecycleRunner::new()))
                },
                ManagedOptions::default(),
            ));
            state.entered.notified().await;
            caller.abort();
            assert!(matches!(caller.await, Err(ref error) if error.is_cancelled()));
            state.release.notify_one();
            state.closed.notified().await;
            assert_eq!(
                *state.log.lock().unwrap(),
                [
                    "first:start",
                    "second:entered",
                    "second:start",
                    "second:stop",
                    "first:stop"
                ]
            );
        }
    })
    .await
    .expect("composition cancellation exceeded watchdog");
}
