# ADR 0009: Experimental fixed container header

- Status: Accepted for experimental v0.3.3 implementation; not a stable interoperability contract
- Date: 2026-10-29

## Context

WX-1 currently has safe primitive decoders and a legacy `WX1X` prefix probe, but no real container header. The legacy prefix must not be repurposed as the new format. A small, testable header is the next vertical slice toward a reader/writer.

## Decision

Introduce a separate experimental 32-byte header with the following byte offsets:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | Magic `WX1` followed by NUL |
| 4 | 2 | Major version, selected byte order |
| 6 | 2 | Minor version, selected byte order |
| 8 | 1 | Byte-order marker: 1 little-endian, 2 big-endian |
| 9 | 1 | Flags; currently must be zero |
| 10 | 2 | Header length; currently exactly 32 |
| 12 | 8 | Metadata byte length |
| 20 | 8 | Directory offset; zero means absent |
| 28 | 4 | Reserved; must be zero |

All multi-byte integers use the byte order identified at offset 8. The current decoder accepts major version 1 only and rejects unknown flags and nonzero reserved bits. Metadata begins immediately after the 32-byte header. If a directory offset is present, it must not overlap the header or metadata and must be within the known file length.

## Safety requirements

- Header parsing performs no allocation.
- Truncated input is rejected.
- Range arithmetic is checked for overflow.
- File bounds are validated before later reads or allocations.
- This implementation does not yet validate metadata CBOR, directory pages, chunks, checksums, commits, or recovery.

## Consequences and limitations

This is an experimental implementation milestone only. The exact layout may change before a stable WX-1 1.0 specification. No existing legacy prefix bytes are reinterpreted. A future normative specification must additionally define feature negotiation, footer/commit semantics, metadata schema, chunk records, integrity, and forward-compatibility rules.
