# Nidus 1.1.0 candidate readiness

Baseline: `99642ed0018c6da5c921db92d966f3b52a7e7d77` (1.0.17). Preserve the prior implementation and its historical validation report. This campaign prepares and verifies an unpublished candidate; no commit, push, tag or publication is included.

- [x] Whole-cohort archive verification and clean exact-version artifact consumers.
- [x] Coherent 1.1.0 manifests, dependency lower bounds, CLI templates, standalone locks and current documentation.
- [x] Multithread admission/shutdown races, streaming response ownership, composition cancellation, partial serving failure, last-owner drop and mixed metrics conservation.
- [x] Controlled baseline/candidate request performance and allocation measurements; separate managed startup measurement.
- [x] Clean identified Linux snapshot with fresh build outputs and local service cleanup.
- [x] Coverage of critical failure paths, SBOM and resolved-consumer security checks.
- [x] Public documentation/site and generated CLI consumer checks.
- [x] Independent standards/specification reviews, fixes, and complete final acceptance on the frozen candidate.
- [x] Final reproducible report with exact outcomes, artifact hashes, limitations and release verdict.

Verdict: the identified 1.1.0 candidate passes the release-readiness gates. See [the final evidence and limitations](validation/release-1.1.0.md). Publication and verification of the final published bytes remain a separate release step.
