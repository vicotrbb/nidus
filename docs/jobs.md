# Jobs

`nidus-jobs` provides lightweight in-memory queues for synchronous and
Tokio-backed asynchronous background work.

```rust
struct SendDigest;

impl Job for SendDigest {
    fn name(&self) -> &'static str {
        "send_digest"
    }

    fn run(&self) -> nidus_jobs::Result<()> {
        Ok(())
    }
}

let mut queue = JobQueue::new();
assert!(queue.is_empty());
queue.push(SendDigest);
assert_eq!(queue.len(), 1);

let report = queue.run_all();
assert!(report.is_success());
```

`run_all` executes jobs in insertion order and continues after failures. The
returned `JobReport` records completed job names and failed jobs with their
`JobError` details.

Use `AsyncJob` and `AsyncJobQueue` when a job awaits I/O or other Tokio tasks:

```rust
struct SendDigest;

#[async_trait::async_trait]
impl AsyncJob for SendDigest {
    fn name(&self) -> &'static str {
        "send_digest"
    }

    async fn run(&self) -> nidus_jobs::Result<()> {
        Ok(())
    }
}

let mut queue = AsyncJobQueue::new();
queue.push(SendDigest);

let report = queue.run_all().await;
assert!(report.is_success());
```
