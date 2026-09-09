#![forbid(unsafe_code)]
use axum::{Router, body::Body, http::Request, routing::get};
use nidus_http::middleware::{ApiDefaults, PrometheusMetrics};
use std::{hint::black_box, time::Instant};
use tower::ServiceExt;
#[cfg(feature = "allocations")]
#[global_allocator]
static GLOBAL: &stats_alloc::StatsAlloc<std::alloc::System> = &stats_alloc::INSTRUMENTED_SYSTEM;

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mode = std::env::args().nth(1).unwrap_or_else(|| "metrics".into());
    let count: usize = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "200000".into())
        .parse()
        .unwrap();
    assert!(count > 0);
    #[cfg(feature = "managed")]
    if mode == "managed_start_stop" {
        use nidus_core::lifecycle::managed::{Managed, ManagedOptions};
        use nidus_core::{ApplicationPlan, Container, LifecycleRunner, ModuleBuilder, ModuleGraph};
        let mut operation = || {
            runtime.block_on(async {
                let graph =
                    ModuleGraph::from_modules([ModuleBuilder::new("Empty").build()]).unwrap();
                let managed = Managed::start(
                    async {
                        let mut plan =
                            ApplicationPlan::prepare(graph, Container::new(), Default::default())
                                .await?;
                        plan.initialize().await?;
                        Ok(plan.finish(LifecycleRunner::new()))
                    },
                    ManagedOptions::default(),
                )
                .await
                .unwrap();
                assert!(managed.signal().is_ready());
                assert!(managed.shutdown().await.is_success());
            })
        };
        measure(&mode, count, &mut operation);
        return;
    }
    let metrics = PrometheusMetrics::new();
    let base = Router::new().route("/proof", get(|| async { "ok" }));
    let router = match mode.as_str() {
        "raw" => base,
        "metrics" => base.layer(metrics.layer()),
        "production_metrics" => ApiDefaults::production("perf").metrics(metrics).apply(base),
        _ => panic!("unknown measurement"),
    };
    let mut operation = || {
        let request = Request::builder()
            .uri("/proof")
            .header("x-request-id", "018f4ad7-56ce-4f6a-a759-29f4438d8d78")
            .body(Body::empty())
            .unwrap();
        let response = runtime.block_on(router.clone().oneshot(request)).unwrap();
        assert!(response.status().is_success());
        black_box(response);
    };
    measure(&mode, count, &mut operation);
}
fn measure(mode: &str, count: usize, operation: &mut impl FnMut()) {
    for _ in 0..10000 {
        operation();
    }
    #[cfg(feature = "allocations")]
    let region = stats_alloc::Region::new(GLOBAL);
    let started = Instant::now();
    for _ in 0..count {
        operation();
    }
    let elapsed = started.elapsed().as_nanos() as f64 / count as f64;
    #[cfg(feature = "allocations")]
    {
        let stats = region.change();
        println!(
            "{{\"mode\":\"{mode}\",\"iterations\":{count},\"allocations\":{},\"deallocations\":{},\"reallocations\":{},\"bytes_allocated\":{},\"bytes_deallocated\":{}}}",
            stats.allocations,
            stats.deallocations,
            stats.reallocations,
            stats.bytes_allocated,
            stats.bytes_deallocated
        );
        black_box(elapsed);
    }
    #[cfg(not(feature = "allocations"))]
    println!("{{\"mode\":\"{mode}\",\"iterations\":{count},\"ns_per_operation\":{elapsed}}}");
}
