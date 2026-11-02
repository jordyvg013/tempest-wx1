# ADR 0025: Parallel Tile/Rank Staging, Deterministic Merge/Commit, and Bounded Asynchronous Writer

**Status:** Accepted  
**Date:** 2026-11-02  
**Context:** Phase 4, Part 3 (`WX1_ROADMAP_CURRENT.md`, `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md` §4.4, §6.6 W3/W5/W6)

---

## Context

In Numerical Weather Prediction (NWP) workflows—both regional grids and global 6-tile cubed-sphere models—forecast integration steps run across multiple CPU threads, domain tiles, or device ranks. Writing checkpoints (`restart`) or diagnostic fields (`output`) sequentially on the main model thread causes two bottlenecks:
1. **CPU Compression Bottleneck:** Byte-shuffling, Zstandard compression, CRC32C calculation, and BLAKE3 hashing dominate write CPU time when executed on a single thread.
2. **Model Step I/O Stall:** Waiting synchronously for disk writes and `fsync` blocks the NWP timestep loop, violating the `W5`/`W6`/`D4` requirement that I/O stall remain $\le 5\%$ of compute-only model step time.

At the same time, WX-1's safety rules forbid uncoordinated concurrent mutation of shared chunk directories, B+tree index pages, or commit footers.

---

## Decision

1. **Two-Phase Parallel Staging and Deterministic Commit (`crates/wx1-format/src/staging.rs`):**
   - **Parallel Staging Phase:** Worker threads (`1, 2, 4, 8, 12, ...`) independently encode raw chunk slices or tiles into self-contained `StagedChunk` structs using safe Rust (`std::thread::scope`, `#![forbid(unsafe_code)]`). Each worker performs byte-shuffling, Zstandard/LZ4 compression, `CRC32C` calculation, and `BLAKE3` hashing in parallel.
   - **Deterministic Merge & Commit Phase:** A single coordinator receives the `StagedChunk` vector, sorts chunks canonically by ascending `chunk_id`, validates scientific model coverage and byte limits, and writes the pre-compressed payloads, directory v2, persistent B+tree index pages, and `WX1G` commit footer sequentially.
   - **Bit-Identical Output Across Worker Counts:** Because chunk payloads and digests depend only on the chunk data and codec, and the coordinator sorts by `chunk_id`, writing with $1, 2, 4, 8,$ or $12$ workers produces a **bit-for-bit identical** `.wx1` file.

2. **Bounded Asynchronous Writer (`AsyncScientificWriter`):**
   - A dedicated background commit thread receives staged generations over a bounded channel (`sync_channel(max_pending_generations)`) guarded by a byte-budget semaphore (`max_queued_bytes`).
   - Supports both initial container creation (generation 1) and multi-timestep append (generations $2..N$, growing the time axis via ADR 0022).
   - Supports explicit `WriteDurability::Buffered` and `WriteDurability::Durable` modes (`W2`, `W4`, `W6`).
   - Records `AsyncWriterTelemetry` (`submitted_generations`, `committed_generations`, `total_staged_bytes`, `total_enqueue_stall_ns`, `max_enqueue_stall_ns`, `backpressure_events`, `total_commit_duration_ns`).

3. **Python & NWP Integration:**
   - `wx1.write_array` gains an optional `workers: int = 1` parameter that releases the GIL and stages chunks in parallel when `workers > 1`.
   - `wx1.AsyncWriter` exposes the bounded asynchronous timestep writer and telemetry to Python/NumPy/JAX NWP loops.

---

## Consequences

- **Positive:**
  - Multi-core CPU scaling for compression, byte-shuffle, CRC32C, and BLAKE3 without any lock contention on B+tree or directory structures.
  - Deterministic, bit-exact wire output across any number of threads/workers.
  - NWP timestep loops overlap compute with background I/O and have built-in stall/backpressure telemetry for `W5`, `W6`, and `D4`.
  - Zero wire-format changes; 100% compatible with existing v0.4.x readers and crash recovery (`C1`).
- **Constraints & Bounds:**
  - In-memory staging is bounded by `max_pending_generations` and `max_queued_bytes`; each individual chunk remains strictly bounded by `MAX_CHUNK_BYTES` (64 MiB).
