# WX-1 scientific datamodel and multidimensional chunks — draft v0.4.0

**Status:** experimental implementation profile. This draft is not a stable interoperability contract.

## 1. Scope

v0.4.0 adds a scientific schema over the existing append-only generation container. It does not change the 72-byte directory-v2 chunk descriptor, the 4096-byte B+tree pages, the index catalog or the 80-byte `WX1G` footer v2. New v0.4.0 generation files use container header minor version **6**. The reader retains support for generation profile minor 4 (v0.3.8/footer v1) and minor 5 (v0.3.9/footer v2).

The scientific schema describes datasets, groups, dimensions, variables, coordinates, grid mappings and provenance. The payload remains in ordinary independently checksummed/compressed WX-1 chunks. No provider-specific GRIB/NetCDF/Zarr interpretation is performed by the storage core.

## 2. Metadata schema version 2

Metadata without a scientific model is serialized exactly as the existing schema-v1 three-entry deterministic-CBOR map. This preserves old metadata bytes and fixtures.

When a scientific model is present, the outer metadata object is a definite-length CBOR map with four entries, in this exact order:

| Integer key | Value | Meaning |
|---:|---|---|
| 0 | unsigned integer `2` | Metadata schema version |
| 1 | UTF-8 text | Non-empty dataset identifier |
| 2 | map of UTF-8 text to UTF-8 text | Existing extensible dataset attributes |
| 3 | byte string | Deterministic-CBOR scientific model, schema version 1 |

Top-level integer keys must appear in ascending order. Dataset attribute keys retain the schema-v1 deterministic text-key order. The byte string is bounded by the existing 16 MiB metadata limit, including the outer object. The nested model must be fully consumed, valid and canonical; trailing bytes and unknown schema versions are rejected.

The nested scientific model is a seven-element definite-length CBOR array:

1. unsigned schema version (`1`);
2. groups;
3. dimensions;
4. variables;
5. coordinates;
6. grid mappings;
7. provenance records.

Each collection is a definite-length array. Identity-bearing collections are canonicalized by path/name before encoding; provenance order is retained. Maps use deterministic CBOR text-key ordering. Every tuple below is a definite-length array with exactly the listed members.

### 2.1 Groups

`[absolute_path, attributes]`. `/` is the implicit root group and is not stored. Non-root groups use slash-separated valid identifiers; a non-root parent must exist. Group attributes are typed values as defined in §2.7.

### 2.2 Dimensions

`[name, length, role]`. Dimension names are dataset-global identifiers. `length` is an unsigned 64-bit element count; zero-length dimensions are valid. Role codes:

| Code | Role |
|---:|---|
| 0 | other |
| 1 | time |
| 2 | forecast/reference time |
| 3 | ensemble/member |
| 4 | vertical |
| 5 | latitude |
| 6 | longitude |
| 7 | height |
| 8 | soil layer |

### 2.3 Variables

`[name, group, datatype, payload_byte_order, dimensions, chunk_shape, fill_value, missing_value, attributes, chunks, grid_mapping]`.

A variable path is `/name` in the root group or `group_path/name` otherwise. Variable names are unique within a group. Dimension names are ordered and unique within a variable. The order of dimensions is the in-memory/on-disk array axis order; the final dimension varies fastest (row-major/C order).

Datatype codes and element widths:

| Code | Datatype | Bytes |
|---:|---|---:|
| 0 | unsigned 8-bit integer | 1 |
| 1 | signed 8-bit integer | 1 |
| 2 | unsigned 16-bit integer | 2 |
| 3 | signed 16-bit integer | 2 |
| 4 | unsigned 32-bit integer | 4 |
| 5 | signed 32-bit integer | 4 |
| 6 | unsigned 64-bit integer | 8 |
| 7 | signed 64-bit integer | 8 |
| 8 | IEEE-754 binary32 | 4 |
| 9 | IEEE-754 binary64 | 8 |
| 10 | boolean byte (`0` or `1`) | 1 |

Payload byte-order codes are `0` little-endian and `1` big-endian. This applies to each numeric array's payload independently of the container-header byte order. One-byte integer and boolean values are order-independent. Floating-point values use IEEE-754 bit representation; no NaN normalization or implicit conversion is performed.

`fill_value` and `missing_value` are either CBOR null or `[datatype_code, unsigned_raw_bits]`. The datatype must match the variable. Signed scalar bits use their two's-complement 64-bit representation; floating-point variants preserve their exact IEEE bits. A fill value is the value used to initialize/unwritten storage by a producer; a missing value is the producer-declared sentinel for semantically missing samples. They are distinct fields and are not assumed equal. v0.4.0 stores and returns these semantics but does not mask or transform values during reads.

The `attributes` map supports tagged pairs `[tag, value]`: 0 text, 1 signed integer, 2 unsigned integer, 3 binary64 raw bits, 4 boolean, 5 byte string. Conventional keys include `units`, `standard_name`, `long_name`, `scale_factor`, `add_offset` and other user/provider metadata. Numeric float attributes retain their raw bits. No implicit unit conversion or standard-name resolution is performed.

A chunk reference is `[chunk_id, origin, shape]`. `origin` and `shape` have exactly one element per variable dimension. `chunk_shape` is the nominal chunk extent per axis. Chunk origins must be aligned to `chunk_shape`; edge chunk extents are clipped to the remaining dimension length. A chunk reference maps exactly one existing primary-directory chunk ID. A chunk ID may be referenced only once in the scientific model.

v0.4.0 requires dense, non-overlapping, complete chunk-grid coverage. Every expected grid cell appears exactly once. Sparse arrays and implicit fill-only regions are deferred. A scalar has rank zero and exactly one chunk with empty origin/shape arrays. A variable with any zero-length dimension has no chunks.

### 2.4 Coordinates

A coordinate is `[name, variable_path, axis, time, vertical, longitude_convention]`. Its value array is the referenced variable; coordinate data is not duplicated in metadata. Axis codes: 0 other, 1 time, 2 forecast/reference time, 3 ensemble, 4 vertical, 5 latitude, 6 longitude, 7 height.

Time and forecast/reference-time axes require a `time` tuple:

`[calendar, units, epoch, timezone, precision]`.

Calendar codes: 0 Gregorian, 1 proleptic Gregorian, 2 Julian, 3 no-leap/365-day, 4 all-leap/366-day, 5 360-day, 6 UTC. Precision codes: 0 day, 1 hour, 2 minute, 3 second, 4 millisecond, 5 microsecond, 6 nanosecond. `units`, `epoch` and `timezone` are preserved as explicit text. v0.4.0 does not parse or normalize calendar arithmetic; it does not conflate forecast reference time with valid time.

Vertical and height axes require a `vertical` tuple `[kind, units, positive]`. Kind codes: 0 pressure, 1 geometric height, 2 geopotential height, 3 altitude, 4 depth, 5 hybrid, 6 soil layer, 7 other. Positive-direction codes: 0 up, 1 down. Units are explicit text; no conversion or physical ordering is inferred.

Longitude axes require a convention code: 0 `[-180, 180]`, 1 `[0, 360]`, 2 unwrapped. Other axes must not carry a longitude convention.

### 2.5 Grid mappings / CRS

A grid mapping is `[name, mapping_name, crs_wkt, epsg_code, axis_order, attributes]`. At least one of WKT or EPSG identifier must be present. `axis_order` is an explicit ordered list of axis labels. The mapping is referenced by name from variables. The core preserves the supplied CRS and attributes; v0.4.0 does not transform coordinates, validate arbitrary WKT, or expose CRS-aware spatial queries.

### 2.6 Provenance

A provenance record is `[source, provider, product, institution, license, history, reference_time]`; all fields except source are nullable text. Records retain caller order. The core does not infer a provider, license or source from parameter codes or payload bytes.

### 2.7 Attribute values

Attribute values are two-element arrays `[tag, value]`, with tags 0 text, 1 signed integer, 2 unsigned integer, 3 binary64 raw bits, 4 boolean and 5 byte string. Unsupported tags, non-canonical maps, invalid UTF-8 and excessive lengths are rejected. Unknown attribute names are retained as values; unknown schema fields are not silently ignored.

## 3. Validation and safety limits

- Metadata including the nested model: at most 16 MiB.
- At most 64 dimensions per variable.
- At most 4,096 groups, dimensions and grid mappings; at most 100,000 variables and coordinates; at most 4,096 provenance records.
- At most 1,000,000 chunk references in the complete model (global across all variables).
- Chunk IDs are unique across all variable mappings and must resolve through the validated primary directory.
- Each chunk's `raw_len` must equal `product(chunk.shape) * datatype_width`, with checked arithmetic.
- The full variable dimension product, grid counts and all slice boundaries use checked arithmetic.
- Boolean payload bytes are restricted to `0` and `1`.
- Array slice materialization is capped at 64 MiB. Every touched chunk is integrity-checked and decoded through the existing snapshot API; codecs still materialize one complete chunk (maximum 64 MiB).
- A slice must have the same rank as the variable, and `start + count` must be within every dimension.
- File open validates the schema against the complete committed descriptor set before returning a generation snapshot.

## 4. Rust API and read behavior

The model is built with `ScientificDataset`, `Group`, `Dimension`, `Variable`, `Coordinate`, `GridMapping` and `Provenance`. Attach it using `Metadata::with_scientific_model`. Write a generation using the existing `write_generation_container` or atomic indexed writer. Chunk IDs in each variable's `chunks` list must match the supplied `EncodedChunkInput` values and decoded byte lengths.

Open with `open_scientific_snapshot`. `ScientificSnapshot::read_array_slice` takes a variable path and an `ArraySelection { start, count }`. It reads only the chunks intersecting that selection, verifies them through the existing CRC32C/BLAKE3 and codec path, then copies the requested row-major region into a contiguous output `ScientificArray`. It does not load unrelated chunks or the complete variable. However, each selected compressed chunk is decoded in full; arbitrary sub-chunk decompression is not implemented.

Returned array bytes retain the variable's declared payload byte order. Callers must decode numeric values using `ScientificArray.byte_order`; the core does not silently convert to host endianness.

## 5. Versioning, compatibility and append behavior

v0.4.0 writes container header minor 6 with the existing 80-byte footer v2 and index-page profile. Metadata schema v1 is used unchanged when no scientific model is attached; schema v2 is used when one is present. The reader supports v0.3.8 header minor 4 and v0.3.9 header minor 5, in addition to minor 6.

Older v0.3.9 readers reject minor 6 rather than misinterpreting schema-v2 scientific metadata. v0.4.0 readers can read v0.3.8 and v0.3.9 generation files. Scientific metadata is stored in the immutable preamble. Updating variable definitions or chunk mappings by append would require rewriting that preamble, so append is deliberately rejected for schema-v2 scientific files in this release. A schema-v2 file must contain exactly one committed generation; readers reject a multi-generation chain. A trailing incomplete/corrupt append tail may still be repaired back to the one valid initial generation. Rewrite to a new file is required to alter a scientific model.

## 6. Explicit non-goals

No GRIB1/GRIB2/eccodes, NetCDF, Zarr, xarray, Python/PyO3, registry binding, GPU, lossy transforms, string arrays, complex numbers, sparse arrays, in-place scientific metadata updates, append of new array tiles, unit conversion, calendar arithmetic, CRS transformations, spatial range queries, compression streaming within a chunk, or stable v1.0 interoperability is included.
