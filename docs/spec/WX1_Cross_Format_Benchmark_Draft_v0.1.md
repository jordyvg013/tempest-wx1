# WX-1 cross-format scientific benchmark suite — specification draft v0.1

**Status:** proposal; suite not implemented and no cross-format results measured.
**Related decisions:** ADR 0019 (existing internal Rust microbenchmark), ADR 0021 (proposed cross-format suite).
**Authority:** `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md` contains the complete workload catalogue and phase gates; this draft defines the suite boundary and rules.

## 1. Two distinct benchmark systems

- **Internal WX-1 microbenchmark (ADR 0019):** Cargo benchmark at `crates/wx1-format/benches/wx1_performance.rs`. It measures internal codec, open/validation, selection-read and linear-planner code paths. It does not compare formats.
- **Cross-format suite (this draft / ADR 0021):** common adapters and workloads compare complete storage workflows for WX-1, GRIB2, NetCDF4 and Zarr 3. It is separate from the Rust microbenchmark. Internal microbench findings can inform hypotheses/settings but cannot substitute for cross-format evidence.

## 2. Adapter contract

The conceptual adapter operations are `write`, `open`, `read_field`, `read_level_all_times`, `read_point_series`, `read_region`, `read_tile`, `scan_all` and `close`. These names define a benchmark protocol, not current WX-1 APIs or CLI commands. Each adapter declares supported operations, format version, codecs, chunk/shard settings, metadata semantics and durability behavior. Unsupported behavior is reported, never silently substituted.

Adapters must preserve or explicitly report differences in dtype/precision, dimensions, coordinates, time/calendar, vertical coordinates, units, CRS/grid mapping, missing values, packing, ensemble/member and tile topology.

## 3. Data and reproducibility

- Tests and benchmark runs operate offline on local fixtures with SHA256 manifests.
- Network is used only by explicit dataset-download scripts.
- Initial cross-format adapters: WX-1, GRIB2/ecCodes, NetCDF4 and Zarr 3.
- Lossless and lossy comparisons are separate. Lossy requires the same declared per-variable error bounds and reports max absolute error, RMSE and bias.
- Run manifests capture hardware, filesystem/device, OS/kernel, runtime/library/compiler versions, compiler flags, WX-1 commit/version, adapter settings, seed and cache state.
- Require at least five full runs and at least 30 cold plus 30 warm repetitions per read workload; first execution is warm-up.
- Cold-cache status is claimed only when cache-control operations succeeded and were recorded.

## 4. Buffered and durable writes

W2 append and W4 restart measure both:

1. **Buffered:** writer and normal file writes without an explicit durability sync.
2. **Durable:** the adapter's documented sync sequence, including file sync and parent-directory sync where needed for atomic publication.

`flush()` is not automatically durable. Adapters report their actual guarantee; incomparable guarantees are not directly ranked. The v0.4.3 microbenchmark excludes `fsync` and remains a buffered-write measurement only.

C1 tests process-crash points and recovery. `kill -9` is not physical power loss; VM/device power-loss testing is separate.

## 5. Workloads

- Reads: R1–R9, cold and warm.
- Writes: W1–W6. W6 compares compute-only, sync WX-1, async WX-1, async Zarr 3 and async NetCDF4 in an end-to-end checkpoint/model-loop measurement. CPU is first; GPU follows the GPU/API gate.
- Robustness: C1–C3.
- Downstream: D1–D5.

The complete definitions and exact measured fields are maintained in the combined development/benchmark specification.

## 6. Frozen acceptance targets

`bench/config/acceptance.yaml` is frozen before the first formal cross-format measurement. It sets: R1 100-GB/1-GB open-time ratio ≤2.0; R2 at least 2× lower latency than both GRIB2 and Zarr 3, cold and warm; size below GRIB2 CCSDS at equal error bound; W5 stall ≤5% of compute-only step time; W6 asynchronous WX-1 step overhead ≤5%; D4 asynchronous overhead ≤5% versus compute-only and not slower than Zarr 3; C1 last fully durable committed step remains valid; C2 all declared byte corruptions are detected; W4 restart is bit-exact including PRNG state.

These are targets, not measured results. Do not revise the thresholds after viewing results. Any future threshold change needs a new config version and explicit reason; old run records remain immutable.

## 7. Reports

Produce one CSV per run, machine/software/filesystem manifest, raw JSON and reproducible plots/tables. Include “where WX-1 loses” and measurement limitations. Report buffered and durable modes separately and do not merge lossless/lossy rankings.

## 8. Not implemented

No cross-format adapter, common runner, download/fixture suite, `make bench-*` targets or multi-format result is claimed by this draft.
