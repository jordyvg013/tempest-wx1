# ADR 0021 — cross-format scientific benchmark suite

- **Status:** Proposed; not accepted, implemented or tested.
- **Date:** 2026-11-01
- **Related:** ADR 0019 (internal WX-1 Rust microbenchmark), ADR 0020 (Python API proposal).
- **Specification draft:** [`docs/spec/WX1_Cross_Format_Benchmark_Draft_v0.1.md`](../spec/WX1_Cross_Format_Benchmark_Draft_v0.1.md).

## Context

ADR 0019 adds a dependency-free Cargo benchmark for internal WX-1 code paths: codecs, generation open/validation, array reads and the current linear planner. It does not compare WX-1 with GRIB2, NetCDF4, Zarr 3 or other storage formats. An independent suite is required to compare complete adapter workflows on identical datasets, precision/error bounds and read/write workloads.

## Proposed decision

Maintain two separate benchmark systems:

1. **Internal Rust microbenchmark (ADR 0019):** continues to profile WX-1 internals and guide single-change optimization. Its results remain format-internal and must not be represented as cross-format performance.
2. **Cross-format suite (`bench/`, this ADR):** provides one adapter protocol and common runner for WX-1, GRIB2/ecCodes, NetCDF4, Zarr 3 and later formats. It compares the same fixtures, semantic dimensions, precision/error limits, workload definitions and durability guarantees.

The internal microbenchmark may identify promising settings or regressions, but does not replace the cross-format measurements. The suites report separately and may share raw metrics only where measurement semantics match.

## Required protocol

- Adapter capabilities and unsupported features are declared, never silently substituted.
- Roundtrip tests use small local fixtures and run offline.
- Dataset manifests include provenance, licenses/terms, version/date, dimensions, coordinates, units, dtype/precision, fixture recipe and SHA256.
- Lossless and lossy profiles are never combined in one ranking. Lossy comparisons require identical declared error limits and report max absolute error, RMSE and bias.
- Buffered writes and durable writes are separate modes. An adapter must document its actual sync guarantee; a userspace flush is not labeled durable.
- Cold and warm reads are reported separately. Cold-cache claims require recorded cache-control steps; otherwise the run is not labeled cold.
- Every run stores raw CSV/JSON, configuration, randomization seed, hardware/software/filesystem manifest and logs.
- Formal results require at least five complete runs and at least 30 cold plus 30 warm repetitions per read workload, excluding the first warm-up run.
- The user-approved thresholds are frozen in `bench/config/acceptance.yaml` before the first formal cross-format measurement and cannot be changed after seeing results.

## Scope and phases

Initial adapters: WX-1, GRIB2/ecCodes, NetCDF4 and Zarr 3 with local synthetic roundtrip fixtures. Workloads are introduced in documented stages: R1–R9; W1–W6; other formats; robustness and downstream workloads. HDF5, Parquet/Arrow, raw F32+Zstd and optional TileDB/COG are later additions. MinIO/object-store simulation is optional until adapters are operational.

## Durability and crash semantics

W2 append and W4 restart report both buffered and durable modes. The durable mode must execute and document the sync sequence actually needed by that adapter (for example file sync and, where atomic publication requires it, parent-directory sync). C1 process-kill tests (`kill -9`) are distinct from VM/device power-loss tests. Do not compare formats as durable if their guarantees differ.

## Acceptance targets

The initial frozen targets are recorded in `bench/config/acceptance.yaml`: R1 100-GB/1-GB open-time ratio ≤2.0; R2 at least 2× lower latency than both GRIB2 and Zarr 3 in cold and warm states; file size below GRIB2 CCSDS at equal error bound; W5 I/O stall ≤5% of compute-only model-step time; W6 asynchronous checkpoint step overhead ≤5%; D4 overhead ≤5% versus compute-only and not worse than Zarr 3; C1 last fully durable committed step remains valid; C2 all declared byte-corruption cases detected; W4 restart bit-exact including PRNG state.

These are acceptance goals, not measured results. If benchmark semantics do not support a fair comparison, mark the result incomparable rather than weakening the threshold.

## Alternatives

- **Use ADR 0019 as the only suite:** rejected because the existing Cargo benchmark only measures WX-1 internals.
- **Mix microbench and cross-format results:** rejected because their workloads and measurement boundaries differ.
- **One shared codec setting for all formats:** rejected; small parameter sweeps are allowed, but settings and defaults must be reported separately.

## Consequences and open questions

- `bench/` is a planned suite; no cross-format adapter or formal multi-format result is claimed by this proposal.
- Adapter API, format-specific sync semantics, dataset licenses and fixture acquisition require implementation review.
- This ADR is **proposed only** and must be reviewed/accepted before implementing the suite.
