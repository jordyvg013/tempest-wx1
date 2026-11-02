# WX-1 Lazy Open and Explicit Full Validation Specification Draft (v0.1)

- **Date:** 2026-11-01
- **Status:** Draft / Specification for ADR 0023 implementation (Fase 3)
- **Authority:** `WX1_ROADMAP_CURRENT.md`, `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md`

---

## 1. Context and Problem Statement

In WX-1 v0.4.3, opening a generation (`open_generation_snapshot`) or a scientific dataset (`open_scientific_snapshot`) performs an eager, exhaustive verification of the entire file:
1. It parses all directory chunk descriptors across all generations in the commit chain.
2. For *every single chunk descriptor*, it seeks to the chunk in the file and recalculates the CRC32C checksum of the chunk payload (`verify_generation_chunk`).
3. For indexed profiles (minor 5), it traverses all B+tree index pages across all generations and validates their cryptographic hashes.
4. For every generation in the chain, it streams and hashes the entire byte span from `generation_start` to `footer_offset` with BLAKE3 (`hash_file_range`).
5. For scientific datasets, it iterates over all variables and all chunk descriptors to verify dimension shapes and length products.

### Impact on Scalability
While this eager validation provides immediate file-wide integrity verification on small datasets (e.g. 1 chunk, 0.75 ms), its time complexity is $O(\text{file\_size} + N_{\text{chunks}})$.
As measured in the Phase 2b microbenchmarks on ext4:
- 4,096 chunks: open validation takes 0.755 ms.
- 65,536 chunks: open validation takes 19.066 ms.
- 1,048,576 chunks ($2^{20}$): open validation takes **278.105 ms** (reading 32 MiB of metadata alone, excluding gigabytes of chunk payloads).
On a 100 GB operational NWP dataset with millions of chunks, eager open takes tens of seconds to minutes. Consequently, acceptance threshold **R1** (open time at 100 GB at most $2\times$ that at 1 GB) cannot be satisfied with eager open.

---

## 2. Normative Specification

### 2.1 Lazy Open Guarantee
`open_generation_snapshot_lazy` (and its scientific counterpart `open_scientific_snapshot_lazy`) MUST operate in bounded $O(1)$ time relative to total stored chunk payload bytes:
1. **Container Header Verification:** Validate 64-byte container header (magic `WX1\0`, version, byte order, limits).
2. **Terminal Footer Lookup:** Seek to the end of the file and inspect the latest generation footer (`WX1G` magic, 80 bytes for minor 5, 72 bytes for minor 4). If trailing uncommitted bytes are present, perform a bounded reverse scan (max candidate limit) to locate the latest valid committed footer.
3. **Generation Chain & Bounds Check:** Verify that `directory_offset`, `index_catalog_offset`, and `prev_footer_offset` fall within valid, strictly monotonic file bounds.
4. **Metadata Decoding:** Parse container metadata (preamble or metadata chunk) and scientific dataset schema.
5. **Deferred Verification:**
   - Chunk payloads MUST NOT be read or hashed during lazy open.
   - Per-chunk CRC32C verification MUST be deferred to chunk read time (`read_generation_chunk_payload` / `read_array_slice`), where payload CRC32C is already checked before decompression or delivery to caller.
   - Generation-wide BLAKE3 digest verification MUST NOT be performed during lazy open.
   - B+tree index page subtrees MUST NOT be recursively traversed during lazy open; only catalog root page validity and page region bounds are verified.

### 2.2 Explicit Full Validation Guarantee
To preserve the archival integrity and recovery guarantees of WX-1, an explicit validation routine MUST be provided:
- Rust API:
  - `GenerationSnapshot::validate_full(&self, reader: &mut R) -> Result<(), ChunkIoError>`
  - `ScientificSnapshot::validate_full(&self, reader: &mut R) -> Result<(), ChunkIoError>`
- Python API:
  - `dataset.validate()` / `dataset.validate_full()`
- CLI:
  - `wx1 validate --full <path>` / `wx1 check --deep <path>`

Full validation executes the exhaustive $O(N)$ verification:
1. Every chunk descriptor in the directory is verified against its payload CRC32C.
2. Every B+tree index page is checked against its page-level BLAKE3 digest.
3. Every generation in the footer chain is verified against the committed generation BLAKE3 digest in its footer.

### 2.3 Compatibility and Safety
- **Wire Format Contract:** No byte-level changes to the container header, generation descriptor, chunk preamble, or generation footer. Lazy open is an engine-level access-path optimization and is 100% wire-compatible with existing WX-1 v0.3.8, v0.3.9, and v0.4.x files.
- **Fail-Safe Integrity:** Any chunk corrupted on disk will still fail immediately with `ChunkIoError::Crc32cMismatch` when accessed during `read_array` or `read_array_slice`. Lazy open does not weaken data read integrity.
