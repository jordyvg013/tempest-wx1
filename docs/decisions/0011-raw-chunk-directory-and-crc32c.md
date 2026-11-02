# ADR 0011: Experimental raw chunk directory and CRC32C

- Status: Accepted for experimental v0.3.5 implementation; not a stable interoperability contract
- Date: 2026-10-30

## Context

The metadata-only v0.3.4 reader/writer can create and inspect a container but cannot store scientific payloads. A minimal, bounded chunk path is needed before adding compression, indexes, append generations, and commit/recovery semantics.

## Decision

- Add an experimental trailing `WX1D` directory with a 16-byte header and fixed 40-byte descriptors.
- Directory schema version is 1. Descriptor fields are chunk ID (u64), offset (u64), stored length (u64), raw length (u64), CRC32C (u32), codec (u8), flags (u8), and reserved (u16).
- All multi-byte directory fields use the container's declared byte order.
- This milestone supports codec 0 (raw/uncompressed) only; stored and raw lengths must match. Unknown codecs, flags, and reserved values are rejected.
- Limit chunk count to 1,000,000 and individual chunk size to 64 MiB. Validate all arithmetic, file ranges, duplicate IDs, and overlap before returning descriptors.
- CRC32C (Castagnoli) is checked when a chunk is read. Directory inspection validates descriptor structure but does not read every chunk payload.
- The writer emits metadata, raw chunks, then the directory. The destination must be new or truncated. Atomic publication, append-only updates, commits, and crash recovery remain future work.
- Keep the implementation dependency-free, safe Rust, and byte-order independent.

## Consequences

WX-1 can now store and selectively read bounded raw chunks. This is not yet a complete scientific storage format: no compression, BLAKE3, indexes, chunk semantic schema, append/commit protocol, or recovery is implemented. The experimental layout may change before a normative specification and independent test vectors are published.
