# WX-1 development roadmap v0.4.3

**Status:** door de gebruiker in WSL geverifieerd als experimentele interne benchmarkbaseline. De volledige multi-format benchmarkfase is nog niet gestart.

## Goal

Establish a repeatable performance and memory baseline before changing planner data structures, chunk geometry or codec-buffer behavior. Correctness and bounded memory remain higher priorities than speed.

## Implemented in this candidate

- Dependency-free Cargo bench target: `wx1_performance`.
- Separate Raw, Zstandard and LZ4 writer/read workloads over a deterministic 2048×2048 F32 field with 64 medium chunks.
- Full-array and 256×256 window read timings.
- A 4096-small-chunk workload measuring the current linear `plan_array_slice` path for a 256×256 selection.
- Median timing, MiB/s, complete-file ratio, open/validation time and per-case process peak RSS on Linux/WSL.
- `WX1_BENCH_DIR` to select the filesystem used for temporary measurement files.
- No new crate dependencies and no wire-format change.

## Door gebruiker aangeleverde WSL-resultaten

De gebruiker heeft het benchmarktarget gebouwd en uitgevoerd op `/tmp` en `/mnt/d`. De exacte metingen en beperkingen staan in `../WX1_CURRENT_STATUS.md`. Dit betrof één volledige benchmarkinvocatie per locatie met interne medianen; er zijn geen vijf volledige runs, 30 koude/warme herhalingen of externe formaatvergelijkingen uitgevoerd.

## Not implemented or not yet verified

- No claim that the planner has been accelerated; the current linear scan remains unchanged.
- No per-function allocation counts or allocator instrumentation; Linux `VmHWM` is process high-water RSS.
- No durable-write benchmark; `fsync` latency is excluded.
- No benchmark using real KNMI/ECMWF data, GRIB2 import, parallel workers, GPU or Python.
- No automatic performance threshold; no benchmark values are claimed before WSL execution.

## Verification and next decisions

1. Workspace formatting, 108 tests, Clippy and CLI-/regressionchecks were by the user reported as passed.
2. The user built the benchmark target with `cargo bench --no-run`.
3. The user ran the benchmark on `/tmp` and `/mnt/d`.
4. CPU, WSL RAM/kernel and Rust/Cargo versions were recorded; filesystem type, disk model, affinity, thermal state and independent full-run count were not recorded.
5. Use these preliminary results only as a starting point; the formal Phase-2 multi-format benchmark and its fixed thresholds remain future work.
6. Repeat all correctness and benchmark workloads after any optimization and compare like-for-like environments.

## Commands

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo bench -p wx1-format --bench wx1_performance --no-run
cargo bench -p wx1-format --bench wx1_performance
```

The complete benchmark protocol is described in `docs/spec/WX1_Performance_Benchmarking_Draft_v0.4.3.md`; the architecture decision is ADR 0019.
