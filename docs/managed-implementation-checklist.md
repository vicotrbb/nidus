# Application composition and lifecycle acceptance

Baseline: main, 99642ed0018c6da5c921db92d966f3b52a7e7d77, initially clean.

- [x] Cancellation accounting: success/error/drop before and after poll/timeout/completed drop/concurrency/exclusion/panic/readiness.
- [x] Shared graph, registration, async initialization, controller assembly and application container.
- [x] Explicit resource overrides before initialization; request scope parity and manual routers.
- [x] Managed startup, resource rollback, router/bind failure and lifecycle caller cancellation.
- [x] Readiness withdrawal, HTTP/worker drain, reverse resource cleanup, telemetry flush, bounded exactly-once shutdown.
- [x] End-to-end module/resource/scope/metrics/worker scenario.
- [x] Documentation and complete example inventory with per-example evidence.
- [x] Entire diff review and findings fixed.
- [x] Executed final formatting, Clippy, workspace tests/doctests/compile-fail, feature matrix, examples/services, docs, benchmarks, dependency/security policy, semver and packaging checks; outcomes and explicit limitations recorded.

Compatibility: existing low-level lifecycle runner remains repeatable; managed APIs own exactly-once cleanup. Existing metrics retain response-availability latency and service-error labels; cancellations use an additive counter without an HTTP status. Opaque routers/factories cannot be introspected. Typed resource metadata enables replacement and cleanup ownership.

Final evidence: [implementation and validation report](validation/nidus-improvements.md).
Completed 2026-09-09 02:05 UTC (2026-09-08 23:05 America/Sao_Paulo).
Source fingerprint before/after final pass:
`62e63fd00ebc5a632b2160b7d6a7e63a7f8da0820c2b863f2da70fb297f73853`.

- [x] Final audit passed after precise h2 0.4.16 security update; existing exceptions unchanged.
- [x] MySQL readiness race corrected with authenticated TCP probe; complete service and final suites repeated.
- [x] All 26 example executables validated, including standalone local-patch consumers and service-backed binaries; owned service cleanup verified.
- [x] 23 supported library semver checks and all 25 package-list preflights passed.
- [ ] Full registry-backed tarball verification: blocked at nidus-http because published nidus-core 1.0.17 lacks the new APIs. First seven crates verified; subsequent 17 not reached. Coherent release/version sequencing remains a separately authorized step.

No code changes followed the final pass. Only this checklist and its linked results report were completed afterward. No commit, push, merge, tag, publication or deployment was performed.
