# WX-1 development roadmap v0.4.2

## Verified foundation

- v0.4.1-fix1 was verified by the user in WSL: 105 workspace tests passed, formatting and Clippy passed, CLI help and the scientific demo/slice flow passed, and generic generation/index regressions passed.
- v0.4.2 was subsequently verified by the user in WSL: all 108 workspace tests passed, formatting and Clippy with `-D warnings` passed, and the scientific, corruption, repair, compatibility, CLI and generic generation/index regressions all passed.
- v0.4.x scientific files retain header minor 6, metadata schema v2, footer v2 and the existing primary directory/index/descriptor layouts.

## v0.4.2 — scientific read planning and recovery hardening

### Implemented and user-verified in WSL

- Public experimental `ArrayReadPlan` and `ScientificSnapshot::plan_array_slice`.
- `read_array_slice` uses the same planner, preserving the 64 MiB output bound and selected-chunk-only decode behavior.
- A deterministic 64×64 U16 dataset split into 256 4×4 chunks tests exact selection planning and row-major output bytes.
- Scientific stored-byte corruption regression test for CRC32C rejection during open.
- Scientific trailing-tail recovery test for explicit `repair_generation_file` behavior.
- Draft spec and ADR 0018 document the API and its boundaries.

### Remaining follow-on work

1. Reproducible Raw/Zstandard/LZ4 throughput, latency and memory benchmarks on representative array sizes/chunk shapes (started in v0.4.3).
2. Planning performance for variables with very high chunk counts; the initial plan builder is O(number of variable chunk references).
3. An explicit extension design for time, vertical and spatial selections, including calendars, coordinate units, CRS, axis order and longitude wrapping.
4. Stable importer API review, plus exact metadata/provenance retention rules.
5. Fuzz and additional corruption/recovery coverage; deterministic unit tests do not replace power-loss or independent-reader validation.

## Later milestones

- GRIB2/ecCodes import only after the scientific metadata and importer contracts are sufficiently precise.
- Python/PyO3, NumPy, xarray, Dask, provider registries, GPU and compute scheduling remain later roadmap stages.
- No stable format, stable API, interoperability or performance claim is made by v0.4.2.
