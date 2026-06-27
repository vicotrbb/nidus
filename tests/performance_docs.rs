//! Regression coverage for benchmark documentation drift.

use std::{fs, path::Path};

const DEPENDENCY_RESOLUTION_BENCH: &str = include_str!("../benches/dependency_resolution.rs");
const ROUTING_BENCH: &str = include_str!("../benches/routing.rs");
const REQUEST_LIFECYCLE_BENCH: &str = include_str!("../benches/request_lifecycle.rs");
const PERFORMANCE_DOCS: &str = include_str!("../docs/performance.md");

const EXPECTED_BENCHMARKS: &[(&str, &str)] = &[
    (
        "nidus singleton dependency resolution",
        "Nidus singleton dependency resolution",
    ),
    ("raw axum route composition", "raw Axum route composition"),
    (
        "nidus controller route composition",
        "Nidus controller route composition",
    ),
    ("raw axum baseline request", "raw Axum baseline request"),
    ("nidus hello-world request", "Nidus hello-world request"),
    ("nidus hello-world app", "Nidus hello-world app"),
    (
        "nidus controller + service request",
        "Nidus controller + service request",
    ),
    (
        "nidus controller + service app",
        "Nidus controller + service app",
    ),
    ("nidus controller setup", "Nidus controller setup"),
    ("nidus guarded route", "Nidus guarded route"),
    ("nidus validation route", "Nidus validation route"),
    ("nidus request-scoped route", "Nidus request-scoped route"),
    (
        "nidus middleware security headers request",
        "Nidus middleware security headers request",
    ),
    (
        "nidus middleware body limit request",
        "Nidus middleware body limit request",
    ),
    (
        "nidus middleware legacy request id request",
        "Nidus middleware legacy request ID request",
    ),
    (
        "nidus middleware validated request id request",
        "Nidus middleware validated request ID request",
    ),
    (
        "nidus middleware request context request",
        "Nidus middleware request context request",
    ),
    (
        "nidus middleware error envelope success request",
        "Nidus middleware error envelope success request",
    ),
    (
        "nidus middleware timeout response request",
        "Nidus middleware timeout response request",
    ),
    (
        "nidus api defaults production request",
        "Nidus API defaults production request",
    ),
    (
        "nidus api defaults production with metrics request",
        "Nidus API defaults production with metrics request",
    ),
    (
        "nidus metrics record response",
        "Nidus metrics record response",
    ),
    (
        "nidus metrics record inner error",
        "Nidus metrics record inner error",
    ),
    ("nidus metrics render text", "Nidus metrics render text"),
];

#[test]
fn benchmark_source_labels_match_expected_surface() {
    let mut source_labels = benchmark_source_labels();
    source_labels.sort_unstable();

    let mut expected_labels = EXPECTED_BENCHMARKS
        .iter()
        .map(|(source_label, _docs_label)| *source_label)
        .collect::<Vec<_>>();
    expected_labels.sort_unstable();

    assert_eq!(
        source_labels, expected_labels,
        "update EXPECTED_BENCHMARKS and docs/performance.md when the Criterion surface changes",
    );
}

#[test]
fn performance_docs_cover_every_current_benchmark() {
    for (source_label, docs_label) in EXPECTED_BENCHMARKS {
        assert!(
            PERFORMANCE_DOCS.contains(docs_label),
            "docs/performance.md must mention benchmark `{source_label}` as `{docs_label}`",
        );
    }
}

#[test]
fn request_lifecycle_result_artifact_covers_current_benchmark_surface() {
    let result_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("benchmarks/results/2026-06-27-request-lifecycle-wave43.md");
    let result = fs::read_to_string(&result_path).unwrap_or_else(|error| {
        panic!(
            "failed to read benchmark result artifact {}: {error}",
            result_path.display()
        )
    });

    for source_label in benchmark_labels_from(REQUEST_LIFECYCLE_BENCH) {
        assert!(
            result.contains(&format!("| `{source_label}` |")),
            "benchmark result artifact must include request-lifecycle row `{source_label}`",
        );
    }
}

fn benchmark_source_labels() -> Vec<&'static str> {
    [
        DEPENDENCY_RESOLUTION_BENCH,
        ROUTING_BENCH,
        REQUEST_LIFECYCLE_BENCH,
    ]
    .into_iter()
    .flat_map(benchmark_labels_from)
    .collect()
}

fn benchmark_labels_from(source: &'static str) -> Vec<&'static str> {
    source
        .lines()
        .filter_map(|line| {
            let label_start = line.find("bench_function(\"")? + "bench_function(\"".len();
            let label_end = line[label_start..].find('"')? + label_start;
            Some(&line[label_start..label_end])
        })
        .collect()
}
