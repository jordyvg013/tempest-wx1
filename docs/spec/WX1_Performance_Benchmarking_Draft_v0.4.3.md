# WX-1 internal Rust performance microbenchmark — draft v0.4.3

**Status:** experimental protocol for the internal WX-1 Rust microbenchmark (ADR 0019). It defines no wire-format bytes and makes no performance guarantee. The separate cross-format suite is specified by ADR 0021 and `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md`.

## 1. Scope

v0.4.3 adds a dependency-free custom Cargo benchmark target at `crates/wx1-format/benches/wx1_performance.rs`. The target calls the existing public scientific writer, snapshot opener, array planner and array reader. It does not change the WX-1 storage implementation.

## 2. Codec workload

The benchmark constructs a deterministic synthetic 2048×2048 F32 field (16 MiB raw) split into 64 chunks of 256×256 elements. It runs separate processes for Raw, Zstandard and LZ4.

Reported values:

- raw array bytes and complete WX-1 file bytes;
- complete-file/raw-array ratio (includes metadata, directories, indexes and footers);
- median generation-writer time and raw-input MiB/s;
- open/whole-generation-validation time;
- median full-array read time and MiB/s;
- median 256×256 window read time and MiB/s;
- process peak RSS where Linux `/proc/self/status` exposes `VmHWM`.

The writer is timed without an explicit `fsync` durability operation. The result is not a durable-write latency measurement.

## 3. Planner workload

A second deterministic 2048×2048 F32 field is split into 4096 chunks of 32×32 elements and stored uncompressed. The benchmark opens and validates the generation, then measures repeated calls to `ScientificSnapshot::plan_array_slice` for one fixed 256×256 window. It also reads that same window and reports the selected chunk count, planning latency, read latency/throughput, validated-open time and process peak RSS.

The current planner is linear in the variable's chunk-reference count. This benchmark measures that implementation; it does not imply that an indexed planner exists.

## 4. Reproducibility and environment

Run using an optimized profile:

```bash
cargo bench -p wx1-format --bench wx1_performance
```

Optional benchmark file location:

```bash
WX1_BENCH_DIR=/mnt/d/wx1-bench cargo bench -p wx1-format --bench wx1_performance
```

Create the directory before setting `WX1_BENCH_DIR`. On Linux/WSL, each case runs in a separate child process and reports its own `VmHWM`. Other operating systems report peak RSS as `unavailable`; the harness does not substitute an estimate.

For meaningful comparisons, record the WX-1 version, CPU, RAM, OS/kernel, Rust toolchain, filesystem and whether `WX1_BENCH_DIR` points to the Linux filesystem or a mounted Windows path. Run each comparison on an otherwise idle system and retain the raw output.

## 5. Interpretation and limitations

- The field is deterministic synthetic data, not real KNMI/ECMWF data.
- Compression ratio and performance depend on the data pattern, chunk shape, codec version and filesystem.
- Write measurements include compression, integrity work, index construction and normal file I/O; they do not include durability sync.
- Open validation scans the committed generation and validates stored payload integrity.
- Read measurements include integrity verification, decompression and selection assembly.
- `VmHWM` is process high-water RSS, not a direct measure of bytes allocated by one function.
- Repetition counts and medians reduce noise but do not replace a full statistical study.
- No thresholds are defined; benchmark output is not a test pass/fail result.
- No changes are made to the current linear read planner, chunk layout, codec level, stream-buffer default or wire format in this increment.


## 6. Relationship to the cross-format suite

This Cargo benchmark remains a separate internal microbenchmark. It is useful for profiling WX-1 codec, open/validate, array-read and planner code paths. It does not compare WX-1 with GRIB2, NetCDF4, Zarr 3, HDF5 or other formats. The separate multi-format suite lives under `bench/` and is planned under ADR 0021. Neither benchmark replaces the other.

## 7. Durable-write follow-up

The current benchmark write timings explicitly exclude `fsync`/durability synchronization. They must continue to be reported as buffered/ordinary write timings, not durable-write latency. A future benchmark extension may add a separately named durable mode, but only after the writer's actual synchronization contract is documented and tested. Durable mode must include the required file sync and, where atomic creation/replacement requires it, parent-directory sync. A normal `flush()` or userspace write completion is not equivalent to durable commit.

The formal W2 append and W4 restart workloads in the cross-format suite will report buffered and durable modes separately. Comparisons are valid only between adapters that provide equivalent documented durability guarantees. Crash tests distinguish process kill from physical power-loss testing.

## 8. Indexed planner follow-up

The current benchmark measures the linear planner at 4096 chunk references. It does not establish scaling at 65536 or 1048576 references. The post-baseline planner investigation must add those scale points (plus selection-density variants) before selecting an indexed design. No indexed planner is implemented by this v0.4.3 benchmark increment.
