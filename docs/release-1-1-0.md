# Nidus 1.1.0

Nidus 1.1.0 is the coordinated minor release of all 25 framework packages. It includes the complete main history through the technical white-paper publication, together with managed applications, shared composition and cancellation-safe metrics. The [candidate validation report](validation/release-1.1.0.md) records the pre-release proofs and their scope.

## Managed applications and shared composition

`ApplicationPlan` provides one graph and provider composition path for production HTTP applications, workers and module-based tests. Explicit overrides apply before asynchronous initialization. Typed initializer metadata and `Resource` declarations make ownership and replacement visible; opaque legacy initializers remain supported.

The additive managed API owns startup, serving tasks and shutdown. It reports lifecycle state, closes task admission during draining, joins accepted tasks before resource cleanup, and returns a shared structured shutdown report. Startup failures roll back initialized resources in reverse order. Caller cancellation and dropping the last managed owner request cleanup through the supervisor.

Managed HTTP serving preserves streaming-body ownership through graceful drain or forced connection cancellation. `TestApp::from_managed` retains the same application container and supervises in-memory request bodies. See [managed applications](managed-applications.md) for usage, lifecycle policy and migration examples.

Existing unmanaged APIs remain available. Managed cancellation is cooperative: blocking callbacks and futures that never yield cannot be forcibly terminated safely. Resource initialization must clean up its own partial failure, and externally supplied overrides remain externally owned.

## Cancellation-safe HTTP metrics

Metrics account for requests cancelled before their first poll, during a pending inner call, or during unwinding. `nidus_http_cancelled_requests_total` records cancellations separately from responses and inner errors; the in-flight gauge returns to zero on every terminal path. Existing hooks remain source compatible through a default cancellation callback.

Response metrics finish when a response becomes available, including its headers, rather than when a streaming body finishes. The managed server still retains ownership until the body completes or its connection is cancelled.

## Integration and release verification

The OTLP exporter enters its captured Tokio runtime while polling HTTP export operations from the SDK batch thread. CockroachDB example decoding uses the database's integer width. The locked h2 dependency includes the RUSTSEC-2026-0258 fix. Local MySQL readiness waits for authenticated TCP connections.

Release packaging now verifies all 25 crates in one Cargo invocation, allowing Cargo to resolve the unpublished cohort through its temporary registry. Clean consumer checks pin the requested version, reject workspace paths, and exercise staged archives, the packaged CLI and both standalone examples.

The candidate adds task-admission stress, streaming ownership, composition-cancellation, partial-startup failure, last-owner drop and mixed metrics conservation proofs. The validation report distinguishes passing evidence from limitations and records the controlled performance comparison, dependency checks and Linux environment.

## Compatibility and installation

The coordinated crate version is 1.1.0, using Rust 1.96 and edition 2024. Generated projects take their dependency version from the CLI package version. Applications can select `nidus-rs = "1.1.0"` (usually aliased to `nidus`) and the matching integration crates.

The technical white paper is included in the repository and linked from the website. Registry, documentation and deployment availability are verified separately during the coordinated release.
