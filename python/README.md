# WX-1 Python package (experimental)

This directory contains the Python package and maturin build configuration. The native module is implemented in `../bindings/wx1-python` and links to the existing `wx1-format` Rust crate.

## CPU-only install

From this directory, build/install the CPU-only package with:

```bash
python -m pip install .
```

The base package has no GPU or scientific-format dependencies. NumPy, xarray, Dask, NetCDF4, Zarr and GRIB2/ecCodes integrations are opt-in extras. A Rust toolchain is required to build from source; a compatible wheel avoids requiring Rust on the installation machine.

```bash
python -m pip install '.[numpy,xarray,dask]'
python -m pip install '.[netcdf]'
python -m pip install '.[zarr]'
python -m pip install '.[grib2]'
```

## API prototype

- `wx1.open_file(path)` opens a scientific schema-v2 file and fully validates the current generation using the existing Rust `open_scientific_snapshot` path. This is **not lazy open**.
- `Wx1File.read_array_slice(path, start, count)` reads only the planned chunks for that selection and returns a NumPy array if NumPy is installed.
- `wx1.write_array(...)` writes one root-level variable to a new file. It accepts chunked C-contiguous array bytes and uses the bounded reader-based Zstandard generation writer. It refuses to overwrite an existing destination.
- `wx1.open_dataset(path, engine='wx1')` exposes variables through an xarray lazy indexing backend. Metadata/generation open currently performs full container validation; only array materialization is lazy. Dask-backed chunking is available when the `dask` extra is installed.

The initial writer handles one variable per file and text-valued variable attributes. It supports multiple chunks, but does not yet expose coordinate variables, groups, grid mappings, provenance construction or multi-variable writes. Those are explicit API gaps, not silently discarded metadata. The reader preserves the scientific model's variable attributes, dimensions, byte order and dtype for variables already present in a file.

## Optional external-format adapters

- `wx1.adapters.netcdf.open_dataset`: xarray NetCDF reader; NetCDF4 support depends on the optional `netcdf` extra.
- `wx1.adapters.zarr.open_dataset`: xarray Zarr reader; supports the installed Zarr version/format.
- `wx1.adapters.grib2.open_dataset`: xarray/cfgrib reader; requires the `grib2` extra and ecCodes.

These are external-format readers, not complete WX-1 import/export or lossless roundtrip adapters. Tests use deterministic local fixtures when the required optional library is installed. GRIB2 test fixtures must be supplied locally; no network download occurs during tests.

## Status and limits

This is an experimental Phase-1 vertical slice. The current authoring environment has Python 3.13.5, NumPy 2.3.5, xarray 2026.4.0, SciPy 1.17.0 and pytest 9.0.2, but no Cargo/rustc/maturin, Dask, NetCDF4, Zarr or ecCodes. An attempt to install optional packages in a temporary venv was blocked by unavailable package-index DNS; no project dependencies were changed. The current Python suite was run with 14 passed and 10 skipped. The native extension, wheel and live NumPy↔WX-1 roundtrip remain unverified because Cargo/rustc/maturin are unavailable here. Dask, NetCDF4 and Zarr were skipped because their optional packages are absent; GRIB2 was skipped because no local fixture or cfgrib/ecCodes installation is available. These integrations must be tested in a Rust-enabled environment before Phase 1 can be declared complete.
