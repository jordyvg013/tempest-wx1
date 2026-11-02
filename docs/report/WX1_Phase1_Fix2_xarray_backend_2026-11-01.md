# WX-1 Phase 1 fix2 — xarray backend integration correction

**Status:** candidate; user WSL verification pending.  
**Basis:** `WX1_standalone_v0.4.3-phase1-fix1`.  
**Scope:** Python xarray backend signature and Dask test engine argument only.

## User-provided failure evidence

The user's full Python suite on phase1-fix1 reported **20 passed, 2 failed, 2 skipped**. The next test blocks had not yet been run.

### Failure A — xarray backend signature

`Wx1BackendEntrypoint.open_dataset` accepted `**kwargs`. The installed xarray plugin registry inspects the backend method signature and rejects `VAR_KEYWORD` / `VAR_POSITIONAL` parameters. The failure occurred when `wx1.open_dataset(..., chunks=...)` selected `engine="wx1"`.

**Correction:** the backend method now declares only `filename_or_obj` and `drop_variables`, both explicitly. Chunking is managed by xarray outside the backend open call; unsupported backend-level kwargs are not silently accepted.

### Failure B — invalid engine argument in Dask test

The test `test_dask_chunked_execution_when_optional_dependency_is_installed` passed `Wx1BackendEntrypoint()` to `xr.open_dataset`. xarray expects a backend entrypoint class (or registered engine name), and attempted `issubclass(engine, BackendEntrypoint)`, raising `TypeError` for the instance.

**Correction:** the test passes `Wx1BackendEntrypoint` (the class).

## Regression coverage

Added `test_xarray_backend_signature_is_explicit_and_engine_class_works`, which checks that the backend method has no variadic signature parameters and opens a fixture through xarray using the backend class, without materializing array data.

## Executed checks in the authoring environment

Environment: Python 3.13.5, NumPy 2.3.5, xarray 2026.4.0, pytest 9.0.2. Dask, NetCDF4, Zarr, the native `_core` extension and a GRIB2 fixture were not available.

- `PYTHONPATH=python python3 -m pytest -q -rs python/tests/test_xarray_backend.py`: **5 passed, 1 skipped** (Dask missing).
- `PYTHONPATH=python python3 -m pytest -q -rs python/tests`: **15 passed, 10 skipped**.
- `python3 -m compileall -q python/wx1 python/tests`: passed.

No Rust build, wheel build, CPU-only installation, native NumPy roundtrip, Dask computation, NetCDF4/Zarr live fixture, GRIB2 fixture, or benchmark was run in this authoring environment.

## Compatibility impact

No WX-1 wire-format, descriptor, header, footer, index, metadata schema, native PyO3 binding or Rust storage behavior changed. The Python backend now follows xarray's explicit backend-entrypoint signature convention. The Dask test now supplies the documented engine class object.

## Required user verification

Re-run the full Rust standard block and full Python suite from the fix2 project root in the user's Conda `Tempest` environment. Keep `LD_LIBRARY_PATH="$CONDA_PREFIX/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"` set for Rust test binaries linked to `libpython3.12.so.1.0`. Then continue only with the previously unrun native NumPy, xarray/Dask, NetCDF4/Zarr and GRIB2 fixture blocks. Do not mark Phase 1 complete until all available required gates have actual results.
