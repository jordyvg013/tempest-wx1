# WX-1 Phase 1 fix2 — xarray backend integration correction (WSL verification pending)

- Corrected `Wx1BackendEntrypoint.open_dataset` to use an explicit xarray-compatible signature (`filename_or_obj`, `drop_variables`) and removed `**kwargs`, which xarray's plugin signature inspection rejects.
- Corrected the Dask backend test to pass `Wx1BackendEntrypoint` (the backend class), not `Wx1BackendEntrypoint()` (an instance); xarray's `engine` API accepts the class.
- Added a regression test that checks the backend signature has no variadic parameters and that xarray can open through the backend class without materializing data.
- User WSL evidence on phase1-fix1: full Python suite 20 passed, 2 failed, 2 skipped. The two failures were exactly the signature-inspection error in the native xarray/Dask path and the invalid backend-instance argument in the Dask test. Test blocks after the full suite were not yet run.
- Authoring environment validation on phase1-fix2: xarray backend tests 5 passed, 1 skipped (Dask absent); full Python suite 15 passed, 10 skipped (native extension, Dask, NetCDF4/Zarr and GRIB2 fixture unavailable); Python compileall passed. These do not replace the user's WSL verification.
- No Rust source, Python native binding, API data layout, WX-1 metadata schema, descriptor, header, footer, index or wire-format bytes changed.
- Phase1-fix2 remains a candidate until the user's WSL rerun confirms the Python suite and the remaining Phase-1 gates.

---

# WX-1 Phase 1 fix1 — Clippy `manual_contains` correction (verification pending)

- Addressed the two concrete Clippy `manual_contains` diagnostics reported in the user's WSL run in `bindings/wx1-python/src/lib.rs`.
- Replaced `shape.iter().any(|length| *length == 0)` and `chunk_shape.iter().any(|length| *length == 0)` with `shape.contains(&0)` and `chunk_shape.contains(&0)`. This is an equivalent zero-element check; no API, error behavior, array layout or WX-1 wire bytes are intentionally changed.
- The preceding user WSL run resolved `libpython3.12.so.1.0` via `LD_LIBRARY_PATH`, passed all 4 `wx1-python-native` unit tests and all 112 workspace tests, but Clippy with `-D warnings` failed on exactly these two diagnostics.
- This fix1 source has not yet been built or tested by the user. The authoring environment has no Cargo/Rust toolchain; rerun formatting, workspace tests and Clippy in WSL before accepting the fix. Python wheel, CPU-only installation, native NumPy roundtrips and optional adapters remain separate open Phase-1 gates.
- No descriptor, header, footer, directory, index, codec, checksum, metadata schema or other wire-format layout changed.

---

# Changelog

## Fase 1 Python prototype — 2026-11-01 (verification pending)

- Added a separate `wx1-python-native` PyO3 crate to the Cargo workspace and maturin project configuration under `python/`.
- Added a CPU-only base package with opt-in NumPy, xarray, Dask, NetCDF4, Zarr and GRIB2/ecCodes extras.
- Added native open/metadata/slice/plan bindings and an initial single-variable, multi-chunk Zstandard writer using the existing bounded reader writer.
- Added NumPy conversion helpers and an xarray lazy-indexing backend. The existing open path still performs full-generation validation; this is not lazy open.
- Added xarray-based external NetCDF, Zarr and GRIB2 reader adapters and local/optional fixture tests. These are not yet full WX-1 import/export adapters.
- Latest Python test run in the authoring environment: 14 passed, 10 skipped; see current status for the exact environment and skipped items. Optional Dask/NetCDF4/Zarr installation was unavailable because package-index DNS could not be resolved. Added xarray reverse/strided slice tests and a native xarray/Dask end-to-end test (not run here because Dask and the native extension are unavailable).
- Native Rust/PyO3 compilation, maturin wheel, native NumPy roundtrip, Dask, NetCDF4, Zarr and GRIB2 fixture tests remain unverified because the authoring environment lacks Cargo/rustc/maturin and the relevant optional libraries/fixtures.
- No WX-1 wire-format, 72-byte generation descriptor, 40-byte legacy descriptor, footer or index layout changes.

## Documentation update — 2026-11-01 (v0.4.3 docs revision)

- Added ADR 0020 proposal and Python API specification draft v0.1; ADR 0005 remains the existing high-level Python/compute architecture.
- Reserved ADR 0021 for the separate cross-format benchmark suite; ADR 0019 remains the internal WX-1 Rust microbenchmark.
- Added post-baseline indexed scientific array planner work to the roadmap (ADR 0028), including unmeasured future scale targets.
- Separated buffered and durable write measurement for W2/W4; documented that the v0.4.3 microbenchmark excludes `fsync`.
- Added W6 end-to-end asynchronous checkpoint-overhead workload.
- Froze pre-measurement cross-format acceptance thresholds in `bench/config/acceptance.yaml`.
- Documentation-only change; no Rust/Python source implementation or wire-format bytes changed, and no tests/benchmarks were run in this documentation update.

## Documentation reconciliation — 2026-11-01

- Added `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md` as the combined development and benchmark brief.
- Added current status, current roadmap and mandatory user CLI handoff instructions.
- Reconciled the verified base to v0.4.3 using the user's supplied WSL test and benchmark output; v0.4.3 remains experimental and the benchmark is preliminary.
- At that earlier documentation snapshot, recorded that Python/PyO3/maturin, NumPy, xarray, Dask and external format adapters were not yet present in the v0.4.3 baseline. The experimental Phase-1 prototype is recorded in the later entry above.
- Preserved ADR 0019 for the existing performance harness and reserved ADR 0020 onward for future decisions (now detailed in the current roadmap).
- Explicitly documented the existing 40-byte legacy chunk descriptor and 72-byte generation descriptor; no wire-format or source-code changes were made.
- Recorded benchmark results and limitations, including that the full multi-format suite has not been run.

## v0.4.3 — reproducible internal performance benchmark baseline (user-verified in WSL)

- Added dependency-free custom Cargo benchmark target `wx1_performance` in `wx1-format`.
- Added isolated Raw, Zstandard and LZ4 workloads over a deterministic 2048×2048 F32 array split into 64 chunks; reports median writer throughput, open/validation time, full-array and window-read throughput, complete-file size ratio and process peak RSS where Linux `/proc` is available.
- Added a 4096-small-chunk planner workload for the current `plan_array_slice` implementation, including a fixed 256×256 selection and its selected-chunk count.
- Added `WX1_BENCH_DIR` to choose the filesystem used by temporary benchmark files.
- Added ADR 0019, the v0.4.3 benchmark protocol and roadmap. No dependencies or wire-format changes.
- Benchmarks have not been executed in this authoring environment; no performance numbers or speedup claims are made. The planner remains linear pending measured WSL results.

## v0.4.2 — scientific read planning and recovery hardening (user-verified in WSL)

- Added the experimental public `ArrayReadPlan` and `ScientificSnapshot::plan_array_slice` API. It validates selection rank/bounds and the 64 MiB output limit, then reports the variable metadata and only intersecting chunk IDs without reading or decompressing payloads.
- Refactored `read_array_slice` to use that same plan, preserving the existing output type/order/shape and selected-chunk-only decode semantics. No file bytes, descriptor fields, codec IDs, checksums or index layouts changed.
- Added integration coverage for a 64×64 U16 variable stored in 256 small 4×4 chunks; a cross-tile selection checks exact four-chunk planning and row-major values.
- Added scientific stored-byte corruption rejection without repair mutation and explicit torn-tail repair regressions.
- Added draft spec `WX1_Scientific_Read_Planning_and_Recovery_Draft_v0.4.2.md`, ADR 0018 and roadmap v0.4.2.
- User WSL verification: all 108 workspace tests passed (77 format unit, 11 scientific arrays, 7 scientific streaming, 12 registry and 1 CLI workflow); formatting, Clippy with `-D warnings`, CLI help, the scientific demo/slice, all three new corruption/repair/planning regressions, compatibility regressions, overwrite guard, invalid-selection rejection, scientific append rejection and generic generation/index checks passed.
- Deferred to v0.4.3 and later: throughput/RAM benchmarking, indexed planning for very large chunk maps, time/vertical/CRS-aware spatial selections, a stable importer API, GRIB2/ecCodes, Python and GPU.

## v0.4.1-fix1 — reader-writer trait import correction (user-verified in WSL)

- Fixed the local I/O trait imports in `write_generation_chunks_and_directory_from_readers`: `std::io::Write` is now in scope for `CountingIntegrityWriter::write_all` and `flush`; unused local `Read` and `Seek` imports were removed.
- No runtime behavior, public API, codec behavior, checksums, footer/index layout, descriptor layout or wire-format bytes were intentionally changed.
- User WSL verification: `cargo fmt --all` and format-check passed; all 105 workspace tests passed (77 format unit, 8 scientific arrays, 7 scientific streaming, 12 registry and 1 CLI workflow); Clippy with `-D warnings` passed. CLI help, scientific demo/inspect/validate/slice, exact F32 slice bytes, no-overwrite protection, invalid-slice rejection, scientific append rejection and generic generation/index checks passed.

## v0.4.1 — bounded generation codec pipeline and scientific CLI (user-verified via fix1)

- Added reader-based generation writer APIs using `ChunkReaderInput` and `ChunkWriteOptions`; the default copy buffer is 256 KiB and the accepted range is 1 byte through 8 MiB.
- Raw and Zstandard payloads are streamed with the configured bounded buffer. LZ4 block encoding remains a single bounded chunk operation (maximum 64 MiB); no LZ4 wire-format change is made.
- Added stored-byte CRC32C and BLAKE3 accounting during write, retaining the existing generation digest, footer v2, B+tree catalog and 72-byte descriptors.
- Added pre-write scientific reference/shape/raw-length checks and streaming Bool8 byte validation. Invalid or short streams do not receive a valid generation footer.
- Added atomic reader-based generation file publication with temporary-file cleanup on error.
- Added CLI `create-scientific-demo`, `validate-scientific` and `read-array-slice`. The demo writes a deterministic 4×4 F32 temperature array, latitude/longitude coordinates and EPSG:4326 mapping; the slice command writes bounded row-major raw payload bytes without datatype or byte-order conversion.
- Added format reader-writer tests and a CLI process-level end-to-end test. These tests and the full CLI workflow passed in the user WSL verification recorded under v0.4.1-fix1.
- No wire-format change: v0.4.x remains header minor 6, metadata schema v2, footer v2, the existing persistent B+tree pages and 72-byte chunk descriptor.
- Deferred parallel codec workers, LZ4 frame/sub-chunk streaming, selection-to-file streaming, benchmarks, GRIB/ecCodes, NetCDF, Zarr, Python and GPU.

## v0.4.0-fix1 — profile regression and Clippy fixes (user verified)

- Corrected the v0.3.9 footer-v2 regression fixture to carry historical header minor 5; the normal v0.4.0 writer correctly emits minor 6. Added an explicit test for the indexed writer's v0.4.0 minor-6 profile.
- Replaced two comparator-based canonical sorts with `sort_by_key` and changed row-coordinate quotient calculation to use checked division, addressing the three initial user-reported Clippy diagnostics without changing encoded ordering or array-read semantics.
- User WSL verification: fmt and fmt-check passed; 77 `wx1-format` unit tests, 8 scientific array integration tests and 12 `wx1-registry` tests passed (97 total); Clippy with `-D warnings` passed. Both profile tests, CLI help, generation creation/inspection, five-index inspection and append to generation 2 also passed.

## v0.4.0 — scientific datamodel and multidimensional chunks (user verified via fix1)
## v0.3.9-fix3 — test-fixture visibility and Clippy cleanup (user verified)

- Moved the shared synthetic v0.3.8 generation fixture into `v038_generation_tests` and exposed it only to the parent test module; v0.3.9 compatibility tests now call it through `super::v038_generation_tests`. This fixes the `E0425` unresolved fixture reference without adding a production API.
- Grouped subtree-validation state, range-query traversal state, B+tree COW writer state and append-generation write inputs into explicit context/plan structs. Range traversal uses the catalogued/validated expected tree level as its recursion bound and continues to enforce `MAX_INDEX_TREE_DEPTH`, the visited-page set and page-count limit.
- Applied the user-reported Clippy fixes: `contains()` for child-zero checks, collapsed chunk-ID validation condition, mutable slices instead of `&mut Vec` where vector mutation is not required, `BTreeMap::entry()` for the mandatory empty chunk-ID index, and reduced argument counts for validation, lookup, COW insertion and append-write helpers.
- No descriptor, directory, footer, index-page, catalog wire layout, public API or on-disk compatibility behavior is intentionally changed.
- User WSL verification: fmt and fmt-check passed; all 87 workspace tests passed (75 `wx1-format`, 12 `wx1-registry`); Clippy with `-D warnings` and CLI help passed. Manual five-index lookups, append to generations 2 and 3, fallback to generation 1, repair with a valid older generation, and rejection of a corrupted sole generation without file mutation all passed.
- Historical user WSL fix2 run: fmt and fmt-check completed without reported errors, but test compilation stopped at `E0425` (`legacy_v038_generation` was not visible from the sibling test module). Clippy also stopped at that compile error and reported 11 lint diagnostics: too-many-arguments, `manual_contains`, `ptr_arg`, `map_entry` and `collapsible_if`. CLI and manual tests were not reached in fix2. Fix3 resolved these findings and its subsequent WSL verification is recorded above.

## v0.3.9-fix2 — footer-version-aware digest regression tests (verification pending)

- Corrected the existing v0.3.9 generation-digest tests to use the footer-v2 digest range at bytes 40..72; bytes 32..40 are the v2 index-catalog pointer.
- Corrected the v0.3.9 generation-hash corruption test to flip the digest field rather than the catalog pointer.
- Added a separate v0.3.8 footer-v1 corruption regression test, which flips the legacy digest at bytes 32..64 and expects `GenerationHashMismatch`.
- Compiled the v0.3.8 footer encoder only for tests, avoiding a production `dead_code` warning without removing the legacy-read compatibility fixture.
- No production wire-format, footer, directory, index-page, descriptor, or public API behavior changed.
- User WSL fix1 run: `cargo fmt --all` and `cargo fmt --all -- --check` completed before the test command; `cargo test --workspace` compiled but reported 74 `wx1-format` tests with 72 passed and 2 failed. Both failures were stale test offsets assuming a 64-byte footer despite the helper creating a v0.3.9 footer v2. Clippy, CLI help and manual tests were not reached because the `&&` chain stopped at the failing test command. A `dead_code` warning for the v0.3.8 test-only footer encoder was also observed and addressed here.

## v0.3.9-fix1 — range traversal level validation (verification pending)

- Fixed the v0.3.9 compile blocker in `collect_index_range`: the decoded internal-page `level` is now explicitly bound and checked against the expected level from its parent/root catalog.
- Range lookup now rejects a leaf at a nonzero expected level and an internal page whose stored level differs from the expected tree level, using `IndexTreeInvalid`.
- Added a regression test that mutates a child internal-page level while recomputing its page BLAKE3 digest; a range lookup on the already-pinned snapshot must reject the structurally invalid tree.
- No wire-format, footer, catalog, descriptor, public API, or on-disk compatibility change.
- User's v0.3.9 attempt was manually interrupted after the real `E0027` compile error; subsequent missing `/tmp` files and `^C` lines are consequences of that interruption, not independent WX-1 test failures. The fix1 build, tests, Clippy and CLI checks remain pending.

## v0.3.9 — persistent copy-on-write B+tree indexes (verification pending)

- Added an experimental header minor-5 generation profile with `WX1G` footer v2 (80 bytes). The footer points to a persistent index catalog; the header's mutable `directory_offset` remains zero.
- Retained the v0.3.8 directory-v2 layout and 72-byte descriptors unchanged.
- Added immutable 4096-byte `WX1B` leaf, internal and catalog pages with independent BLAKE3 page digests. The generation digest covers only the new generation's chunks, directory and newly written index pages/catalog, excluding its footer.
- Added a mandatory chunk-ID B+tree whose key/value mappings must exactly match the visible primary directory, plus optional caller-keyed variable, time ordinal, vertical ordinal, spatial tile and model/provider indexes.
- Added bulk index construction, exact-key and inclusive byte-key range lookup, path-copy append insertion, leaf/internal splits and root splits. Unchanged subtrees may be reused from earlier committed generations; published pages are never overwritten.
- Added page/catalog/tree bounds validation, index-specific errors, directory cross-validation, pinned snapshot lookups, v0.3.8 minor-4/footer-v1 read compatibility and legacy v1/`WX1C` read compatibility.
- Added CLI `append-indexed-demo`, `inspect-indexes` and `index-query`; the generation demo now includes persistent indexes.
- Added tests for both byte orders, exact footer/catalog offsets, empty datasets, 10,000-entry bulk indexes, leaf and internal root splits, COW snapshots, index corruption, fallback, duplicate mappings, invalid chunk references, read-only v0.3.8 compatibility and byte-by-byte append failure.
- Deferred timestamp/calendar semantics, physical vertical coordinates/units, CRS-aware spatial queries, deletion, replacement, compaction, vacuum, index repair, GRIB, NetCDF, Zarr, Python and GPU.
- Historical verification status: the original v0.3.9 candidate was superseded by fix1–fix3; the confirmed v0.3.9-fix3 verification is recorded above.

## v0.3.8-fix1 — Clippy cleanup (user verified)

- Removed seven unused `Read` imports and one unused `Write` import from the generation reader/writer helpers.
- No runtime behavior, public API, directory layout, footer layout, or on-disk compatibility changed.
- User WSL verification: formatting checks passed; all 70 workspace tests passed; Clippy with `-D warnings` passed; CLI help and the supplied generation, recovery, repair, and corruption checks passed.

## v0.3.8 — durable directory and append-only generations (user verified via fix1)

- Added directory v2 with 72-byte descriptors; BLAKE3 is computed over the exact on-disk stored payload bytes (after compression) and CRC32C retains its existing stored-byte coverage.
- Added 64-byte `WX1G` generation footers with generation number, previous-footer offset, directory/root offset and BLAKE3 over only that generation's payload-plus-directory region. The generation-profile header keeps `directory_offset` zero.
- Added append-only generation writing, file-backed data/footer sync ordering, pinned `GenerationSnapshot` reads and backward recovery to the newest fully valid generation.
- Added explicit repair that truncates only the tail after a valid generation; open/inspect remain read-only and append refuses an unrepaired tail.
- Added layer-specific directory/footer/chain/chunk-hash/CRC errors and CLI `create-generation-demo`, `append-generation`, and `repair`; inspection validates all chunks before success output.
- Retained the v1 40-byte directory and legacy `WX1C` footer reader path.
- Documented the single-writer-per-file requirement; concurrent append/repair must be externally serialized because the file API does not acquire a cross-process lock.
- Added tests for both byte orders, v1 compatibility, multiple generations, pinned snapshots, byte-by-byte append failure, torn tails, malformed offsets, duplicate IDs and integrity corruption.
- The user's v0.3.8-fix1 WSL run confirmed 70 tests, fmt, Clippy, CLI and the supplied manual generation/recovery/repair/corruption checks.

## v0.3.7-fix3

- Distinguishes malformed/truncated commit footers (`CommitFooterInvalid`) from BLAKE3 digest mismatches (`CommitHashMismatch`).
- Preserves the two import corrections verified by the v0.3.7-fix2 user run.
- Adds tests for footer magic corruption, digest corruption, and truncated footers.

## v0.3.1 — 2026-10-29

- Added `wx1-registry` crate with validated immutable table packages and CSV importer.
- Added exact external-code resolution including optional centre/subcentre/discipline/category context.
- Added safe explicit-endian primitive decoder to `wx1-format`.
- Added registry and primitive-decoder tests.
- Retained v0.2 architecture and decisions; clarified that no stable binary layout or full reader/writer exists yet.

## v0.2.0

- Independent standalone project scaffold, architecture drafts and ADRs.

## v0.3.1 — registry export and version comparison

- Added deterministic CSV export with RFC-4180 field escaping.
- Added exact-key table version diffs (added, removed, changed) and deterministic table identity listing.
- Added CSV round-trip, diff classification and incompatible-family tests.


## v0.3.2

- Added explicit, provenance-bearing parameter overlays that create new immutable table versions.
- Prevented overlays from adding new external code identities or changing known physical unit dimensions.
- Added tests for base immutability, dimensional mismatch and unknown external codes.

## v0.3.3 — experimental container header

- Added a separate 32-byte experimental WX-1 container header; legacy `WX1X` prefix remains legacy-only.
- Added little-/big-endian header encoding and decoding.
- Added strict validation for major version, header length, flags, reserved bits, metadata bounds and directory offsets.
- Added tests for both byte orders, truncation, malformed fields and overflow-safe range validation.
- Added ADR 0009 and an explicit non-normative header draft.


## 0.3.3-fix1

- Added `Decoder::read_u8`, completing the primitive decoder API used by the experimental container-header parser.
- No on-disk header layout or semantics changed.


## v0.3.4 — metadata-only reader/writer vertical slice

- Added deterministic CBOR metadata schema version 1 with bounded decoding and canonical attribute-key ordering.
- Added metadata-only container read/write APIs using the experimental 32-byte header.
- Added 16 MiB metadata and 100,000-attribute limits; readers reject truncation, trailing bytes, and unsupported directory offsets.
- Added CLI `create-demo` and `inspect` commands.
- Added ADR 0010 and an experimental metadata profile draft.
- Scientific chunks, indexes, checksums, compression, commits, and recovery remain unimplemented.


## v0.3.4-fix1

- Fixed compilation of the CBOR unsigned-integer encoder by naming the `u32`
  maximum as a `u64` constant for the match range pattern.
- Removed redundant `Read` and `Seek` imports from `read_container` to keep
  `clippy -- -D warnings` clean.
- Added CBOR unsigned-integer boundary coverage, including the u32/u64 width transitions.
- Corrected the crate-level status comment to reflect the implemented metadata-only reader/writer.
- No wire-format or metadata behavior changed.


## v0.3.5 — experimental raw chunk storage

- Added a trailing `WX1D` chunk directory with fixed 40-byte descriptors.
- Added raw chunk writing and descriptor validation with duplicate-ID, bounds, overlap, count, and per-chunk size checks.
- Added dependency-free CRC32C (Castagnoli) and checksum verification when reading an individual chunk.
- Added CLI `create-chunk-demo` and `inspect-chunks` workflows.
- Added ADR 0011 and the experimental raw chunk directory draft.
- Raw codec only; compression, sub-chunk streaming, indexes, BLAKE3, commits, and recovery remain unimplemented.

## v0.3.6

- Added experimental Zstandard and LZ4 lossless chunk codecs alongside raw storage.
- CRC32C is verified over stored bytes before decoding.
- Added decoded/stored length limits and exact decompressed-length validation.
- Added `create-compressed-demo` and codec-aware `inspect-chunks` output.
- Chunk processing is bounded to one complete chunk; sub-chunk streaming remains future work.


## v0.3.7 — streaming, BLAKE3 and commit groundwork

- Added reader-based raw/Zstandard chunk writing and bounded streaming decode to writers.
- Added CRC-verified raw range reads and configurable bounded buffers.
- Added configurable input chunk splitting and bounded descriptor-batch enumeration.
- Added BLAKE3 stored-chunk digest helper and optional BLAKE3 commit footer (`WX1C`).
- Added atomic temporary-file publication helpers with file sync and Unix parent-directory sync.
- Added in-memory chunk-ID/offset and caller-keyed scientific lookup indexes.
- Preserved the v0.3.6 40-byte descriptor and directory version 1.
- Explicitly deferred durable B+tree/spatial index pages, per-chunk BLAKE3 fields, append-only multi-generation recovery, LZ4 frame streaming, and internal parallel executor.

## v0.3.7-fix2

Fixed Rust trait imports required by bounded streaming paths, changed raw range reads to borrow the reader rather than move it, and replaced `Read::take` on trait-object chunk inputs with an exact-length bounded-buffer copy helper. No on-disk layout or public API changed.
