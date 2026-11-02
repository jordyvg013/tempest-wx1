# WX-1 Python API and packaging — specification draft v0.1

**Status:** proposal for Fase 1; experimental source prototype present, native build/wheel/integration verification pending.
**Related ADRs:** 0005 (architecture direction), 0020 (specific PyO3/package decision, proposed).

## 1. Package layers

- Rust crates remain the source of truth for file parsing, integrity, generation handling, codecs and chunk reads/writes.
- `wx1` is the planned Python distribution, built with PyO3 and maturin.
- CPU-only install is the mandatory baseline.
- NumPy, xarray, Dask, NetCDF, Zarr and ecCodes integrations are optional features/extras and cannot be required to import/open/validate a WX-1 dataset in the minimal CPU-only package.
- JAX/GPU integration is deferred until CPU correctness and API stability.

## 2. Proposed capabilities (names/signatures not frozen)

1. Open/close a dataset/file and expose explicit metadata/validation state.
2. Read a variable/selection into a NumPy-compatible array while preserving dtype, byte order and shape.
3. Write supported arrays through the bounded Rust writer with explicit profile and codec settings.
4. Provide xarray lazy access that does not eagerly materialize the full dataset.
5. Provide Dask chunk iteration/task graphs with bounded memory and deterministic resource cleanup.
6. Provide optional local-fixture adapters for NetCDF4, Zarr v2/v3 and GRIB2/ecCodes.

The current prototype method names are documented in the implementation section below. They remain experimental and are not stable API guarantees or CLI commands.

## 3. Buffer and concurrency contract

- Define buffer ownership, mutability, lifetime and close behavior.
- Measure and report all copies between Python, NumPy, Rust and (later) device memory.
- Do not claim zero-copy without evidence from pointer/lifetime tests and a representative benchmark.
- Define GIL release boundaries and Rust thread-safety before enabling concurrent operations.
- Respect the current single-writer/serialization rules until a separate parallel-writer design is implemented.

## 4. Required tests and phase gate

- Build wheel with maturin in a clean CPU-only environment.
- Install and import with no GPU runtime/libraries installed.
- NumPy write/read roundtrip: dtype, byte order, shape, values, bitwise equality where lossless.
- xarray lazy open/read: instrument or otherwise assert that opening does not read/materialize every data chunk.
- Dask chunked read and reduction: correctness, chunk boundaries, bounded memory and clean close.
- NetCDF4/Zarr/GRIB2 local fixtures: roundtrip, coordinates, units, time/calendar, vertical axes, CRS/grid mapping, missing values, packing and metadata-preservation checks.
- Rust tests, Python tests, wheel build and CPU-only install are reported separately.

## 5. Compatibility

Bindings must call supported Rust APIs and must not alter the wire format. The 72-byte generation descriptor and the separate 40-byte chunk descriptor remain unchanged unless a future wire-format specification, compatibility analysis, tests, golden fixtures and ADR explicitly authorize a change.

## 6. Not in this draft

GPU buffers, DLPack device transfers, async GPU writer, parallel multi-rank commit, lossy output, stable public API versioning and a universal array protocol are deferred to later reviewed specifications.


## Prototype-implementatie in de Fase-1 werkbranch

De werkbranch bevat nu `bindings/wx1-python` (PyO3) en `python/wx1`. De eerste native API biedt `open_file`, `Wx1File.variables/dimensions/attributes`, `plan_array_slice`, `read_array_slice` en `write_array`. De Python convenience layer bouwt NumPy-arrays uit de ongewijzigde payloadbytes en biedt een xarray lazy-indexing backend. De reader laat het GIL los tijdens slice-I/O.

`write_array` maakt een nieuw bestand met één root-level variabele, meerdere Zstandard-chunks en tekstuele variabele-attributen; het weigert impliciet overschrijven via de bestaande atomische Rust-writer. De huidige wrapper maakt eerst een C-contiguous NumPy-kopie en `tobytes()` buffer. Er is dus geen zero-copy claim. Coördinaten, groepen, grid mappings, provenance, multi-variable write, streaming vanuit Dask en een appendwriter zijn niet in dit prototype opgenomen.

De bestaande `open_scientific_snapshot` wordt gebruikt; deze valideert de generatie/chunks volledig. De xarray arraydata wordt lazy gematerialiseerd, maar dit is **geen lazy open**. Volledige bestandvalidatie en lazy open blijven aparte API-garanties en worden later in ADR 0023 behandeld.

NetCDF, Zarr en GRIB2-adapters zijn op dit moment xarray-gebaseerde **read adapters** voor externe formaten. Ze voeren nog geen volledige import/export naar WX-1 uit en mogen niet als metadata-preserverende roundtrip-adapters worden geclaimd.

Native build/NumPy roundtrip, Dask, NetCDF4, Zarr en GRIB2 fixture-integratie blijven verificatiegates. Zie `WX1_CURRENT_STATUS.md` voor de exacte teststatus.
