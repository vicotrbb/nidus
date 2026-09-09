# Nidus 1.1.0 candidate validation

The 1.1.0 candidate passes the release-readiness gates below and is ready for the coordinated release process. At the candidate acceptance checkpoint, no commit, push, tag, publication or deployment had been performed. Readiness means the documented gates pass for the identified source and artifacts; it is not a guarantee that software has no defects.

## Candidate identity

- Baseline: `99642ed0018c6da5c921db92d966f3b52a7e7d77` (1.0.17).
- Candidate source SHA-256: `c38a5d22ffea03ab965b82dfe78992e389821f26796c7dec379f88337ee23112` across 1,267 tracked and nonignored source files.
- Source inventory and archive: `target/release-readiness/final-source/source.json` and `source.tar`.
- The checklist and this results report are archived with separate hashes and excluded only from the stable source digest, allowing evidence to be completed afterward.
- All 25 publishable packages, internal dependency lower bounds and CLI generation use version 1.1.0. Historical release documents retain their original versions. The official sparse registry check at 2026-09-09 05:18 UTC confirmed that 1.1.0 was unused for all 25 names and newer than their latest unyanked stable release (1.0.17). This availability must be rechecked at publication.
- Linux: isolated Debian Bookworm, Rust 1.96.0, aarch64 GNU, Node 24; image `sha256:21a2e8bc9ab3fce803dca12fb8647271608c63a9b8890f499527f15d7bdaf540`. No host target directory or Cargo cache was mounted. The local OrbStack socket was used only by the reviewed disposable-service harness.

The initial Linux snapshot's Rust inputs match the final candidate. Reviewed consumer-verification and service helpers were reconciled before their final stages; the final source reconciliation records the complete candidate. Reports describe actual executed checks, rather than treating a historical commit or earlier green run as current proof.

## Targeted ownership and accounting proofs

The implementation now has 40 added regression tests across application planning, managed lifecycle, HTTP composition, metrics and actual OTLP transport. Eight were added during this readiness campaign.

| Proof | Evidence and result |
| --- | --- |
| Admission racing with shutdown | 80 rounds across single-thread and multithread runtimes; 20,480 raced submissions plus controlled accepted/rejected tasks. Accepted futures are dropped before cleanup, rejected futures never execute, concurrent shutdown shares one report, and cleanup runs once. |
| Streaming responses | Real TCP and in-memory bodies remain owned until completion or forced cancellation. Body destruction precedes resource cleanup. Metrics finish at response availability, with no invented cancellation after headers. Listener reuse is checked. |
| Cancellation during composition | 32 rounds cancel the startup caller while the second resource initializer is pending, then prove completed initialization and reverse cleanup. |
| Partial serving startup | Both ordinary errors and panics after installing a task preserve the startup failure, abort/join owned work and retain cleanup failures. |
| Last managed owner dropped | 32 rounds prove admission closure and completed cleanup while observer/spawner handles remain. |
| Mixed metrics outcomes | Eight workers issue 768 mixed requests spanning success, inner error, cancellation before/after polling, unwind, exclusion and series overflow. All accounting is conserved and in-flight gauges return to zero. |

These tests extend the original production/test parity, override, request-scope, early validation, rollback, bind-failure, readiness, drain, deadline and telemetry proofs. The earlier implementation report remains historical evidence; its packaging limitation is closed by the cohort packaging described below.

## Archive and consumer provenance

All 25 packages pass a single normal `cargo package --locked --allow-dirty --all-features` cohort invocation. Cargo verifies the unpublished internal dependencies using its temporary registry. No publication or `--no-verify` waiver is needed.

`scripts/prepare-artifact-registry.py` checks normalized internal dependency versions, archive SHA-256 values and registry-index checksum agreement, then constructs a fresh closed registry from the cohort and locked third-party archives. Reusing an existing registry directory is rejected. The final manifest is `target/release-readiness/final-artifacts/cohort.json`.

Fresh Cargo homes and independent target directories exercise:

1. An all-adapter consumer with exact 1.1.0 pins and every feature, including an actual controller/resource/request/shutdown assertion.
2. The packaged CLI installed from its archive, generating and testing a fresh project, serving a real request and releasing its listener.
3. Both standalone applications, with real HTTP workflows and strict resolved-cohort checks.

Temporary consumers omit source lockfiles, generate fresh exact-version resolutions, then test and build with `--locked`. The final archive proof is deliberately launched with inherited local-patch mode enabled; its own environment forces that mode off, and every resolved Nidus source must be the canonical crates.io identity backed by the isolated archive registry. Resolved metadata and lockfiles are retained under `final-artifacts/consumer-evidence`.

The tracked standalone lockfiles describe these candidate archives. A subsequent commit/repackage can change `.cargo_vcs_info.json` and therefore archive checksums. Refresh those locks against the final archives or actual published registry before claiming post-publication `--locked` reproduction. Fresh consumer verification deliberately does not claim to reproduce stale source lockfiles.

## Acceptance results

All acceptance gates below passed on the identified candidate. Independent specification and standards reviews found no outstanding actionable issues; the final evidence review found no material overclaims.

| Gate | Result |
| --- | --- |
| Host workspace tests and doctests | 646 passed; 14 illustrative macro doctests remain ignored, and nine service tests are exercised separately with live dependencies. |
| Linux workspace tests and doctests | Passed in the isolated Linux environment. |
| Formatting, strict Clippy, feature matrix, strict rustdoc | Passed on host and Linux. |
| Semver | 23 supported libraries passed against the baseline; proc-macro and binary-only targets retain their documented tool exclusions. |
| Cohort package verification | All 25 archives verified. |
| Fresh archived consumers | All-adapter, CLI-generated and both standalone consumers passed with fresh lock generation and strict registry provenance. |
| Live examples and services | All 26 example executables plus generated CLI applications passed across the host and Linux proofs; all nine service tests passed on each platform with strict cleanup. |
| Website | Final domain/project builds, local links, benchmark assets and domain SEO passed on host and Linux, including the completed reporting pages. |
| Dependency policy and advisories | Host policy and whole-lock audit passed, retaining only the existing scoped SQLx/MySQL RSA exception. |
| Consumer security | Four graphs/locks audited; all 539 third-party versions matched official registry checksums and were not yanked. The final fresh-consumer metadata refresh passed the same checks. |
| Fuzz targets | All three built; bounded campaigns completed 657,637 configuration, 1,180,441 route and 705,575 OpenAPI executions without a crash. |
| Existing Criterion benchmarks | All 44 unchanged workloads completed with their original settings. Fresh estimates are retained separately from cached comparisons. |
| Worker-only dependencies | Normal/build dependency graph excludes `nidus-http` and Axum. |

One reused host target failed rustdoc with `E0463` while loading Axum/HTTP metadata. The isolated reproduction passed unchanged, and the complete test/doctest suite passed in a fresh target directory. The original failure remains recorded; the isolated final run is the acceptance evidence. No source fix, test suppression or relaxed compiler flag was used for this issue.

The first Linux service attempt exposed a client/daemon filesystem mismatch in certificate bind mounts. The harness now generates certificates inside a labeled container and uses explicit Docker copies for client and server files. TLS verification and strict cleanup remain unchanged. The failed run's uniquely named daemon-side test directory was inspected and removed; final service runs use no certificate bind mounts.

## Coverage and dependency inventory

`cargo llvm-cov --workspace --all-features` passed. The report measures line coverage, not branch or MC/DC coverage: 13,141 / 16,755 lines (78.43%) and 1,868 / 2,493 functions (74.93%). The targeted tests above provide behavioral assertions; percentages are not a substitute for those assertions or a claim of exhaustive failure coverage.

| Implementation file | Line coverage |
| --- | --- |
| HTTP metrics | 96.96% |
| Application plan | 90.32% |
| Managed lifecycle supervisor | 88.67%; all 42 functions exercised |
| Managed HTTP serving | 90.32% |
| Shared HTTP composition | 70.00% |
| Test application | 94.12% |
| Facade application builder | 81.29% |
| OTLP runtime exporter | 89.29% |

Machine-readable and HTML reports are under `target/release-readiness/coverage.json`, `critical-coverage.json` and `coverage-html/html`.

The CycloneDX 1.6 SBOM contains 559 top-level dependency components and 39 workspace components, including the complete 25-package 1.1.0 release cohort. Its SHA-256 is `ba4b3d88bab491d83cd213663882baad2b9fb246956fbed8e865f9d1ceface66`; the artifact is `target/release-readiness/sbom.cdx.json`.

The broader consumer audit found `chacha20 0.10.1` had been yanked. The candidate lock now uses the compatible non-yanked 0.10.2 patch. The earlier h2 0.4.16 advisory fix remains. The RSA exception is checked against every direct parent in the resolved graph and remains limited to SQLx MySQL. Standalone fuzz and benchmark-tool locks also pass advisory audits. No runtime dependency was added for performance instrumentation; `stats_alloc` is confined to the unpublished benchmark tool's optional instrumentation feature.

One host service attempt detected an added anonymous volume in the shared daemon after all service tests passed. Ownership of `edb65b76ca0f86c26ca542431e63ed0c6c8e3b4e43ae31515e77779fc25a82b8` could not be established, so it was left untouched. The unchanged rerun recorded Docker volume/container events and passed the strict global inventory comparison; every volume created by that traced run was destroyed. The failed attempt and event traces remain in the evidence directory. No cleanup assertion was waived.

## Controlled performance

Measurements ran after this campaign's compilation and service workloads stopped, on macOS 26.5.2 / arm64 with Rust 1.96.0. `scripts/measure-release-performance.py` prepares isolated baseline/candidate binaries with identical third-party versions, records source/executable hashes, rejects stale inputs, alternates ten paired runs after warmup, and separates allocation instrumentation from latency measurement. Managed composition/startup/shutdown is measured separately from request processing.

The ten-pair steady-state medians are:

| Workload | Baseline | Candidate | Change | Allocations per operation |
| --- | --- | --- | --- | --- |
| Raw Axum control | 680.64 ns | 695.40 ns | +2.17% | 18 → 18 |
| Metrics middleware | 933.78 ns | 953.59 ns | +2.12% | 23 → 23 |
| Production defaults with metrics | 2.921 µs | 3.010 µs | +3.05% | 88 → 88 |

The control also shifts, so the small timing differences should not be presented as isolated cancellation-guard cost. No additional allocation operation was measured. Metered workloads request eight additional heap bytes per operation (2,677 → 2,685 for metrics; 8,561 → 8,569 for production defaults). Allocation and deallocation counts/bytes balance in every measured workload, with no reallocations.

Separately, an empty worker application's composition, managed startup and shutdown cycle measures 35.62 µs on a warmed runtime, with 53 allocations and 13,815 allocated/deallocated bytes per cycle. This excludes process/runtime startup and real resource initialization; it is not a database-backed application's startup guarantee.

Raw samples, build/dependency provenance and executable hashes are under `target/release-readiness/performance/`. All 44 existing Criterion benchmarks completed successfully with their original workloads and settings. Fresh estimates are retained in `target/release-readiness/criterion-results.json` and the full output in `benchmarks.log`. Cached comparison percentages are not used as controlled baseline evidence.

## Reproduction and retained evidence

Start from the source inventory above and retain fresh output directories. The final host command sequence is saved in `target/release-readiness/host-final/reproduce.sh`; Linux uses `scripts/release-linux.Dockerfile` and `scripts/verify-linux-release.sh`. Service proof uses `NIDUS_VALIDATE_EXAMPLES=1 bash scripts/test-integration-services.sh`. The package/consumer sequence is:

```sh
bash scripts/package-publishable-crates.sh --allow-dirty --locked --all-features
python3 scripts/prepare-artifact-registry.py target/release-readiness/new-artifacts
python3 scripts/verify-packaged-release.py target/release-readiness/new-artifacts
python3 scripts/audit-release-consumers.py target/release-readiness/new-artifacts
```

The registry output must be new; the helper deliberately refuses reuse. The environment requires the locked archives and registry metadata available to Cargo. Performance preparation and measurement instructions are in `scripts/release-perf/README.md`; do not combine latency measurement with other compilation or service workloads.

The consolidated acceptance outcomes are `target/release-readiness/final-outcomes.tsv`. Linux final static logs, initial live-example logs and final service logs are retained separately so superseded failures remain visible. `evidence-manifest.json` records SHA-256 values for the final report, source inventory, cohort, SBOM, performance/coverage results and acceptance logs. All 25 current package archives were compared again with the tested cohort; every checksum matches.

## Release handoff and limits

The source changes were uncommitted at the candidate acceptance checkpoint. The subsequently authorized release includes remote main commit `a02d2fa8eb71fe7227e7fdd8b5c6903b302d5544`, which adds only the white-paper PDF and its website integration; these additions do not change the tested Rust implementation. Release-time site and package checks cover the combined source. After choosing the final commit/archive provenance, regenerate affected artifact and standalone-lock evidence if bytes change, publish the coordinated cohort, and run `bash scripts/verify-published-release.sh 1.1.0`. That verifier checks all package versions and docs.rs pages, then resolves fresh consumers with an isolated public-registry Cargo home. Refresh source example locks against the published registry before testing their `--locked` builds.

The tested platforms are macOS/aarch64 and Linux/aarch64. This campaign does not establish x86_64, Windows, live docs.rs publication, deployed website behavior or production workload guarantees. Managed HTTP owns HTTP/1 connections; arbitrary detached tasks, process aborts, panicking destructors and non-yielding blocking code remain outside its cleanup guarantee. Explicitly await shutdown while the Tokio runtime is alive. Resource initializers must bound and clean up their own partial failures, and overrides remain externally owned.
