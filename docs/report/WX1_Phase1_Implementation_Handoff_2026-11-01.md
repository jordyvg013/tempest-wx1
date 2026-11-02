# WX-1 Phase 1 implementation handoff — 2026-11-01

**Base:** user-verified `WX1_standalone_v0.4.3`  
**Working status:** experimental Phase-1 prototype; verification gate not passed  
**Wire compatibility:** no WX-1 wire-format changes

## 1. Scope delivered in this working revision

- Added the `wx1-python-native` PyO3 crate under `bindings/wx1-python` and included it in the workspace.
- Added a maturin project under `python/` with a CPU-only base package and optional extras for NumPy, xarray, Dask, NetCDF4, Zarr and GRIB2/ecCodes.
- Added an experimental Python convenience API for opening, inspecting, planning and reading scientific WX-1 variables, plus an initial single-root-variable writer.
- Added an xarray backend with lazy per-selection array materialization. Opening still invokes the existing complete generation validation; this is not lazy open.
- Added read adapters for external NetCDF, Zarr and GRIB2 datasets through xarray. They are not full WX-1 import/export adapters and have not been shown to preserve all meteorological metadata.
- Added local/optional fixture tests and native NumPy roundtrip test cases.
- Added the current planner baseline clarification to the project status/roadmap: `6228.0 ns` (`6.228 us`) per plan at `4096` chunk references, with `black_box` present. Values at `65536` and `1048576` chunk references remain **not measured** and belong to Phase 2. Numeric reports use a decimal point and no thousands separators.

## 2. Tests actually run in this authoring environment

Environment: Python 3.13.5, NumPy 2.3.5, xarray 2026.4.0, SciPy 1.17.0 and pytest 9.0.2.

| Check | Actual result |
|---|---|
| `PYTHONPATH=python python3 -m pytest -q -rs python/tests` | 14 passed, 10 skipped |
| `python3 -m compileall -q python/wx1 python/tests` | passed |
| TOML parse of workspace, PyO3 crate and Python manifests | passed |
| Deterministic NetCDF3/SciPy local fixture + SHA256 check | passed |
| Rust `cargo fmt`, `cargo test`, `cargo clippy` | not run; Cargo/rustc unavailable |
| PyO3 native compilation | not run; Cargo/rustc unavailable |
| maturin wheel build and CPU-only wheel install | not run; maturin/Rust toolchain unavailable |
| Native NumPy↔WX-1 roundtrip | not run; extension not built |
| Dask live execution | not run; Dask unavailable |
| NetCDF4 live fixture | not run; netCDF4 unavailable |
| Zarr live fixture | not run; zarr unavailable |
| GRIB2/ecCodes fixture | not run; no local fixture or cfgrib/ecCodes installation |
| Planner scaling at 65536 and 1048576 references | not run; reserved for Phase 2 |

The latest Python-suite skip breakdown is six native-extension cases, one Dask test, one NetCDF4 test, one Zarr test and one GRIB2 fixture test. An attempt to install Dask, NetCDF4 and Zarr into a temporary virtual environment was blocked by package-index DNS resolution; no project dependencies were changed.

The generated NetCDF3/SciPy fixture `wx1-synthetic.nc` had SHA256:

`80e3b26c6ee183322e86c47a4e65e35d6a22a21c69aefb608c0db237de006b47`

`sha256sum -c SHA256SUMS.txt` reported `OK`.

## 3. Known prototype limits

- The native PyO3 crate has not been compiled or linted in this environment.
- The NumPy writer accepts one root-level variable per file and converts the input to a contiguous array, then to Python bytes, then to a Rust-owned vector. This can create multiple full-size copies; it is not zero-copy or end-to-end bounded input streaming.
- Coordinate variables, groups, grid mappings, provenance, multiple-variable writes, append and Dask-to-writer streaming are not exposed by this initial writer.
- `open_file()` fully validates the generation. Only xarray array-data materialization is lazy.
- External format adapters are read paths via xarray only; WX-1 import/export and full metadata-preservation are not implemented.
- Optional adapter tests are fixture-based and remain unverified if dependencies or fixture paths are absent.

## 4. Compatibility impact

No modifications were made to the 72-byte generation descriptor, the separate 40-byte legacy chunk descriptor, headers, footers, metadata schema, chunk directories, persistent indexes, codec IDs, CRC32C or BLAKE3 coverage. The change is additive at the workspace/package/API layer only. Native API compatibility is not stable and no v1.0 guarantee is claimed. No `Cargo.lock` was generated because Cargo is unavailable in the authoring environment; dependency resolution/lockfile reproducibility therefore remains unverified.

## 5. Files added or modified

### Added

- `bindings/wx1-python/Cargo.toml`
- `bindings/wx1-python/src/lib.rs`
- `python/pyproject.toml`
- `python/wx1/__init__.py`
- `python/wx1/xarray_backend.py`
- `python/wx1/adapters/__init__.py`
- `python/wx1/adapters/netcdf.py`
- `python/wx1/adapters/zarr.py`
- `python/wx1/adapters/grib2.py`
- `python/tests/test_cpu_only_import.py`
- `python/tests/test_external_adapters.py`
- `python/tests/test_native_roundtrip.py`
- `python/tests/test_python_api.py`
- `python/tests/test_xarray_backend.py`
- `python/tests/fixtures/README.md`
- `python/tests/fixtures/make_synthetic_fixtures.py`
- `docs/spec/WX1_Python_API_Draft_v0.1.md`
- `docs/decisions/0020-python-api-and-pyo3.md`
- `docs/report/WX1_Phase1_Implementation_Handoff_2026-11-01.md`

### Modified

- `Cargo.toml` (workspace member list)
- `README.md`
- `WX1_CURRENT_STATUS.md`
- `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md`
- `WX1_ROADMAP_CURRENT.md`
- `WX1_USER_CLI_TEST_INSTRUCTIONS.md`
- `WX1_PLAN.md`
- `WX1_ARCHITECTURE.md`
- `AGENTS.md`
- `WX1_AGENTS.md`
- `docs/CHANGELOG.md`
- `docs/decisions/README.md`
- `bench/config/internal-planner-baseline.yaml`
- `python/README.md`

## 6. Gate and next steps

Phase 1 remains open. Complete the Rust formatting/workspace tests/Clippy in WSL, compile the PyO3 crate, build and install the CPU-only wheel, and execute native NumPy roundtrips. Then install optional Dask/NetCDF4/Zarr dependencies and run their local fixture tests; supply a local GRIB2 fixture and ecCodes/cfgrib for GRIB2 verification.

Do not run the `65536`/`1048576` planner scale measurements until Phase 2. Do not implement an indexed planner before those measurements. Keep all benchmark numeric output with a decimal point and no thousands grouping.
