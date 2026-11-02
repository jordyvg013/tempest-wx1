# ADR 0004: Integrity, codecs and committed generations

- Status: Accepted as design direction; exact registries and bytes remain open
- Date: 2026-10-29

## Decision

- CRC32C is the proposed per-chunk fast corruption check.
- BLAKE3 is the proposed stronger integrity digest for configured metadata/index/commit coverage.
- Codec IDs initially proposed: 0 none, 1 Zstandard, 2 LZ4.
- Zstandard is the default lossless codec.
- Lossy transforms require explicit opt-in, distinct IDs and documented error semantics.
- Use append-only generations, end-of-generation footer/commit records and redundant commit records for recovery.

## Consequences

Checksums are not signatures. Codec IDs are permanent and never reused. Exact digest coverage, codec parameter bounds, commit scan limits and durability profile must be specified before a stable format release.
