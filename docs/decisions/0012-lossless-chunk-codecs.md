# ADR 0012: Experimental lossless chunk codecs

- Status: Accepted for experimental v0.3.6; not a stable interoperability contract
- Date: 2026-10-30

## Context

The v0.3.5 directory stores raw chunks only. Compression must be independently applied to each chunk so that chunks remain addressable and bounded in memory. Codec identifiers were already proposed in ADR 0004.

## Decision

- Codec ID 0 is raw/uncompressed.
- Codec ID 1 is a Zstandard frame (writer level 3).
- Codec ID 2 is an LZ4 block as defined by `lz4_flex::block` (no size prefix; decoded length comes from the directory).
- CRC32C covers the stored representation and is verified before decompression.
- The directory's existing codec byte is used; descriptor size and field offsets do not change.
- Raw length is limited to 64 MiB. Stored length is limited to 65 MiB to allow bounded codec framing overhead.
- Decompression must produce exactly the declared raw length. Zstandard output is capped at raw length plus one byte to detect over-expansion; LZ4 receives the exact expected output size.
- Chunks are encoded independently. The writer retains at most one encoded chunk in addition to caller-owned raw input at a time. The reader materializes one decoded chunk at a time, bounded to 64 MiB.
- Lossy codecs remain unsupported. Codec IDs are not to be reused.

## Consequences and limitations

- Existing raw-only v0.3.5 files remain readable because codec ID 0 is unchanged.
- The v0.3.6 implementation still uses complete per-chunk buffers; it is bounded chunk processing, not sub-chunk streaming. Range reads, streaming sinks, compression-ratio policy, and formal codec conformance vectors remain future work.
- This draft remains experimental. The exact LZ4 block profile and Zstandard frame constraints must be frozen and accompanied by independent cross-language test vectors before a stable format release.
