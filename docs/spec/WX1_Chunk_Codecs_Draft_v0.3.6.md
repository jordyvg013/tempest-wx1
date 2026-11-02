# WX-1 chunk codecs — experimental draft v0.3.6

**Status:** experimental and non-normative. Not a stable interoperability contract.

## Directory compatibility

The v0.3.5 40-byte descriptor is retained unchanged. The byte at descriptor offset 36 is the codec ID; flags and reserved fields retain their v0.3.5 meaning. Existing raw chunks remain codec 0.

| ID | Codec | Stored representation |
|---:|---|---|
| 0 | Raw | Exact uncompressed bytes; stored length equals raw length. |
| 1 | Zstandard | One Zstandard frame, written at level 3 by this implementation. |
| 2 | LZ4 | LZ4 block produced by `lz4_flex::block::compress`; no embedded size prefix. |

All codecs are lossless. Lossy codecs are not supported. Codec identifiers are permanent and must not be reused.

## Limits and validation

- Decoded chunk length: at most 64 MiB.
- Stored chunk length: at most 65 MiB.
- CRC32C (Castagnoli) covers the exact stored bytes and is verified before decoding.
- The decoder must produce exactly the descriptor's raw length.
- Zstandard decoding is capped at raw length plus one byte, so an expansion beyond the declared size is rejected.
- LZ4 decoding is given the exact declared output size.
- Unknown codec IDs, invalid flags, invalid ranges, overlapping ranges, duplicate IDs, oversized lengths, checksum failures, and decoded-length mismatches are rejected.

## Memory model

Chunks are independently encoded. The writer processes one chunk at a time and holds at most one encoded chunk buffer in addition to caller-owned raw input. The reader verifies and decodes one complete chunk at a time. The decoded allocation is bounded to 64 MiB. This is bounded per-chunk processing, not sub-chunk streaming; range reads and streaming sinks are not implemented.

## Interoperability caveats

This draft does not yet freeze compression parameters, dictionary policy, frame concatenation policy, or independent test vectors. Implementations must not assume that this draft is a stable v1.0 contract.
