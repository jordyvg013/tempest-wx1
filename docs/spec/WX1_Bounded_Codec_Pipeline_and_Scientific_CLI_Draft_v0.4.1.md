# WX-1 bounded codec pipeline and scientific CLI — draft v0.4.1

**Status:** experimental implementation profile; not a stable interoperability contract.

## 1. Scope

v0.4.1 extends the v0.4.0 scientific data model with a bounded streaming writer for generation-profile containers and a first native CLI workflow. It does not change the v0.4.0 metadata schema-v2 encoding, header minor 6, footer v2, 72-byte physical chunk descriptor, 4096-byte B+tree pages or index catalog.

The release provides:

- generation-profile chunk writing from caller-owned `Read` streams with explicit raw lengths;
- configurable bounded I/O buffers;
- streaming Raw and Zstandard writes;
- bounded LZ4 block writes, with one raw chunk materialized (at most 64 MiB);
- per-chunk CRC32C and BLAKE3 over stored bytes, plus the existing generation BLAKE3 coverage;
- scientific Boolean-byte validation while streaming;
- atomic file publication for the new reader-based writer;
- a synthetic scientific CLI create/inspect/read-slice/validate workflow.

The CLI's `create-scientific-demo` command is a deterministic example, not a general-purpose external manifest importer. A declarative manifest format and GRIB/NetCDF/Zarr adapters remain future work.

## 2. Bounded writer API

The format crate exposes `ChunkWriteOptions` with a configurable `buffer_size`. The default is 256 KiB; valid values are 1 through 8 MiB. The existing on-disk maximum raw chunk length remains 64 MiB and the maximum stored chunk length remains 65 MiB.

`ChunkReaderInput` supplies a unique chunk ID, a `Read` source, declared uncompressed `raw_len`, and a lossless `ChunkCodec`. The writer consumes exactly `raw_len` bytes from each source. It does not read beyond that declared length; the caller remains responsible for ensuring that the stream represents one complete chunk. Transient `Interrupted` read errors are retried; other I/O errors and an early EOF abort the write.

The public write operations are:

- `write_generation_container_from_readers`;
- `write_generation_container_from_readers_with_indexes`;
- `write_generation_container_from_readers_atomic`;
- `write_generation_container_from_readers_atomic_with_indexes`.

The generic writer requires an empty destination and may leave an uncommitted tail if an input or output fails. The file-based atomic writer writes to a sibling temporary file, flushes and syncs the data before publishing the footer, syncs the completed file, then renames it into place. Platform-specific durability limitations remain as documented for the existing atomic writer.

### 2.1 Codec memory behavior

| Codec | Write behavior | Memory bound |
|---|---|---|
| Raw | Copies exactly the declared raw length | Configured stream buffer |
| Zstandard | Streams input through a Zstandard encoder | Configured stream buffer plus codec-internal bounded state |
| LZ4 block | Reads and compresses one complete chunk | One raw chunk up to 64 MiB plus codec output |

The LZ4 block encoding is unchanged from previous profiles. v0.4.1 does not claim sub-chunk LZ4 streaming.

### 2.2 Integrity and commit ordering

CRC32C and BLAKE3 for each chunk cover the exact stored payload bytes. The generation digest continues to cover the concatenated stored payloads, encoded generation directory and newly written index pages/catalog in the existing order. The 80-byte footer v2 is written only after the payloads, directory and indexes have completed and the pre-footer flush/sync hook succeeds.

No checksum/hash domain or footer field is changed. A failed generic stream write does not publish a valid footer; the atomic file API removes its temporary file on failure.

### 2.3 Scientific model validation

Before writing, the writer validates dataset structure, chunk-ID uniqueness, every scientific chunk reference, and the expected raw byte length derived from checked element count times datatype width. Boolean arrays are additionally checked during streaming; every byte must be `0` or `1`. An invalid Boolean value aborts the write before the generation footer is committed.

## 3. CLI workflow

### 3.1 Create a deterministic scientific example

`wx1 create-scientific-demo <file> <dataset-id>` writes a 4 × 4 `F32` temperature field at `/temperature`, split into four 2 × 2 chunks. It includes latitude/longitude dimension roles, units and standard-name attributes, an EPSG:4326 grid mapping and synthetic provenance. The four chunks exercise Zstandard, Raw and LZ4 codecs.

The demo is written through the reader-based generation writer and its atomic file publication path.

### 3.2 Inspect and validate

`wx1 inspect <file>` retains its existing behavior: it opens and validates the complete committed generation, decodes all visible chunks, and prints the scientific model summary when present.

`wx1 validate-scientific <file>` requires schema-v2 scientific metadata, validates the scientific chunk map against the committed primary directory, decodes every visible chunk and reports success only after all checks pass.

### 3.3 Read an array slice

`wx1 read-array-slice <file> <variable-path> <start-csv> <count-csv> <output.raw>` reads the requested row-major selection and writes its unconverted payload bytes to the output file. `start-csv` and `count-csv` contain one unsigned integer per dimension, separated by commas. The output uses the variable's declared datatype and payload byte order. It is not a NumPy file and carries no header; the CLI prints datatype, byte order, shape and byte count.

The existing 64 MiB selection-output limit applies. Only intersecting chunks are decoded, but each selected compressed chunk is still decoded as one bounded chunk. The output path must not be the input WX-1 file.

## 4. Compatibility

- Header minor remains 6 for v0.4.0/v0.4.1 generation files.
- Metadata schema-v1 bytes without scientific metadata remain unchanged.
- Metadata schema-v2 scientific model bytes remain unchanged from v0.4.0.
- Footer v2, index pages/catalog, primary directory and 72-byte chunk descriptor remain unchanged.
- Readers retain v0.3.8 (minor 4), v0.3.9 (minor 5) and v0.4.x (minor 6) support.
- Existing slice APIs and existing writers remain source-compatible; the new reader-based APIs are additive.
- Scientific schema-v2 files remain single-generation and immutable at the manifest level. This release does not add scientific append/update.

## 5. Limits and non-goals

- No general user-supplied JSON/YAML/TOML manifest parser.
- No GRIB1/GRIB2, ecCodes, NetCDF, Zarr, Python/PyO3, xarray, GPU or provider-specific adapters.
- No change to existing compression level defaults or codec identifiers.
- No LZ4 frame conversion or sub-chunk decompression.
- No implicit numeric conversion, byte-order conversion, missing-value masking, unit conversion or calendar arithmetic during CLI reads.
- The CLI reads a selection into the existing bounded `ScientificArray` result before writing it; this is not a zero-copy or fully streaming output selection API.
- Fuzzing, exhaustive power-loss tests, independent implementations and performance claims remain future work.

## 6. Conformance-oriented tests

The v0.4.1 test suite covers streaming roundtrip for Raw/Zstandard/LZ4, little- and big-endian container headers, configurable buffer and chunk boundaries, exact declared input lengths (including unread trailing source bytes), retry of interrupted source reads, scientific boolean validation, raw-length/model mismatch before writing, truncated streams without a committed footer, atomic temporary-file cleanup, all existing v0.4.0 array semantics, old-profile regression tests and CLI demo/read/validation plus no-overwrite/error-path tests.
