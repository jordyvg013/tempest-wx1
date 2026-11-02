# ADR 0031 — Fuzzing, Independent Test Vectors, Power-Loss Resilience, Security Hardening, and Cross-Platform Validation for v1.0

## Status
Accepted (Fase 4, Onderdeel 8)

## Context
As WX-1 approaches its v1.0 specification freeze, the format engine must transition from experimental prototype to an enterprise-grade, high-reliability storage engine capable of serving mission-critical meteorological forecasting pipelines (such as ECMWF, KNMI, EUMETSAT, and DWD).

In operational numerical weather prediction (NWP) environments:
1. **Untrusted & Corrupt Inputs**: Files may arrive over network streams, satellite downlinks, or third-party pipelines truncated, corrupted by bit-flips, or maliciously crafted. A parser crash or memory exhaustion (OOM) halts operational workflows.
2. **Power Loss & Abnormal Termination**: Nodes in High-Performance Computing (HPC) clusters or edge observation stations may lose power, experience kernel panics, or receive `SIGKILL` mid-write. The storage format must guarantee that partial writes never corrupt previously committed state.
3. **Security Boundaries & Denial of Service**: Malicious or malformed inputs must not cause buffer overflows, unbounded memory allocations, path traversal attacks via variable paths, or decompression bombs (zip/zstd bombs).
4. **Cross-Platform & Architecture Independence**: Data generated on x86_64 Linux clusters must be byte-for-byte readable and verifiable on ARM64 macOS laptops, Windows workstations, and big-endian network systems.
5. **Deterministic Golden Vectors**: Future implementations in C, Fortran, Python, and Rust require an immutable set of golden binary vectors with known bit layouts and deliberate corruption patterns.

## Decision

### 1. Fuzzing Infrastructure & Robustness Testing
- Implement automated mutation and structured fuzzing harnesses covering:
  - Superblock and generation header decoding.
  - CBOR manifest parsing and recursion limits.
  - Chunk descriptor directory unpacking.
  - Codec decompressors (Raw, Zstandard, LZ4) with corrupted payloads.
  - Scientific array slicing and indexing engine with random query windows.
- Any crash, panic (in non-test code), out-of-bounds access, or unhandled assertion is treated as a critical defect. All recoverable errors must return `Result<T, Wx1Error>`.

### 2. Independent Conformance Test Vectors (Golden Vectors)
- Establish a versioned directory of independent test vectors (`golden_vectors/`):
  1. `valid_scalar_2d_zstd.wx1`: Standard 2D surface field with Zstd compression and complete metadata.
  2. `valid_volume_3d_lz4.wx1`: Multi-level isobaric dataset with LZ4 compression.
  3. `valid_multigen_append.wx1`: Dataset with multiple append generations and CoW indexes.
  4. `valid_special_floats.wx1`: IEEE-754 special values (NaN, +inf, -inf, -0.0, subnormals).
  5. `corrupt_super_magic.wx1`: Invalid container magic bytes.
  6. `corrupt_footer_crc.wx1`: Damaged commit footer CRC32C.
  7. `corrupt_chunk_blake3.wx1`: Single-bit flip in chunk payload triggering BLAKE3 integrity failure.
  8. `corrupt_truncated_payload.wx1`: File truncated mid-chunk.
  9. `corrupt_unsupported_codec.wx1`: Chunk claiming unknown codec ID (reserved for future ADR).
  10. `malicious_path_traversal.wx1`: Variable name containing relative traversal sequences (`../../etc/passwd`).
  11. `malicious_decompression_bomb.wx1`: Chunk with 1000x expansion ratio exceeding security threshold.

### 3. Power-Loss Resilience & Crash Consistency
- **Atomic Three-Phase Generation Commit**:
  - Phase 1: Payload chunks are written sequentially to disk.
  - Phase 2: Index pages and metadata manifest are written.
  - Phase 3: The generation commit footer (containing generation sequence, timestamp, BLAKE3 manifest hash, and footer CRC32C) is flushed to persistent storage via `fsync` (`durable_commit`).
- **Crash Recovery Model**:
  - If power is lost before the generation commit footer is fully flushed and verified, the file remains in its previous valid generation state.
  - The `repair()` function inspects the container, detects any uncommitted trailing bytes or partial headers, validates the generation chain backwards to the highest valid commit footer, and truncates the orphaned tail safely.

### 4. Security Hardening & Untrusted Input Defenses
- **Allocation Limits**:
  - Maximum chunk uncompressed size: **64 MiB** (`MAX_CHUNK_BYTES`).
  - Maximum metadata manifest size: **32 MiB** (`MAX_METADATA_BYTES`).
  - Maximum scientific array rank: **64** (`MAX_SCIENTIFIC_RANK`).
  - Maximum total dataset dimensions: **4,096** (`MAX_SCIENTIFIC_DIMENSIONS`).
  - Maximum chunk references per generation: **1,048,576** (`MAX_CHUNK_REFS` / `MAX_CHUNKS`).
  - Decompression bomb guard: uncompressed chunk length capped at `MAX_CHUNK_BYTES` (64 MiB) with strict bounded streaming `.take(limit)` and pre-allocation clamp, rejecting declared sizes $> 64\text{ MiB}$ prior to decompression.
  - CBOR parser memory & depth: Non-recursive tabular schema design with structural depth bounded to $\le 3$ levels, bounded entity counts (`MAX_SCIENTIFIC_VARIABLES`, `MAX_SCIENTIFIC_GROUPS`, etc.), preventing nested recursion attacks.
- **Variable Path Sanitization**:
  - Variable paths must follow Unix-style canonical hierarchy: `/group/subgroup/variable`.
  - Disallow relative components (`.`, `..`), control characters, null bytes, backslashes (`\`), or empty segments (`//`).
- **Memory Safety**:
  - Maintain strict `#![forbid(unsafe_code)]` across all core Rust crates (`wx1-format`, `wx1-registry`, `wx1-cli`).

### 5. Cross-Platform & Endianness Guarantees
- All header integers, chunk descriptors, index keys, and CBOR integers are explicitly Little-Endian on disk.
- Array data byte order is explicitly recorded in `ArrayMetadata` (`ByteOrder::LittleEndian` or `ByteOrder::BigEndian`). When reading on different host architectures, byte swapping is performed transparently and without data loss.

## Consequences
- **Positive**:
  - WX-1 containers become robust against malicious tampering, bit-rot, and unexpected hardware failures.
  - Guarantees data survival in operational HPC cluster checkpointing and streaming radar environments.
  - Establishes a verifiable test harness for independent third-party implementations.
- **Negative / Trade-offs**:
  - Decompression bomb checks require measuring output byte counts against input byte counts, adding minor tracking overhead.
  - Fsync durability adds latency on writes (addressed in ADR 0025 via asynchronous batch staging).
