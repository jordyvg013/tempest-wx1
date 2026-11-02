# WX-1 streaming and commit draft v0.3.7

**Experimental, non-normative.** The 40-byte chunk descriptor remains unchanged.

## Streaming APIs

- `ChunkReaderInput` accepts a reader and declared raw length.
- Raw and Zstandard writes stream with a configurable 1-byte to 8-MiB buffer.
- LZ4 block writing/reading materializes one chunk, bounded to 64 MiB.
- `copy_chunk_payload_to_writer` performs a bounded CRC32C pass before streaming raw/Zstandard decode to a writer.
- `read_raw_chunk_range` verifies CRC32C over the entire stored raw chunk, then copies only the requested range.
- `for_each_input_chunk` splits an input stream into caller-sized chunks (maximum 64 MiB).

## BLAKE3 and commit footer

The optional 48-byte footer is:

| Offset | Length | Field |
|---:|---:|---|
| 0 | 4 | ASCII `WX1C` |
| 4 | 2 | Version, little-endian `1` |
| 6 | 2 | Reserved zero |
| 8 | 8 | Length of the committed file prefix, little-endian |
| 16 | 32 | BLAKE3 digest of bytes `[0, committed_length)` |

The footer must be exactly at EOF. The digest is unkeyed and detects accidental corruption, not malicious tampering. Legacy files without a footer remain accepted. A present but invalid footer is rejected.

## Index and parallel preparation

`ChunkIndex` provides an in-memory BTreeMap by chunk ID and physical-offset ordering. `ScientificChunkIndex` accepts explicit variable/time/level/spatial-tile keys. These are not serialized. `for_each_chunk_batch` emits bounded descriptor batches; the caller owns worker pools and concurrency policy.

## Crash safety

Atomic helper functions write to a sibling temporary file, sync file contents, append/verify the commit footer where requested, rename into place and sync the parent directory on Unix. This protects new-file publication only. Append-only generations, multi-commit recovery, torn-write scanning and rollback to prior generations are not implemented.
