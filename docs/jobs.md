# Jobs

`nidus-jobs` provides a lightweight in-memory queue for synchronous background
work.

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
queue.push(SendDigest);

let report = queue.run_all();
assert!(report.is_success());
```

`run_all` executes jobs in insertion order and continues after failures. The
returned `JobReport` records completed job names and failed jobs with their
`JobError` details.
