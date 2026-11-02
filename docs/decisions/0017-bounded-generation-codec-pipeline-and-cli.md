# ADR 0017 — bounded generation codec pipeline and scientific CLI

- **Status:** Proposed for v0.4.1 implementation; accept after WSL verification.
- **Date:** 2026-10-30

## Context

v0.4.0 establishes scientific schema-v2 metadata and multidimensional array chunk maps. Its generation writer accepts byte slices, so callers must already hold each raw chunk in memory. Earlier WX-1 work added reader-based streaming for the older chunk-container profile, but that path is not connected to the generation profile or scientific metadata validation. The scientific Rust API can read array selections, while the CLI has no create/read-slice workflow.

The roadmap identifies v0.4.1 as the fully bounded codec pipeline. A practical scientific workflow must use that pipeline rather than add a CLI that still stages all source chunks in memory.

## Decision

Add additive reader-based generation writer APIs using `ChunkReaderInput` and a configurable `ChunkWriteOptions`. The new writer preserves the v0.4.x header minor, schema-v2 metadata, footer v2 and B+tree layout. It computes stored-byte CRC32C and BLAKE3 while bytes pass through the writer and incorporates the same stored bytes into the generation digest.

Raw and Zstandard are streamed with a bounded caller-configurable buffer. LZ4 remains block-based and materializes one raw chunk, capped by the existing 64 MiB limit. This limitation is explicit rather than falsely described as streaming.

When scientific metadata is present, the writer validates chunk references and exact raw lengths before writing. Boolean payload bytes are validated as they pass through the stream. The footer is not committed after any validation, source, codec, or I/O failure. A separate atomic file API provides the same temporary-file/sync/rename publication pattern as the existing generation writer.

Add a small native CLI workflow: create a deterministic scientific example using all available codecs, inspect/validate it, and read a bounded array selection to a raw output file. The example is for conformance and usability; it is not a general manifest-import format.

## Alternatives considered

1. **Keep only the v0.4.0 slice API and add a demo CLI around byte-slice writer.** Rejected because it would retain full-chunk input staging and bypass the v0.4.1 bounded-codec roadmap item.
2. **Replace LZ4 block with LZ4 frame.** Rejected because it changes existing codec wire compatibility and requires a separately versioned codec identifier/profile.
3. **Add a general JSON/YAML/TOML manifest importer now.** Deferred because it introduces a second public metadata representation and parser/dependency surface before the storage writer has been verified.
4. **Add Python/xarray or GRIB adapters.** Deferred to their roadmap stages; storage semantics must remain in Rust and provider-specific interpretation must not leak into the core.

## Consequences

- Existing APIs and files remain compatible; the new writer is additive and writes the existing v0.4.x profile.
- Raw/Zstandard ingestion can operate with a configurable bounded buffer instead of staging each chunk.
- LZ4 memory remains bounded but can peak near one full 64 MiB raw chunk plus compressed output.
- Generic writer failures may leave an uncommitted tail; callers needing safe publication should use the atomic file API.
- Scientific schema-v2 files remain one-generation immutable-manifest files.
- CLI users can inspect a scientific dataset and extract typed raw array selections, but no external manifest importer or provider adapter is implied.
