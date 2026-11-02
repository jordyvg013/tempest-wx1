# ADR 0020 — Python API, PyO3 bindings and maturin packaging

- **Status:** Proposed; experimental implementation prototype present, native build/verification pending.
- **Date:** 2026-11-01
- **Complements:** ADR 0005 (high-level Python, compute backends and extensible parameter tables).

## Context

Before the Phase-1 prototype, the v0.4.3 workspace contained `wx1-format`, `wx1-registry` and `wx1-cli`, while `python/` contained only a README and architecture draft. The current work branch adds a PyO3 binding crate, maturin project metadata, Python wrapper, xarray backend and optional external-format reader adapters. The prototype has Python wrapper tests, but the native extension has not yet been compiled or wheel-tested in the authoring environment; it is not a verified release capability.

## Proposed decision

Build a thin Python package over the Rust storage core:

1. PyO3 provides the native binding; maturin builds the package/wheels.
2. The default install is CPU-only and does not require CUDA, ROCm, JAX or another GPU runtime.
3. NumPy, xarray, Dask, ecCodes, NetCDF and Zarr integrations are optional extras/adapters and must not become dependencies of the minimal core install.
4. The Python API exposes dataset/file lifecycle, metadata inspection, reads, writes, validation and selection semantics backed by the Rust implementation; it does not reimplement wire parsing, checksums or generation recovery in Python.
5. Public API names and signatures are not frozen by this proposal. They must be specified and tested before being advertised.

## Required design and test contracts

- **Ownership/lifetime:** define who owns files, snapshots, arrays and buffers; prevent use-after-close and dangling borrowed memory.
- **Copies:** document every copy. Buffer protocol or DLPack may be evaluated, but no zero-copy claim is permitted without measurement and lifetime tests.
- **NumPy:** roundtrip dtype, shape, strides/contiguity policy, byte-order, missing values and bit-exact values for lossless profiles.
- **xarray:** implement a lazy backend that opens metadata without materializing the complete dataset; reads must be chunk-selective and tested for unexpected eager loads.
- **Dask:** chunked execution, deterministic close/cleanup and representative reductions; task graphs must not silently materialize all chunks.
- **Errors:** map Rust errors to documented Python exception classes without losing corruption/validation distinctions.
- **GIL/concurrency:** document which operations release the GIL and verify thread-safety and writer serialization rules.
- **Packaging:** build and install a CPU-only wheel in a clean environment; test supported OS/Python/Rust targets explicitly.
- **Optional adapters:** NetCDF, Zarr and GRIB2/ecCodes use local deterministic fixtures; report metadata preservation and all unsupported semantics.

## Compatibility and wire-format impact

This decision is an API/package decision and must not change the 72-byte generation descriptor, the separate 40-byte legacy chunk descriptor, existing footer/index layouts or metadata schemas. Python API compatibility is versioned separately from the WX-1 wire format.

## Alternatives

- **Pure Python parser:** rejected for the core because it would duplicate safety-critical binary parsing and integrity logic.
- **CFFI/handwritten CPython API:** not selected at this stage; PyO3 is the intended Rust binding path, subject to prototype and platform verification.
- **GPU-required package:** rejected; CPU-only installation must remain independently usable.

## Consequences and open questions

- Fase 1 is implementation work, not merely verification. A PyO3/maturin prototype is now present in `bindings/wx1-python` and `python/`; it is not considered verified until the extension compiles, a CPU-only wheel installs, and native roundtrip tests pass.
- The exact Python API surface, supported Python versions, wheel matrix, optional dependency extras and stable/lazy dataset object model remain to be specified. The initial writer is intentionally limited to one root-level variable per file; coordinate construction and full metadata authoring remain open.
- This ADR is **proposed only**. Do not implement against it as accepted policy until reviewed and accepted.
