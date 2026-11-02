# WX-1 architecture and wire-format design draft v0.2

- Status: **Draft for review; not a frozen normative byte specification**
- Project checkpoint: 0.2.0
- Scope: storage container, scientific data model, table registry, Python and compute architecture

## 1. Design principles

1. The file format is language-, platform-, host-endian-, CPU-, GPU- and codec-independent.
2. Files may exceed 4 GiB. On-disk offsets and lengths are unsigned 64-bit values, subject to explicit implementation limits and checked arithmetic.
3. Readers support bounded streaming and partial reads. Loading an entire file, variable, time series, or spatial domain into RAM/VRAM is never a requirement.
4. Chunks are independently addressable and independently decodable.
5. Lossless storage is the default. Lossy encoding is explicit, identified, and accompanied by error semantics.
6. Unknown optional metadata and identifiers are preserved where possible. Unknown required features cause a clear unsupported-feature error.
7. A valid committed generation is distinguishable from an incomplete write.
8. Scientific meaning is explicit: units, dimensions, coordinates, temporal/statistical processing, vertical coordinates, missing values, provenance and quality.

## 2. Byte order and primitive values

**Proposed decision:** support both little-endian and big-endian files. Little-endian is the default for newly created files. The file header declares the byte order used by fixed-width numeric fields. Readers must not infer file byte order from the host.

All integer widths, signedness and floating-point representations are explicit. IEEE 754 binary32 and binary64 are the initial floating-point representations. Byte order applies to multi-byte numeric fields and is not a property of opaque byte strings or compressed payloads. Implementations must convert explicitly at boundaries.

A file's byte order is fixed for that file generation; mixed-endian numeric structures are prohibited unless a future feature explicitly defines one.

## 3. Metadata encoding

**Proposed decision:** CBOR using deterministic encoding rules, constrained by a WX-1 metadata schema.

The schema defines required fields, allowed types, limits, semantic constraints and extension namespaces. CBOR maps must not be treated as an unvalidated bag of values. Duplicate keys, excessive nesting, excessive item counts, oversized strings and unsupported semantic versions must be rejected safely.

JSON is an optional diagnostic/export representation, not the canonical on-disk encoding. Unknown extension fields should be preserved by tools that perform metadata-preserving transformations.

## 4. Logical file regions

The following is a logical order, not yet a final byte layout:

1. **Header:** signature, major/minor version, byte-order marker, required/optional feature flags, bounded header fields.
2. **Metadata root:** dataset identity, dimensions, variables, coordinates, grids, time axes, provenance, quality and table references.
3. **Payload chunks:** independently addressable compressed or uncompressed data blocks.
4. **Index pages:** chunk directory and specialized indexes.
5. **Integrity records:** per-chunk checksums and integrity data for metadata/index/commit structures.
6. **Commit/footer records:** generation identity, offsets/lengths, integrity checks and a clear commit marker.

All offsets are relative to the beginning of the file unless a future ADR explicitly defines another basis. Every offset+length calculation must use checked arithmetic and be bounded by the committed file length and configured limits.

## 5. Chunk model

Chunks are the unit of storage, compression, checksum, partial read and compute scheduling. A chunk descriptor records at least:

- stable chunk identity and variable identity;
- dimension coordinates or tile coordinates;
- payload offset and stored length;
- decoded length and explicit datatype;
- codec ID and codec parameters;
- checksum algorithm ID and checksum value;
- optional transform/quantization metadata;
- required feature flags, if any.

Chunks must be independently decodable in the baseline profile. Chunk size is workload-configurable within normative safety limits. Implementations must not allocate based only on an untrusted decoded-length field: validate against file bounds, configured limits and checked arithmetic first.

## 6. Multiple indexes

**Proposed decision:** multiple logical indexes share stable chunk references. A B+tree is the baseline ordered-index structure. The specification may permit alternate encodings if they preserve the same query and safety semantics.

Initial logical indexes:

- variable/parameter index;
- valid-time, reference-time and forecast-step index;
- spatial/grid/tile index;
- vertical-coordinate index;
- model and provider index;
- chunk-location directory.

An implementation may omit optional indexes and scan a bounded directory where permitted, but must advertise available index capabilities. A spatial index may use a specialized structure; it is not forced into the B+tree if another structure is more suitable. Index pages are independently checksummed and loaded on demand. Index caching has explicit memory limits.

## 7. Checksums and codecs

**Proposed direction:** CRC32C per chunk for fast accidental-corruption detection, with BLAKE3 integrity records for stronger content integrity where configured. IDs are stable and registry-managed. Neither checksum is a digital signature or proof of publisher identity.

Codec registry initial proposal:

- `0`: none;
- `1`: Zstandard;
- `2`: LZ4.

Zstandard is the default lossless codec. Codec IDs are never reassigned. Codec parameters are versioned and bounded. Lossy codecs require explicit opt-in, a distinct registered ID and machine-readable error bounds/semantics. CPU-only readers must be able to report unsupported codecs without loading GPU dependencies.

## 8. Footer, generations and crash safety

**Proposed direction:** append-only generations, end-of-generation commit/footer records, and redundant commit records. A reader locates and validates the newest complete generation and may fall back to an earlier valid generation if the newest is incomplete or corrupt.

A writer must write payload and index data before publishing a commit record. Durability requires appropriate flush/sync behavior; guarantees vary by filesystem and storage device and must be documented. For new files, writing to a temporary file and atomically renaming after successful commit is recommended where supported. Updating an existing file must not destroy the previous committed generation before the new generation is committed.

Recovery must never guess missing index or payload data. It reports the last valid generation and any discarded/incomplete tail.

## 9. Versioning and feature compatibility

- Major version: incompatible required structural/semantic changes.
- Minor version: backward-compatible additions under defined rules.
- Required feature flags: unsupported means the reader must refuse full interpretation.
- Optional feature flags: a reader may ignore only when doing so cannot change data meaning.
- Unknown metadata extensions: preserve when possible.
- Table and codec identifiers: immutable within their namespace; versions are explicit.

The exact compatibility matrix and version-negotiation rules require a separate normative specification section before v1.0.

## 10. Scientific data model

The core model includes variables, dimensions, coordinate variables, regular/irregular grids, projections/CRS, time coordinates, forecast reference time, lead time, vertical coordinates, statistical processing, units, missing values, quality flags, provenance and source identifiers.

The model must distinguish instantaneous values from accumulations, averages, maxima/minima and other statistical products. A variable's identity is not merely its display name: parameter identity, units, vertical coordinate, temporal meaning, processing and source-table context all matter.

## 11. Parameter and table registry

WX-1 has a stable internal parameter identity and a versioned, namespaced external-code mapping system. It must support WMO/GRIB tables, KNMI and other provider tables, private research tables and user-defined tables.

Each table package has a namespace, table ID, table version, WX-1 schema version, provenance, license/attribution, optional signature/digest, and entries. Entries can map external codes to canonical parameter identities and carry names, descriptions, units, dimensions, temporal/statistical semantics, vertical-coordinate meaning and deprecation/supersession metadata.

Import formats planned: CSV and JSON in the base Python package; YAML and XLSX via optional dependencies; native WX-1 table packages for deterministic distribution. Import is data-only: no executable code, macros, dynamic imports or scripts. Validation checks schema, duplicates, namespace ownership, units, semantic conflicts, reference integrity and resource limits.

Tables are immutable once published under a version. Changes create a new version. A user may create a private namespace or an explicit overlay/extension that references a parent table; it must never silently overwrite upstream definitions. Colliding codes are resolved by namespace and version, not by guessing.

WX-1 files should embed the exact table definitions (or sufficient immutable, content-addressed snapshots) required to interpret their data. External registries may accelerate lookup but must not be the only source of essential scientific meaning. Third-party table content retains its original license; redistribution is allowed only where that license permits it.

## 12. Python package architecture

Planned distribution: `wx1` on Python Package Index, subject to package-name availability and release readiness. Rust core bindings use PyO3 and maturin. The package is layered:

- `wx1`: open/create, metadata, variables, streaming and selection API;
- `wx1.tables`: import, validate, register, resolve, diff and export table packages;
- `wx1.compute`: execution policy and chunk scheduler;
- `wx1.integrations`: optional NumPy, xarray, Dask and MetPy adapters;
- `wx1.backends`: CPU and optional GPU backends.

The pure file-format specification remains independent of Python and Rust. A Python implementation must not be required to read WX-1 files.

Target Python API (illustrative, not implemented):

```python
import wx1

ds = wx1.open("forecast.wx1")
field = ds["air_temperature_2m"].sel(
    time="2026-10-29T12:00:00Z",
    bbox=(3.0, 50.5, 7.5, 54.0),
)
for block in field.iter_chunks():
    consume(block.to_numpy())
```

## 13. MetPy and scientific function extensions

WX-1 should integrate with MetPy rather than fork or silently duplicate its scientific algorithms. The integration layer converts WX-1 metadata and arrays into MetPy-compatible units, coordinates and objects, then exposes supported MetPy calculations as optional functions.

A function registry allows built-in, MetPy-backed and user/plugin-provided operations. Each function declares input variables, units, dimensions, coordinate requirements, output metadata, missing-data behavior, numerical precision, CPU/GPU support, chunk/halo requirements and whether it is safe to execute independently per chunk.

Functions that require neighboring cells (for example spatial derivatives) declare halo/overlap requirements. Functions requiring global reductions declare multi-pass behavior. Results preserve provenance: function name/version, parameters, input identities and software/backend versions.

MetPy remains an optional dependency. WX-1 core and file reading must work without MetPy. Users may add their own functions through a documented plugin API; plugins are Python code and therefore trusted code, unlike imported parameter-table files.

## 14. CPU/GPU execution and 6 GB VRAM target

Compute backends are optional and separate from the wire format. CPU-only operation is complete and does not install CUDA, ROCm, JAX or CuPy.

Execution policies:

- `cpu`: force CPU;
- `gpu`: require a selected supported GPU backend, otherwise return a clear error;
- `auto`: select a supported backend based on operation support, memory budget and measured/estimated transfer cost;
- `hybrid`: schedule eligible chunks on CPU and GPU concurrently, subject to explicit CPU RAM and GPU VRAM budgets.

GPU support is capability-based. JAX is a candidate backend, not a hard dependency or guaranteed universal solution. CuPy may be another optional backend for NumPy-like kernels. The first GPU milestone should select one backend after compatibility and packaging tests on the target OS/GPU stack.

Memory rules:

1. Never move an entire dataset to GPU by default.
2. Read/decompress bounded chunks, then transfer only the current tile/batch.
3. Maintain separate configurable budgets for host RAM, pinned host memory and device VRAM.
4. Reserve VRAM for the driver/runtime and other processes; never treat advertised VRAM as entirely available.
5. Account for input, output, temporary arrays, halos and workspace before scheduling a GPU operation.
6. Reuse buffers where safe and release them under memory pressure.
7. Fall back to CPU in `auto` mode when a kernel is unsupported or the tile does not fit; explicit `gpu` mode reports failure rather than silently switching.
8. In `hybrid`, partition independent chunks; do not execute the same chunk twice or race writes to the same output region.
9. Dask/lazy execution is supported through bounded task graphs and backpressure; avoid creating a graph with one task per tiny element.
10. A single operation may stream across multiple batches even when its full input/output is larger than VRAM.

A 6 GB GPU is a design target, not a performance guarantee. Actual chunk sizes and concurrency depend on dtype, algorithm workspace, driver overhead and available memory. Defaults must be conservative and configurable. Tests must include simulated low-memory conditions and real hardware benchmarks before claims are made.

## 15. Interoperability

Planned import/export adapters: GRIB1/2, NetCDF, HDF5 and Zarr. Adapters must report semantic losses, unsupported local tables, unit conversions and coordinate transformations. They must preserve original source codes and table versions wherever possible.

## 16. Security and resource limits

All files, tables and metadata are untrusted. The normative specification will define maximum header/metadata sizes, nesting depth, chunk counts, index page sizes, string lengths, decoded chunk size and supported integer ranges. Implementations may expose stricter configurable limits. Validate offsets and lengths before allocation, decompression or seeking. Decompression bombs, cyclic/invalid index references, duplicate IDs, invalid table overlays and integer overflow must be tested.

## 17. Decisions still required before freezing v1 bytes

- Exact header and footer field layout, alignment and signature.
- Exact B+tree page layout and index-key ordering.
- Exact CBOR schema and canonicalization profile.
- Registry governance and permanent ID allocation process.
- Codec parameter constraints and BLAKE3 coverage profile.
- Commit recovery scan limits and filesystem durability profile.
- Python API stability and supported Python/OS/architecture matrix.
- First GPU backend and supported hardware/software matrix.

No stable file-format compatibility promise is made until these are specified, implemented and tested with independent test vectors.
