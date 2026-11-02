# WX-1 development roadmap v0.2

## Milestone 0.2.0 — architecture and open-source foundation

- Independent open-source project with MIT license and contribution guidance.
- Draft storage architecture, multiple-index design and table registry.
- Draft Python/MetPy/CPU-GPU execution architecture.
- ADRs for byte order, metadata, indexes, integrity/codecs/commit and Python/tables/compute.
- No stable byte-level file format or complete reader/writer yet.

## Milestone 0.3 — normative specification and threat model

- Freeze header/footer field definitions, feature flags and version matrix.
- Define CBOR schema and deterministic profile.
- Define chunk descriptor and payload alignment.
- Define index key ordering, B+tree pages and optional spatial index.
- Define checksum coverage, codec parameters and commit recovery.
- Define all normative resource limits and malicious-input behavior.
- Publish language-independent test vectors.
- Review table registry namespace, immutable versioning and licensing model.

## Milestone 0.4 — safe Rust format prototype

- Header, metadata, chunk and footer read/write.
- Checked offsets and bounded allocation/decompression.
- CRC32C and selected BLAKE3 coverage.
- Zstandard and uncompressed chunks; LZ4 after tests.
- Multiple indexes and partial reads.
- Fuzzing and corruption/recovery tests.

## Milestone 0.5 — scientific model and table registry

- Parameter identities, dimensions, coordinates, time/forecast semantics and grids.
- WMO/GRIB table adapters and versioned KNMI/provider table packages where licensing permits.
- CSV/JSON user table import, validation, namespace isolation, overlays, export and embedded table snapshots.
- GRIB/NetCDF/Zarr interoperability prototypes.

## Milestone 0.6 — Python package

- PyO3/maturin packaging and platform wheels.
- `wx1.open`, `wx1.create`, metadata/selection and chunk iteration APIs.
- NumPy and xarray adapters; Dask lazy/out-of-core integration.
- Optional MetPy adapters and function registry.
- CPU-only installation and CI matrix.

## Milestone 0.7 — GPU and hybrid compute

- CPU compute scheduler and chunk memory budget first.
- Benchmark JAX and CuPy on target environments; select backend based on compatibility and measured results.
- Explicit `cpu`, `gpu`, `auto`, `hybrid` policies.
- Bounded VRAM, backpressure, tile/halo scheduling and low-memory tests.
- Validate 6 GB VRAM operation with representative weather workloads; publish measured limits, not assumptions.

## Milestone 1.0 — stable format

- Independent implementations and cross-language test vectors.
- Security review, fuzzing, recovery tests and interoperability validation.
- Stable specification, compatibility policy, release process and migration guidance.
