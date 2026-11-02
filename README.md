# WX-1 — Open scientific and meteorological storage

## Current roadmap and development status

- [`WX1_ROADMAP_CURRENT.md`](WX1_ROADMAP_CURRENT.md) — current phase order and gates.
- [`WX1_CURRENT_STATUS.md`](WX1_CURRENT_STATUS.md) — verified tests, measured benchmarks and limitations.
- [`WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md`](WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md) — normative development/benchmark task.
- `crates/wx1-format/benches/wx1_performance.rs` / ADR 0019 — internal Rust microbenchmark.
- `bench/` / ADR 0021 — separate planned cross-format suite.


WX-1 is an independent, open-source, language-independent storage format and storage engine for large scientific datasets. Meteorology is a primary workload; climate, radar, satellite, oceanographic and other multidimensional scientific data are also in scope.

> **Status: v0.4.3 is user-verified in WSL as an experimental development baseline.** The workspace run passed 108 tests (77 format unit tests, 11 scientific-array integration tests, 7 scientific-streaming integration tests, 12 registry tests and 1 CLI workflow test); formatting, Clippy with `-D warnings`, CLI/help regressions, benchmark-target build and manual regressions passed. The user also ran the internal synthetic benchmark on `/tmp` and `/mnt/d`; results and limits are recorded in `WX1_CURRENT_STATUS.md`. This is not yet the formal multi-format benchmark suite and not a stable interoperability contract.
>
> **Python integration status: experimental Phase-1 prototype, not release-verified.** In the user WSL environment the PyO3 native crate compiled, 4 native Rust unit tests passed and all 112 workspace tests passed after setting `LD_LIBRARY_PATH` to the active Conda `lib` directory. The first complete Python suite on phase1-fix1 reported 20 passed, 2 failed and 2 skipped: two xarray backend integration errors were identified (variadic backend signature and backend instance passed instead of backend class). `phase1-fix2` corrects those two issues and awaits WSL verification. maturin wheel/CPU-only installation and native NumPy, Dask, NetCDF4, Zarr and GRIB2 interoperability remain open gates. See `WX1_CURRENT_STATUS.md` for exact evidence and limitations.

## v0.4.0 — scientific data model (verified via v0.4.1-fix1)

- `wx1-format`: schema-v2 deterministic-CBOR scientific metadata, while metadata without a scientific model remains encoded as the existing schema-v1 profile.
- Scientific objects: groups, dimensions and dimension roles, typed variables, coordinates, time/calendar/epoch/timezone/precision metadata, vertical coordinate kind/units/direction, longitude convention, CRS/grid mappings, typed attributes and provenance records.
- Numeric array types: signed/unsigned 8/16/32/64-bit integers, IEEE-754 binary32/binary64 and boolean bytes. Payload byte order is stored per variable, independent of container byte order. Floating sentinel values retain raw IEEE bits.
- Multidimensional chunk maps: rank 0 through 64, row-major axis order, explicit origin/shape, aligned chunk grids, edge chunks, dense non-overlapping complete coverage, scalar and zero-length-dimension semantics.
- Scientific writer validation: every mapped chunk ID must exist in the supplied input set, chunk raw length must equal checked element-count × datatype width, and boolean payload bytes must be 0 or 1. The open path repeats manifest-to-primary-directory validation before returning a generation snapshot.
- `ScientificSnapshot::read_array_slice`: reads only chunks intersecting the requested region, verifies them using the existing chunk CRC32C/BLAKE3 and codec pipeline, then returns a contiguous row-major array with its declared datatype and byte order.
- Container header minor 6 for v0.4.0, retaining footer v2 and the 72-byte physical descriptor. Readers continue to support generation profile minor 4 (v0.3.8) and minor 5 (v0.3.9).
- CLI `inspect` prints a scientific schema summary and variable/dimension descriptions when schema-v2 metadata is present.
- Specification draft, ADR 0016, tests, roadmap, architecture notes and changelog updated for the implemented behavior and limitations.

## v0.4.2 — scientific read planning and recovery hardening (user-verified in WSL)

- Adds `ArrayReadPlan` and `ScientificSnapshot::plan_array_slice` as an experimental additive Rust API. The plan validates selection rank/bounds and the existing 64 MiB output limit, then lists only intersecting chunk IDs without reading payloads.
- `read_array_slice` now uses the same plan. Existing row-major bytes, declared datatype and payload byte order are preserved; selected compressed chunks are still decoded in full.
- Adds deterministic regression coverage for a 64×64 U16 array split into 256 4×4 chunks, stored-byte corruption rejection and explicit repair of trailing uncommitted bytes.
- No wire-format changes: header minor 6, metadata schema v2, footer/index structures, descriptors, codec IDs, CRC32C and BLAKE3 coverage remain unchanged.
- User WSL verification: all 108 workspace tests passed, formatting and Clippy with `-D warnings` passed, and the CLI, scientific data/slice, corrupt-payload, explicit-repair, overwrite protection, invalid-selection, append-rejection and generic index regressions passed.
- Throughput/RAM benchmarks, indexed planning for very large variables, richer time/vertical/CRS-aware selections and a stable importer contract remain open work. No GRIB2/ecCodes import is included.

## v0.4.3 — internal performance baseline (user-verified in WSL)

- Adds `cargo bench -p wx1-format --bench wx1_performance`, a dependency-free harness for Raw, Zstandard and LZ4 writer/read comparisons over a 2048×2048 F32 synthetic field with 64 medium chunks.
- Reports median writer throughput, open/validation time, complete-array and 256×256 window-read throughput, complete-file/raw-array ratio and per-case peak RSS where Linux `/proc` is available.
- Adds a separate 4096-small-chunk workload for the current linear `plan_array_slice` path, reporting planning latency, selected chunk count and window-read throughput.
- Benchmark files default to the OS temporary directory; `WX1_BENCH_DIR` can select another filesystem. Each workload runs in an isolated child process so Linux peak RSS is per case.
- The write benchmark excludes `fsync`; its ratio includes metadata/index/footer overhead; synthetic data is not observational meteorology. There are no performance claims before real WSL measurements.
- The current linear planner and full selected-chunk decompression remain unchanged pending benchmark evidence. No new dependencies, public API behavior or wire-format bytes are changed.

## v0.4.1-fix1 — compile import correction (user-verified in WSL)

- Fixed the reader-based generation writer module imports: `Write` is now in scope for `write_all` and `flush`; unused local `Read` and `Seek` imports were removed. No behavior, public API or wire-format change.
- The initial v0.4.1 build error was limited to a missing `Write` trait import and unused local imports; fix1 corrects those imports without changing behavior or wire bytes.
- User WSL verification: formatting and format-check passed; all 105 workspace tests passed (77 format unit, 8 scientific arrays, 7 scientific streaming, 12 registry and 1 CLI workflow); Clippy with `-D warnings` passed. CLI help, the scientific create/inspect/validate/slice flow, exact 2×2 F32 slice bytes, no-overwrite protection, invalid-selection rejection, scientific append rejection and generic generation/index regression checks passed.

## v0.4.1 — bounded generation writer and scientific CLI (verified via fix1)

- Additive reader-based generation writer APIs accept `ChunkReaderInput` and `ChunkWriteOptions` (default 256 KiB; range 1 byte to 8 MiB).
- Raw and Zstandard input streams use a bounded buffer. LZ4 retains the existing block codec and materializes one raw chunk (maximum 64 MiB); it is not sub-chunk streaming.
- CRC32C and per-chunk BLAKE3 are calculated over exact stored bytes as they are written; generation digest and footer/index commit layout remain unchanged.
- Scientific chunk references and exact raw lengths are validated before writing. Bool8 bytes are checked during stream ingestion.
- Atomic file APIs publish through a sibling temporary file, sync and rename; failed writes remove the temporary file.
- CLI `create-scientific-demo` creates a deterministic 4×4 F32 temperature field and latitude/longitude coordinates. `validate-scientific` validates the complete scientific dataset. `read-array-slice` writes a selected row-major raw payload with explicit datatype/byte-order output.
- CLI array output remains capped at 64 MiB and selected compressed chunks are decoded in full. The demo is not a general external manifest importer.
- Specification draft, ADR 0017, format streaming tests and CLI end-to-end tests are included; the fix1 verification above covers this functionality.

## Not implemented / limitations

- No stable/release-verified Python/PyO3 interface, provider registry binding, GPU, string arrays, complex types, lossy transforms or complete metadata-preserving external import/export adapters are included. Experimental Python reader-adapter and binding prototypes are present in the current development branch; see `WX1_CURRENT_STATUS.md`.
- v0.4.0 supports dense arrays only. Sparse chunk coverage, implicit fill-only regions, append/update of a scientific manifest, deletion, compaction and vacuum are not implemented.
- The scientific metadata preamble is immutable. Appending to a schema-v2 scientific file is rejected; write a new file to change its variable definitions or chunk mapping. Generic v0.3.9 index-profile append behavior remains available for files without a scientific model.
- Calendar strings and numeric time values are preserved, not normalized or arithmetically interpreted. Forecast reference time and valid time remain separate coordinate concepts; no CF validation is claimed.
- Vertical units and levels, CRS/WKT/EPSG data and axis order are preserved but are not converted, transformed or spatially queried. Unknown attribute names are retained; unknown structural schema fields are rejected.
- Array selections are capped at 64 MiB. Only intersecting chunks are read, but each selected compressed chunk is decoded in full (maximum 64 MiB); sub-chunk codec seeking/streaming is not implemented.
- v0.4.1 Raw/Zstandard ingestion uses a configured 1-byte–8-MiB buffer, while LZ4 block ingestion materializes one raw chunk (maximum 64 MiB). Parallel codec workers/queues, LZ4 frame streaming and selection-to-file streaming are not implemented.
- `create-scientific-demo` is a deterministic synthetic example, not a general JSON/YAML/TOML scientific-manifest importer. Scientific schema-v2 files remain immutable at the metadata-manifest level and cannot be appended.
- Opening a generation still validates the full committed chain and all stored chunks/indexes, so open cost is proportional to the committed file size. No stable v1.0 interoperability claim is made.
- Fuzzing, independent cross-implementation wire vectors, the full multi-format benchmark suite, exhaustive power-loss testing and Python/NumPy/xarray interoperability remain future work. The internal synthetic v0.4.3 benchmark has been run by the user; see `WX1_CURRENT_STATUS.md`.

## Build and test

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p wx1-cli -- --help
cargo bench -p wx1-format --bench wx1_performance --no-run
cargo bench -p wx1-format --bench wx1_performance
```

Targeted integration coverage is in `crates/wx1-format/tests/scientific_arrays.rs`, `crates/wx1-format/tests/scientific_streaming.rs` and `crates/wx1-cli/tests/scientific_workflow.rs`. The v0.4.2 additions and the v0.4.3 Rust/benchmark-target workflows were verified by the user in WSL. The internal benchmark results are preliminary and documented in `WX1_CURRENT_STATUS.md`; the formal multi-format suite has not been run.

## Design principles retained

- Little-endian is the default for container/header and newly written metadata; payload byte order is explicit per scientific variable.
- The primary directory remains authoritative for chunk locations, physical lengths, codecs and integrity. Scientific metadata maps variable regions to stable chunk IDs only.
- CRC32C and BLAKE3 continue to protect stored payload bytes; the generation footer and index pages are unchanged from v0.3.9.
- Chunk and array materialization remain bounded. Raw/Zstandard generation ingestion is bounded by `ChunkWriteOptions`; LZ4 block writes and selected compressed-chunk reads materialize one bounded chunk. CPU functionality has no GPU dependency.
- Scientific metadata and provenance do not cause provider-specific parameter codes to be inferred or silently rewritten.

## Open source

MIT licensed. See `CONTRIBUTING.md` and `AGENTS.md`.

## v0.3.3-fix1

Bugfix: added the missing bounds-checked `Decoder::read_u8` primitive required by the experimental container-header decoder. This fixes compilation errors in the v0.3.3 header implementation.


## v0.3.4

The first metadata-only container path is implemented. See `docs/spec/WX1_Metadata_Profile_Draft_v0.3.4.md`; this does not yet store meteorological arrays or chunks.


## v0.3.4-fix1

Compile-compatibility fix for the deterministic CBOR encoder: the `u32` upper
bound is now a named `u64` constant usable in a Rust match range pattern.
Removed redundant local `Read` and `Seek` imports from the bounded container
reader. No on-disk bytes, metadata semantics, public APIs, or format limits
changed.


## v0.3.5

Experimental raw chunk directory and selective chunk reads with CRC32C validation. See `docs/spec/WX1_Raw_Chunk_Directory_Draft_v0.3.5.md` and ADR 0011. Raw/uncompressed codec only; no indexes, compression, commit protocol, or recovery.


## v0.3.6

Experimental per-chunk lossless compression: codec 0 raw, codec 1 Zstandard, codec 2 LZ4 block. CRC32C covers stored bytes and is checked before decoding. Decoded chunks are capped at 64 MiB and one complete chunk is processed at a time. See ADR 0012 and `docs/spec/WX1_Chunk_Codecs_Draft_v0.3.6.md`. This is bounded chunk processing, not sub-chunk streaming.


## v0.3.7

See `docs/spec/WX1_Streaming_and_Commit_Draft_v0.3.7.md` and ADR 0013. Raw/Zstandard input and output streaming, raw range reads, configurable bounded buffers, descriptor batch enumeration, BLAKE3 commit footer, atomic publication and in-memory index preparation are implemented experimentally. LZ4 block operations still buffer one chunk; durable specialized indexes and append-only recovery are not yet implemented.


## Current development documents

- `WX1_CURRENT_STATUS.md` — actuele, bewijsgebaseerde implementatie-/teststatus en benchmarkresultaten.
- `WX1_ROADMAP_CURRENT.md` — leidende fasering en gates.
- `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md` — gecombineerde ontwikkel- en benchmarkopdracht.
- `WX1_USER_CLI_TEST_INSTRUCTIONS.md` — vaste regels voor complete testhandoffs.
- `docs/WX1_Development_and_Benchmark_Analysis_2026-11-01.md` — inhoudelijke analyse en verduidelijkingen.


## Python package (experimental)

The experimental PyO3/maturin package lives in `python/` and `bindings/wx1-python/`. From `python/`, install with `python -m pip install '.[numpy,xarray]'`. Dask, NetCDF4, Zarr and GRIB2/ecCodes integrations are optional extras. The current Python layer is a prototype; see `WX1_CURRENT_STATUS.md` for verification gaps and documented limitations.
