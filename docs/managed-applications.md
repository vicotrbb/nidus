# Composed and managed applications

Nidus separates composition from execution. `Nidus::create::<M>().build().await`
assembles module controllers, manual routes, and configured middleware using one
application container. `TestApp::from_application(app)` transfers that application
and its lifecycle into an in-memory test harness. It does not rebuild or strip the
configured router. `into_router()` remains the explicit low-level escape hatch
that discards application lifecycle ownership.

For applications with resources or background work, use the managed path:

```rust,ignore
use nidus::prelude::*;
use nidus::lifecycle::managed::ManagedOptions;

let app = Nidus::create::<AppModule>()
    .start_managed("127.0.0.1:3000".parse()?, ManagedOptions::default())
    .await.map_err(|report| format!("startup failed: {report:?}"))?;
shutdown_signal().await;
let report = app.shutdown().await;
assert!(report.is_success(), "{report:?}");
```

`start_managed` binds a concrete socket address, avoiding synchronous DNS.
`build_managed` starts the same application without a listener, for tests.
`start_managed_worker` uses core composition without requiring HTTP features.
The hello-world example contains a complete executable HTTP and signal example.

## Resources, dependencies and overrides

Implement `nidus::Resource` for a resource-owning application type, then declare
it with `ModuleBuilder::resource::<Database>()`. Its async `initialize(&Container)`
returns a fully started value; `shutdown(&self)` closes it. Initialization must
clean up its own partial failures. Blocking operations belong on a blocking
executor. Resource shutdown must tolerate cancellation at its deadline.

Resources initialize sequentially in module dependency order, then declaration
order within each module. Successfully initialized resources acquire cleanup
ownership immediately. Shutdown reverses that order. Import telemetry resources
before database/resource modules when telemetry must flush **after** resource
cleanup. Ordinary `lifecycle_hook` participants start after resources, and shut
down before resources, in reverse hook order. Do not register the same resource
cleanup twice. External references may retain a closed resource after shutdown.

An explicit `override_provider(value)` applies before provider registration,
async initialization and controller construction. A replaced resource's original
initializer and cleanup do not run; the replacement is externally owned. Other
providers remain registered. `with_singleton` remains an additive registration
and preserves duplicate-registration errors.

`async_initializer_for::<T>(callback)` declares the sole output of a legacy
initializer, enabling replacement of that initializer without pretending its
Rust code is introspectable. It does not acquire cleanup ownership; use a
`Resource` declaration for that. Legacy `async_initializer` remains opaque and
cannot be selectively skipped. It retains responsibility for its side effects.
Do not declare both a resource and a registrar/initializer producing that type.

The graph and declared controller paths are checked before resource initialization.
Path checks use Axum insertion semantics, including conflicting capture names.
Opaque manual routers and factories can only be checked during assembly; their
ordinary errors and unwinding trigger resource rollback. Synchronous registrars
and controller constructors run on Tokio's blocking executor during async
composition. Arbitrary factory dependency relationships cannot be inferred from
module names, and a module graph is not a security boundary for container access.

## Production and test parity

```rust,ignore
let managed = Nidus::create::<AppModule>()
    .override_provider(test_database)?
    .with_router(manual_routes)
    .with_observability(observability)
    .build_managed(ManagedOptions::default())
    .await.map_err(|report| format!("startup failed: {report:?}"))?;
let app = nidus_testing::TestApp::from_managed(managed);
app.get("/users").send().await.assert_status(StatusCode::OK);
assert!(app.shutdown_report().await.unwrap().is_success());
```

Alternatively, `TestApp::bootstrap::<M>()?.override_provider(value)?
.build_managed(options).await` shares module validation, registration,
initialization, controller assembly and request-context installation. Facade
features such as OpenAPI and observability are configured on the facade builder;
use `from_managed` to test that exact configured production router.

Requests and test-side `resolve` share provider instances. The application
installs request-scope middleware only when request-lifetime providers exist.
Each request gets an isolated scope and reuses its providers within that request.
Explicit `with_request_scope` remains supported by the test builder even when
all providers have singleton lifetimes. Manual routes remain supported.

For module tests, `.config(config)` replaces the concrete `nidus_config::Config`
provider before initializers and constructors run. It cannot automatically replace
an unrelated typed application configuration provider; override that type explicitly.
`TestApp::config()` is the explicit **harness override**, not an eagerly resolved
configuration snapshot. Use `resolve::<Config>()` for application configuration.
`from_application` and `from_managed` never invoke a configuration factory.
Router-only helpers keep their separate harness configuration behavior.

`from_router`, `builder(router)` and synchronous `build()` retain their low-level
roles. Module `build()` now assembles synchronous controllers; it panics on
composition failure or a module requiring async initialization. `try_build()`
returns these errors. `build_started()` runs async initialization and the legacy
lifecycle runner; use `build_managed()` when cancellation-safe startup and
exactly-once cleanup are required. Neither `build` nor `build_started` transfers
cleanup to a supervisor if its caller drops the composition future.

## Managed execution contract

The supervisor owns `Built → Starting → Running → Draining → Stopping → Stopped`.
Only `Running` is ready. The HTTP target gates an existing `/health/ready` route;
it does not create a health endpoint or replace application dependency checks.
The phase can also be observed through `signal()`. Draining closes task admission
and stops accepting HTTP connections. In-flight connections and managed workers
retain their dependencies until they complete or the drain deadline expires.
Managed in-memory test requests are supervised through response-body collection.
Cancelling the test caller cancels its owned request task; cancelling shutdown
waiters does not cancel cleanup.

Register workers using `ManagedOptions::worker(name, |container, signal| async
move { ... })`. Cooperative workers wait on `signal.cancelled()`, stop accepting
work, and finish outstanding work. Finite successful workers may finish normally;
worker errors and panics trigger application shutdown. `spawner()` transfers child
futures to the same task owner. Admission closes at draining, queued tasks are
included, and all admitted tasks are joined. A raw `tokio::spawn`, detached
WebSocket/upgrade handler or adapter-owned worker is not automatically supervised:
register it explicitly or give its participant an honest join/cleanup contract.

Default shutdown gives draining 30 seconds and cleanup 10 seconds. At the drain
deadline, tracked async tasks are aborted and joined before resource cleanup.
Cleanup divides its remaining budget among remaining hooks so a hanging hook
cannot prevent every later hook from being attempted. A timed-out hook future is
dropped, and cleanup continues. This policy cannot forcibly terminate blocking,
non-yielding or independently spawned work; such work may exceed the deadline.

Startup hook failure rolls back successfully started hooks and initialized
resources. Bind and router failures also trigger cleanup. Resource-composition
rollback has a 10-second cleanup budget. Errors retain the original framework
failure; rollback errors are carried in `NidusError::LifecycleStartup`. Managed
reports retain startup, task and cleanup error sources. `deadline_expired` classifies
supervisor drain/cleanup expiry. Composition rollback precedes transfer to the
supervisor: its timeout is reported inside the composition error's `rollback_errors`,
not that flag.
An unsuccessful report still reaches `Stopped`: this means cleanup was attempted,
not that every external system confirmed closure.

Repeated/concurrent managed shutdown requests return the same shared report and
execute cleanup once. Cancelling the caller awaiting startup lets the owned
supervisor finish composition and then clean up when delivery fails. Initializers
must bound their own external waits; startup does not forcibly cancel a hanging
initializer. Dropping the last managed handle requests shutdown, but synchronous
`Drop` cannot await it. Keep Tokio alive and explicitly await shutdown to establish
that tasks were joined and cleanup finished.

The existing `LifecycleRunner`, `Application::shutdown` and low-level HTTP serving
methods remain intentionally repeatable/independent. Graceful Axum serving alone
does not invoke application cleanup. Managed HTTP uses Axum/Tower handlers with
explicitly supervised Hyper HTTP/1 connections to retain cancellation and joining
ownership. It preserves peer `ConnectInfo`, response-body draining and Tower
readiness behavior. This adds direct dependencies on existing Hyper components
only to the HTTP crate, without enabling HTTP/2 or HTTP on worker-only builds.

Unwinding from resource, controller, managed startup/cleanup hooks and worker code
is reported and ordinary cleanup continues. Process aborts, panicking destructors
and termination of the Tokio runtime cannot guarantee asynchronous cleanup.

## HTTP metrics contract

A request starts at `MetricsService::call`, before the inner service is called.
An owned guard releases built-in in-flight accounting exactly once on response,
service error or cancellation, including a drop before the first poll. Latency
continues to end at **response availability**, not response-body completion.

Existing response/error counters and status labels are unchanged. Cancellation
increments the additive `nidus_http_cancelled_requests_total{method,route}` counter;
it does not invent an HTTP status, response histogram entry or service error.
Route exclusions and cardinality limits apply to cancellation too.

Custom `HttpMetricsHook` implementations remain source compatible because
`on_cancel` has a default no-op. Hooks that acquire their own in-flight accounting
must implement it to release that accounting. Hooks must not panic. Inner-service
unwinding releases built-in accounting, but an aborting panic cannot run a guard.
A custom hook is responsible for its own partial updates if it panics.
