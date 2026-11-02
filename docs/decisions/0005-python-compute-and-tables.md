# ADR 0005: Python, compute backends and extensible parameter tables

- Status: Accepted as architecture direction
- Date: 2026-10-29

## Context

WX-1 targets meteorological/scientific Python workflows and datasets too large to fit in memory or GPU VRAM. It must also support official and user-maintained parameter tables.

## Decision

- Provide an official `wx1` Python distribution, subject to package-name availability, using PyO3/maturin bindings to the Rust core.
- Keep NumPy, xarray, Dask, MetPy, JAX and CuPy integrations optional.
- Integrate MetPy through adapters; do not fork MetPy algorithms into the storage core.
- Provide a function registry for built-in, MetPy-backed and user-authored trusted plugins.
- Provide a versioned, namespaced table registry with CSV/JSON baseline import and optional YAML/XLSX adapters.
- Support WMO/GRIB, KNMI, ECMWF, NOAA and other provider tables alongside private/user namespaces.
- Preserve table provenance, versions, license/attribution and original external codes. Never silently overwrite upstream tables.
- Support `cpu`, `gpu`, `auto` and `hybrid` execution policies.
- Use bounded chunks and explicit host/device memory budgets. A 6 GB GPU is a design target, not a guarantee that every operation fits or accelerates.

## Consequences

CPU-only install and operation must remain complete without GPU toolchains. GPU backend choice is deferred until a compatibility/performance spike. No claim is made that all MetPy functions are GPU-compatible. The file format remains independent of all these Python libraries and compute backends.
