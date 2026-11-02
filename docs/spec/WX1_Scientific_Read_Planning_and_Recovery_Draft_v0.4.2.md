# WX-1 scientific read planning and recovery — draft v0.4.2

**Status:** experimental draft. This increment does not define a stable external API or change any on-disk bytes.

## 1. Scope

v0.4.2 adds a public, inspectable plan for a bounded multidimensional array selection and adds regression coverage for scientific generation integrity, many-small-chunk arrays, and explicit repair of trailing uncommitted bytes.

The release retains header minor 6, metadata schema v2, the current generation footer v2, persistent index page/catalog layout, primary directory, 72-byte chunk descriptors, codec IDs, CRC32C coverage, and BLAKE3 coverage.

## 2. Array read plan

`ArrayReadPlan` is an additive Rust type. `ScientificSnapshot::plan_array_slice(variable_path, selection)`:

- resolves an existing variable path;
- requires `start` and `count` ranks to equal the variable rank;
- uses checked addition for selection end coordinates and rejects any selection outside its declared dimensions;
- checks the resulting output size against the existing 64 MiB `MAX_ARRAY_READ_BYTES` limit before allocating output;
- returns the variable path, datatype, payload byte order, requested start coordinate and shape, output byte count, and only the chunk IDs whose multidimensional extents intersect the selection;
- preserves model chunk order in the returned ID list and does not read, decompress, or expose physical file offsets.

A zero-element selection yields an empty chunk list. A scalar selection has rank zero and selects its scalar chunk. The planner uses the already validated immutable `ScientificSnapshot`; it does not accept arbitrary unvalidated metadata.

`ScientificSnapshot::read_array_slice` uses `plan_array_slice` as its selection-validation and chunk-selection path. It still decodes each selected compressed chunk as one bounded chunk and returns one contiguous row-major output buffer. The selected output limit remains 64 MiB.

The first implementation scans the variable's chunk references linearly. The API is a planning boundary, not yet an indexed spatial query engine or a performance claim. A more efficient plan based on persistent logical indexes requires explicit index-key semantics and benchmarks.

## 3. Integrity and repair regressions

The test suite adds the following deterministic checks:

1. A scientific generation whose stored chunk payload is modified must fail open with its chunk CRC32C error; it must not be accepted as a valid scientific snapshot.
2. A valid scientific generation with trailing uncommitted bytes remains readable as its last fully committed generation.
3. `repair_generation_file` explicitly truncates only the trailing bytes after the valid footer; the committed scientific generation remains readable after repair. If stored scientific payload bytes are corrupt, repair fails and leaves the file byte-for-byte unchanged.
4. A 64×64 U16 variable split into 256 4×4 chunks can plan a 3×3 selection crossing four chunk boundaries. The plan must identify exactly the four intersecting chunks, and the selected row-major payload must match the expected values.

These are regression tests, not exhaustive power-loss, filesystem-durability or fuzz-testing proofs. Scientific schema-v2 manifests remain single-generation and immutable; append/update is still rejected.

## 4. Limits and known gaps

- No wire-format changes or compatibility migration are introduced.
- No LZ4 sub-chunk or partial decompression is implemented; selected compressed chunks are still decoded in full.
- The plan currently scans all chunk references for the selected variable; very large-map planning benchmarks and an indexed planner remain open.
- No throughput, latency, allocation-count or peak-RAM benchmark results are claimed by this draft.
- Calendar arithmetic, temporal range queries, physical vertical-level selection, CRS transformations, longitude wrapping queries and spatial window selection are not implemented.
- No stable importer API, generic manifest parser, GRIB2/ecCodes, NetCDF, Zarr, Python, xarray, Dask or GPU adapter is added.
- Fuzzing, exhaustive crash/power-loss testing and independent implementations remain future work.

## 5. Compatibility

`ArrayReadPlan` and `plan_array_slice` are additive Rust APIs. Existing array model bytes and existing file layouts remain unchanged. Existing `read_array_slice` output datatype, byte order, selection shape and 64 MiB output limit remain unchanged; it uses the new planner internally to identify the same intersecting chunks.
