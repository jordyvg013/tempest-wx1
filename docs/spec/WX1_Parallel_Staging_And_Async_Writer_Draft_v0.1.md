# WX-1 Parallel Tile/Rank Staging, Deterministic Merge, and Asynchronous Writer Specification (Draft v0.1)

**Status:** Draft v0.1  
**Target Release:** v0.4.x (Phase 4, Part 3)  
**Related ADRs:** ADR 0022 (Appendable Scientific Datasets), ADR 0024 (Lossy Quantization & Byte-Shuffle), ADR 0025 (Parallel Staging, Merge, and Async Writer)

---

## 1. Scope and Objectives

Numerical Weather Prediction (NWP) and climate models execute across multiple domain tiles, threads, or MPI/device ranks and emit high-volume state checkpoints (`restart`) and diagnostic fields (`output`) at regular timestep intervals.

This specification defines:
1. **Parallel Tile/Rank Chunk Staging (`StagedChunk`)**: Workers independently quantize, byte-shuffle, compress (`Raw`, `Lz4`, `Zstandard`, `ZstdByteShuffle2/4/8`), and compute `CRC32C` and `BLAKE3` digests in parallel without touching shared directory, B+tree index, or file state.
2. **Deterministic Merge and Commit**: Staged chunks arriving in arbitrary completion order from $W$ workers are canonically sorted by `chunk_id` before sequential payload emission, directory construction, B+tree page emission, and generation footer commit. Output bytes are bit-for-bit identical regardless of worker count $W \in \{1, 2, 4, 8, 12, \dots\}$.
3. **Bounded Asynchronous Writer with Backpressure (`AsyncScientificWriter`)**: A background commit engine fed by a bounded queue (`max_pending_generations` and `max_queued_bytes`) so NWP timestep integration overlaps with disk I/O while bounding host RAM and recording exact I/O stall/backpressure telemetry (`W5`, `W6`, `D4`).

---

## 2. Wire-Format Invariance

ADR 0025 introduces **zero changes** to the on-disk `.wx1` container wire format:
- All committed generations conform to Container Header `minor = 6`, Directory v2, Persistent B+Tree Index Page v1 (`WX1B`), and Generation Footer v2 (`WX1G`).
- Any single-threaded WX-1 reader can open, slice, and validate containers produced by parallel staging or the asynchronous writer.

---

## 3. Parallel Staging Protocol

### 3.1 Independent Worker Execution
Given $C$ logical chunks for a variable or timestep and $W \ge 1$ worker threads:
- Even when a domain topology has a fixed number of top-level tiles (e.g., 6 cubed-sphere faces), each tile may be subdivided into multiple chunks so $W > 6$ workers (e.g., 8 or 12 workers) operate on disjoint chunk subsets without contention.
- Each worker processes its assigned chunks in isolation and produces a `StagedChunk`:
  - `id: u64` — unique chunk identifier;
  - `raw_len: u64` — uncompressed byte length ($\le 64\text{ MiB}$);
  - `stored_bytes: Vec<u8>` — compressed payload ($\le \text{MAX\_STORED\_CHUNK\_BYTES}$);
  - `crc32c: u32` — Castagnoli CRC32C of `stored_bytes`;
  - `blake3: [u8; 32]` — BLAKE3 digest of `stored_bytes`;
  - `codec: ChunkCodec` — lossless codec applied to the chunk;
  - `index_entries: Vec<PersistentIndexEntry>` — optional secondary-index entries associated with this chunk.

### 3.2 Deterministic Merge and Commit Ordering
1. **Canonical Sort**: The commit coordinator sorts all staged chunks by ascending `id`. Duplicate `id` values are rejected before mutating the destination file.
2. **Sequential Payload Write**: Pre-compressed `stored_bytes` are written sequentially in canonical `id` order, recording each chunk's physical file `offset` and updating the generation `blake3::Hasher` without re-compressing or re-hashing chunk payloads.
3. **Directory, Index, and Footer Commit**: The generation directory, copy-on-write B+tree index pages, updated scientific manifest page, and `WX1G` footer are emitted using the exact crash-safe commit protocol of ADR 0022 (supporting both `Buffered` and `Durable` sync modes).

---

## 4. Asynchronous Writer and Backpressure Semantics

### 4.1 Bounded Queue and Memory Budget
`AsyncWriterOptions` configures:
- `max_pending_generations: usize` (default `2`, minimum `1`, maximum `64`);
- `max_queued_bytes: u64` (default `512 MiB`, bounded by host RAM budget);
- `staging_workers: usize` (number of parallel CPU compression threads per submitted step);
- `durability: WriteDurability` (`Buffered` vs `Durable` with `sync_data` + `sync_all`).

### 4.2 Backpressure and Stall Telemetry
When the NWP loop calls `submit_generation(...)`:
- If the queue has capacity and queued bytes are within `max_queued_bytes`, submission returns immediately after staging so the NWP model proceeds to timestep $t + 1$ while the background writer thread writes and syncs timestep $t$.
- If the background writer is still busy and the queue is full, the caller blocks until capacity is freed. The exact blocking duration is accumulated in `total_enqueue_stall_ns`, `max_enqueue_stall_ns`, and `backpressure_events`.

### 4.3 Crash Safety
- Because the background writer uses the append-only generation protocol (ADR 0022), an unexpected process termination (`kill -9`) during an asynchronous background write leaves all previously committed timesteps intact and recoverable via `open_scientific_snapshot_lazy` or `repair_generation_file`.
