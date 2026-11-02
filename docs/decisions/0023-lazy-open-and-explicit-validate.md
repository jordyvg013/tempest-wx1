# ADR 0023: Lazy Open and Explicit Full Validation

- Status: Accepted for implementation (Fase 3)
- Date: 2026-11-01
- Authority: `WX1_ROADMAP_CURRENT.md`, `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md`

## Context

WX-1 v0.4.3 open routines (`open_generation_snapshot`, `open_scientific_snapshot`) eagerly parse all directory entries, check the CRC32C checksum of every stored chunk payload on disk, traverse all B+tree index pages, and calculate a BLAKE3 cryptographic hash over every generation's byte range.
While this guarantees immediate full container validation on open, it creates an $O(\text{file\_size} + N_{\text{chunks}})$ open latency bottleneck:
- At 4,096 chunks: 0.755 ms.
- At 65,536 chunks: 19.066 ms.
- At 1,048,576 chunks: 278.105 ms (metadata alone).
On 100 GB files with operational NWP grids, open times scale to seconds or minutes, making acceptance threshold R1 (open time ratio 100 GB / 1 GB $\le 2.0$) unachievable.

## Decision

1. **Lazy Open as Primary Open Path:**
   Implement `open_generation_snapshot_lazy` and `open_scientific_snapshot_lazy` in `crates/wx1-format`.
   - Verify the 64-byte container header and seek to inspect the newest generation footer.
   - Verify generation footer bounds and directory offsets without walking or hashing chunk payloads.
   - Defer per-chunk CRC32C verification to read time (`read_generation_chunk_payload`), which already validates CRC32C before decompression.
   - Defer generation-span BLAKE3 hashing and deep index page tree traversal.
2. **Explicit Full Validation Method:**
   Provide `validate_full(&self, reader: &mut R) -> Result<(), ChunkIoError>` on `GenerationSnapshot` and `ScientificSnapshot`.
   This method carries out the exhaustive verification (all chunk CRC32Cs, index pages, and generation BLAKE3 digests) when archival integrity verification is explicitly requested by the caller.
3. **Preserve Wire Contract:**
   No changes to container header (64 bytes), generation descriptor (72 bytes), or footer layouts (80/72 bytes). Existing files remain 100% binary compatible.
4. **Python & CLI Integration:**
   - In Python, `wx1.open_file` and `xarray` backend use lazy open by default, providing sub-millisecond open latency. Add a `.validate()` / `.validate_full()` method to `Wx1File`.
   - Update `wx1` CLI: `wx1 view` uses lazy open for instant inspection; `wx1 check` retains or adds a `--deep` / `--full` flag for full cryptographic validation.

## Consequences

- Open time becomes bounded $O(1)$ relative to total stored chunk payload bytes.
- Open time on 1,048,576 chunks drops from 278 ms to $<1\text{ ms}$, satisfying the R1 acceptance criterion.
- Read operations maintain identical safety guarantees because payload CRC32C is checked on demand when individual chunks are read.
- Archival validation and recovery workflows can invoke `validate_full()` explicitly when data integrity certification is required.
