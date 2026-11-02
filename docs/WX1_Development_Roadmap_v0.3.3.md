# WX-1 roadmap — v0.3.3

## Completed in this milestone

- Experimental fixed container header with explicit endian marker.
- Header encode/decode and strict structural validation.
- Checked metadata and directory bounds against a caller-supplied file length.
- ADR 0009 records the provisional layout and its limitations.

## Next vertical slices

1. Define and validate the metadata envelope and deterministic CBOR profile.
2. Define chunk descriptors and codec identifiers independently from physical index pages.
3. Implement a bounded reader over `Read + Seek`, validating every offset and length before reading.
4. Implement a minimal writer to a new temporary file, then finalize atomically where the platform permits.
5. Add CRC32C chunk integrity and corruption/truncation tests.
6. Only then add directory/index pages, append-only generations, commit/footer and recovery.

The v0.3.3 header remains experimental and may change before the stable WX-1 format specification. Do not treat it as a finalized public wire contract.
