# ADR 0019 — reproducible WX-1 performance benchmark harness

- **Status:** Accepted for the v0.4.3 experimental baseline; user-provided WSL output confirms target build and execution. This ADR covers only the internal WX-1 benchmark, not the planned multi-format suite.
- **Date:** 2026-10-30

## Context

v0.4.2 added an inspectable array-read plan, but its chunk search remains linear and selected compressed chunks are decoded in full. No quantitative throughput, latency or peak-memory baseline exists. Optimizing the read planner or changing chunk geometry without a baseline risks trading one workload for another.

## Decision

Add a dependency-free custom Cargo benchmark target `wx1_performance` under `wx1-format`.

The harness runs one isolated child process per workload so the Linux process high-water RSS (`VmHWM`) is attributable to one scenario. On systems without `/proc/self/status`, peak RSS is reported as unavailable rather than guessed.

The benchmark creates deterministic synthetic F32 fields (not observational weather data) and exercises existing public WX-1 APIs:

1. Codec workload: a 2048×2048 F32 array in 64 chunks of 256×256 elements, written and read with Raw, Zstandard and LZ4. It reports median write time/throughput, whole-file size ratio, open/validation time, full-array read time/throughput and a 256×256 window read time/throughput.
2. Planner workload: a 2048×2048 F32 array in 4096 chunks of 32×32 elements. It reports validated-open time, median `plan_array_slice` time for a 256×256 region, the selected chunk count, selection-read time/throughput and process peak RSS.

Each time is the median of multiple repetitions. All sizes and counts are fixed and printed in the result. `WX1_BENCH_DIR` optionally selects the directory used for temporary benchmark files; otherwise the OS temporary directory is used.

## Measurement semantics

- The optimized Cargo bench profile is used by `cargo bench`.
- Write timing includes the existing writer's encoding, checksums/hashes, index construction and ordinary file writes. It excludes `fsync`/durability sync and therefore is not a durable-write benchmark.
- Open/validation timing includes the existing full committed-generation validation.
- Read timing includes the existing chunk-integrity checks, decode and row-major selection assembly.
- `file_ratio` is complete WX-1 file bytes divided by raw array bytes; it includes metadata, directory, index pages and footers, not just compressed payload bytes.
- `VmHWM` is per isolated benchmark process on Linux. It is an OS-level process high-water RSS estimate, not an allocator-level per-function allocation measurement.
- Results are observational baselines, not pass/fail thresholds. No speedup or production suitability claim follows from one machine's run.

## Alternatives considered

1. **Add Criterion or another benchmarking dependency.** Deferred to keep this release dependency-free; the four fixed workloads need no statistical framework dependency.
2. **Measure all scenarios in one process.** Rejected because the process peak RSS would be cumulative and not attributable to individual workloads.
3. **Optimize chunk planning before measuring.** Rejected because no baseline exists and an optimization could change memory/performance trade-offs without evidence.
4. **Add persistent spatial/time indexes now.** Deferred because the scientific key semantics remain unspecified; opaque index keys cannot be treated as coordinate semantics.

## Consequences

- The benchmark target can be run with `cargo bench -p wx1-format --bench wx1_performance` and has no additional crate dependencies.
- The first benchmark baseline remains synthetic and host/filesystem dependent.
- The current linear planner and full selected-chunk decompression behavior remain unchanged in this increment.
- The next implementation step should use actual WSL measurements to decide whether planner indexing, chunk geometry or buffer tuning offers the best improvement.
- No wire-format bytes, codec identifiers, checksums, metadata schema or public reader/writer behavior change.


## Verification note (2026-11-01)

The user supplied WSL output showing `cargo bench --no-run` succeeded and the benchmark completed on `/tmp` and `/mnt/d`. The numeric results and measurement limitations are recorded in the repository-root `WX1_CURRENT_STATUS.md`. No multi-format comparison, five independent full runs, 30 cold/warm reads, `fsync` durability benchmark or real meteorological dataset benchmark is claimed.
