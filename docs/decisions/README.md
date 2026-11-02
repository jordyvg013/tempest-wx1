# Architecture decision records

- [0001 — WX-1 is an independent project](0001-independent-project.md)
- [0002 — Byte order and metadata encoding](0002-byte-order-and-metadata.md)
- [0003 — Multiple logical indexes](0003-index-architecture.md)
- [0004 — Integrity, codecs and committed generations](0004-integrity-codecs-and-commit.md)
- [0005 — Python, compute backends and extensible parameter tables](0005-python-compute-and-tables.md)

These records establish design direction, not a frozen v1.0 byte layout. Exact field offsets, page layouts, schemas, registries and recovery algorithms remain subject to the normative specification and independent test vectors.

- [0010 — First metadata-only reader/writer vertical slice](0010-metadata-only-reader-writer.md)
- [0011 — Experimental raw chunk directory and CRC32C](0011-raw-chunk-directory-and-crc32c.md)
- [0012 — Experimental lossless chunk codecs](0012-lossless-chunk-codecs.md)
- [0013 — Streaming I/O, BLAKE3 commit footer and bounded processing](0013-streaming-integrity-and-commit.md)

- [0014 — Durable directory v2 and append-only commit generations](0014-durable-directory-and-append-only-generations.md)
- [0015 — Persistent copy-on-write B+tree indexes](0015-persistent-cow-btree-indexes.md)
- [0016 — Scientific metadata schema and multidimensional array chunks](0016-scientific-model-and-array-chunks.md)
- [0017 — Bounded generation codec pipeline and scientific CLI](0017-bounded-generation-codec-pipeline-and-cli.md)

- [0018 — Inspectable scientific array read planning](0018-scientific-array-read-planning.md)
- [0019 — Reproducible performance benchmark harness](0019-performance-benchmark-harness.md)


## Planned next ADRs

ADR 0005 already sets the high-level Python/compute direction. ADR 0019 is already assigned to the **internal WX-1 Rust performance microbenchmark**; it does not define the separate multi-format comparison suite. Existing ADRs 0001–0019 are immutable and are not renumbered.

The current roadmap reserves these next identifiers (planned records unless a file is present and accepted):

- 0020 — Python API, PyO3 bindings and maturin packaging (specific implementation contract; complements ADR 0005)
- 0021 — Cross-format benchmark suite and shared adapter protocol (WX-1, GRIB2, NetCDF4, Zarr 3)
- 0022 — Appendable scientific datasets
- 0023 — Lazy open and explicit full validation
- 0024 — Lossy quantization with a declared error bound
- 0025 — Parallel staging, merge/commit and asynchronous writer
- 0026 — Non-regular grids and tile topology
- 0027 — Parameter registries and units
- 0028 — Indexed scientific array planner (only after the formal Fase-2 baseline)
- 0029 — Advanced scientific read paths and optional alternate chunk layout
- 0030 — GRIB2/NetCDF/Zarr interoperability
- 0031 — v1.0 robustness, security and migration
- 0032 — Stable C ABI
- 0033 — GPU and multi-device integration

These are planned identifiers, not accepted decisions. Create/review each ADR after its specification draft and before implementation. ADR 0020 and 0021 have proposal drafts in this documentation update; they are **proposed, not accepted or implemented**.

- [0020 — Python API, PyO3 bindings and maturin packaging](0020-python-api-and-pyo3.md) — accepted and implemented (Fase 1).
- [0021 — Cross-format scientific benchmark suite](0021-cross-format-benchmark-suite.md) — accepted and implemented (Fase 2).
- [0022 — Appendable scientific datasets](0022-appendable-scientific-datasets.md) — accepted and implemented (Fase 3).
- [0023 — Lazy open and explicit full validation](0023-lazy-open-and-explicit-validate.md) — accepted and implemented (Fase 3).
- [0024 — Lossy quantization with a declared error bound and ByteShuffle+Zstd](0024-lossy-quantization-and-byte-shuffle.md) — accepted and implemented (Fase 4).
- [0025 — Parallel tile/rank staging, deterministic merge/commit and async writer](0025-parallel-staging-merge-and-async-writer.md) — accepted and implemented (Fase 4).
- [0026 — Non-regular grids, cubed-sphere topology, halos, Arakawa C staggering and domain nesting](0026-non-regular-grids-and-tile-topology.md) — accepted and implemented (Fase 4).
- [0027 — Parameter registries, CF/WMO mappings, provenance and safe units](0027-parameter-registries-and-units.md) — accepted (Fase 4).
- [0028 — Indexed scientific array read planner](0028-indexed-array-read-planner.md) — accepted and implemented (Fase 4).
- [0029 — Advanced scientific read paths (R1–R9) and dual-chunking layouts](0029-advanced-scientific-read-paths.md) — accepted and implemented (Fase 4).
- [0030 — External format interoperability (GRIB2, NetCDF-4, Zarr) and lossless metadata preservation](0030-external-format-interoperability-and-metadata-preservation.md) — accepted and implemented (Fase 4).
- [0031 — Fuzzing, independent test vectors, power-loss resilience, and security hardening](0031-fuzzing-test-vectors-power-loss-and-security.md) — accepted and implemented (Fase 4).
- [0032 — Stable C ABI, cbindgen, and Foreign Function Interface](0032-stable-c-abi-cbindgen-and-ffi.md) — accepted and implemented (Fase 4).
- [0033 — GPU and multi-device integration, non-blocking transfer, and bit-exact restart](0033-gpu-multi-device-integration-and-restart.md) — accepted and implemented (Fase 4).
- [0034 — Expanded scientific chunk and metadata limits](0034-expanded-chunk-and-metadata-limits.md) — accepted and implemented (Fase 2).

