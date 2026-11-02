# ADR 0016: Scientific metadata schema and multidimensional array chunks

- Status: Accepted for experimental v0.4.0 implementation
- Date: 2026-10-30

## Context

v0.3.9 provides durable append-only generations, a validated primary chunk directory and persistent copy-on-write B+tree indexes. Its secondary index keys intentionally remain opaque: the format has no normative calendar, vertical-coordinate, array-shape, data-type, CRS, fill-value or provenance semantics. Extending the 72-byte physical descriptor would mix scientific meaning with the storage locator and would make future array layouts difficult to evolve.

The v0.4.0 roadmap calls for datasets, groups, variables, dimensions, coordinates, typed numeric values, missing/fill semantics, per-dimension chunk shapes and partial array reads.

## Decision

1. Keep the directory-v2 72-byte chunk descriptor, B+tree pages, index catalog and 80-byte footer v2 unchanged.
2. Add container header minor 6. Continue reading minor 4 (v0.3.8) and minor 5 (v0.3.9); new v0.4.0 generation files use minor 6. An older reader must reject the newer minor version.
3. Preserve metadata schema v1 byte-for-byte when no scientific model is present. When present, use outer metadata schema v2: the existing dataset ID and string attributes plus a byte string containing a separately versioned deterministic-CBOR scientific model.
4. Store the scientific model in the immutable container preamble. Keep scientific metadata separate from chunk payload and the physical chunk descriptor.
5. Represent groups by absolute paths; dimensions are dataset-global; variables belong to groups and refer to ordered named dimensions.
6. Represent coordinates as references to ordinary coordinate variables. Store time/calendar, vertical kind/units/direction and longitude convention explicitly. Preserve CRS/grid mapping and provenance text without inferring semantics from provider codes.
7. Support numeric integer types, IEEE binary32/binary64 and boolean bytes. Keep payload byte order per variable and independent of container-header byte order. Floating scalar sentinels preserve raw IEEE bits.
8. Require dense, complete, non-overlapping chunk-grid coverage. Every chunk reference resolves to one existing primary-directory chunk ID, and each chunk's raw length equals the checked product of its shape and element width. Scalars use one rank-zero chunk; zero-length dimensions use no chunks.
9. Read a requested array selection by finding intersecting chunk references, reading/verifying only those chunks through `GenerationSnapshot`, and copying the selected intersection into row-major output. Cap each materialized array selection at 64 MiB and retain the existing per-chunk 64 MiB decode bound.
10. Reject append for scientific schema-v2 files because the preamble's dataset manifest is immutable and v0.4.0 does not yet define transactional manifest replacement/update semantics. A schema-v2 file must have exactly one committed generation; readers reject a multi-generation chain. Repair may remove only an invalid trailing tail and retain its single valid initial generation. Existing generic v0.3.9 index-profile append behavior remains available for files without the scientific model.
11. Unknown attribute names are retained. Unknown schema versions, tags and structural fields are rejected; this schema version does not claim forward-preservation of unknown structural fields.

## Alternatives considered

- **Expand the physical chunk descriptor:** rejected because chunk offsets, stored/raw lengths, codec and integrity remain physical concerns; scientific schema can evolve separately.
- **Store the scientific manifest as an ordinary user chunk:** rejected because it would need a reserved ID or special descriptor semantics, risk index pollution and complicate validation before a generation snapshot can be trusted.
- **Put all scientific metadata into the existing three-field schema v1 attributes map:** rejected because string-only attributes cannot express typed dimensions, datatypes, coordinate semantics or chunk maps without a fragile private convention.
- **Use JSON or add serde/CBOR dependencies:** rejected for this increment to avoid a new runtime dependency and to retain the project's existing deterministic-CBOR implementation. The nested schema uses definite-length CBOR arrays and tagged typed attributes.
- **Allow sparse variables and implicit fill regions:** deferred. Dense coverage gives deterministic, simple validation and avoids ambiguous unwritten/missing semantics in the first array implementation.
- **Permit appending array chunks without updating the manifest:** rejected because the dataset schema would not reference new tiles and the visible array coverage could become inconsistent.

## Consequences

- Existing v0.3.8 and v0.3.9 files retain read compatibility and v1 metadata bytes remain unchanged for files without scientific models.
- v0.4.0 files are not readable by older readers that do not support header minor 6; this is an intentional version-gated incompatibility.
- The model can preserve provider-specific/unknown metadata attributes, but the core does not interpret them or perform unit/calendar/CRS transformations.
- Read selections avoid unrelated chunks and whole-variable materialization, but each touched compressed chunk is still fully decoded.
- The preamble is immutable; changing dimensions, variables or chunk mappings requires writing a new file. Appendable schema updates and mutable metadata catalogs are future work.
- This remains experimental. Independent wire-format vectors, fuzzing, larger multi-chunk tests, performance tests and crash/power-loss testing remain outstanding.
