# ADR 0010: First metadata-only reader/writer vertical slice

- Status: Accepted for experimental v0.3.4 implementation; not a stable interchange contract
- Date: 2026-10-30

## Context

The v0.3.3 milestone could encode/decode a draft header but could not create or open a WX-1 container. A small end-to-end vertical slice is needed before adding chunk and index structures.

## Decision

- Add a bounded, deterministic CBOR metadata profile (schema version 1) with dataset ID and string attributes.
- Use a definite-length three-entry CBOR map with integer top-level keys and canonical ordering for attribute text keys.
- Limit metadata to 16 MiB and attribute count to 100,000.
- Implement stream-based metadata-only write and read APIs using the existing experimental header.
- Metadata-only files have `directory_offset = 0` and no trailing bytes; readers reject directory offsets and trailing data until those structures are implemented.
- Keep the implementation dependency-free and safe Rust; do not add unsafe code.

## Consequences

This enables actual metadata-only file creation and inspection, but it is not yet a complete scientific storage engine. The narrow schema rejects unknown top-level fields and does not yet provide arbitrary typed metadata. The header and CBOR profile remain experimental pending a normative wire specification and independent test vectors.
