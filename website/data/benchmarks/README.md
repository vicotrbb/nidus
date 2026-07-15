# Published Benchmark Data

This directory contains the result artifacts used to build the public benchmark
page. It intentionally does not contain the benchmark harness, peer application
source, workload source, SQL setup scripts, container archives, or compiled
binaries.

- `v1.0.4/summary.json` preserves the original June 30, 2026 single-run website
  snapshot. It is historical evidence and is not eligible for version-delta
  calculations because its raw repetition series was not retained.
- `v1.0.12/summary.json` is generated from the publication-eligible qualified
  campaign and is the source for both the current peer table and the paired
  1.0.4-to-1.0.12 comparison. Every Nidus candidate/control row met the strict
  repeatability policy; the summary preserves the one Spring ping qualification.
- `v1.0.12/run/` contains the public result subset: raw k6 summaries, execution
  order, row-count checks, cluster-health samples, stage-pinned k6 runner
  identities, the base-analysis decision, repeatability-extension completion
  records, runtime identities, hashes, and a SHA-256 manifest.

The public subset replaces private RFC 1918 endpoint addresses with node labels
and excludes unrelated homelab workload snapshots. The qualified, unsanitized
campaign and all benchmark code remain outside this repository.
