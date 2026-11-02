# WX-1 raw chunk directory — experimental draft v0.3.5

**Status:** experimental and non-normative. Not a stable interoperability contract.

## Container layout

The existing experimental 32-byte header and v0.3.4 deterministic-CBOR metadata profile are retained. Metadata begins at byte 32. Raw chunk payloads follow metadata contiguously in writer output. A trailing directory begins at the header's nonzero `directory_offset`.

## Directory header

The directory header is 16 bytes:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | ASCII magic `WX1D` |
| 4 | 2 | Directory schema version, currently 1 |
| 6 | 2 | Descriptor size, currently 40 |
| 8 | 4 | Descriptor count |
| 12 | 4 | Reserved, must be zero |

All multi-byte integers use the container byte-order marker.

## Descriptor

Each descriptor is exactly 40 bytes:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Unique chunk ID |
| 8 | 8 | Absolute file offset |
| 16 | 8 | Stored length |
| 24 | 8 | Raw length |
| 32 | 4 | CRC32C of stored bytes |
| 36 | 1 | Codec: 0 = raw only |
| 37 | 1 | Flags: currently zero |
| 38 | 2 | Reserved: currently zero |

For codec 0, stored length MUST equal raw length. Each chunk is limited to 64 MiB and the directory to 1,000,000 descriptors. Chunk byte ranges must be within the payload region (after metadata and before the directory), must not overlap, and IDs must be unique. The file length must equal the end of the directory. Readers reject unknown codec values, nonzero flags/reserved fields, invalid ranges, duplicates, and arithmetic overflow.

## Integrity and partial reads

CRC32C uses the Castagnoli polynomial (reflected representation `0x82F63B78`). Directory parsing validates descriptors. `read_chunk(id)` reads only the selected payload and verifies its CRC32C before returning bytes. This API currently loads one complete chunk, bounded to 64 MiB; sub-chunk range reads and streaming buffers are future work.

## Explicit non-goals

This draft does not define compression, BLAKE3, indexes, semantic dimensions/coordinates, append-only generations, commit/footer records, atomic publication, or crash recovery. The writer requires a new or truncated destination and emits metadata, raw chunks, then the directory.
