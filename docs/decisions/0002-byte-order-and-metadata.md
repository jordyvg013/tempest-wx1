# ADR 0002: Byte order and metadata encoding

- Status: Accepted as project design direction; exact wire bytes remain open
- Date: 2026-10-29

## Context

WX-1 needs portable files across host architectures and a compact, extensible metadata representation.

## Decision

- Support little-endian and big-endian files.
- Use little-endian for newly created files by default.
- Declare byte order explicitly in the file header; never infer it from host architecture.
- Use deterministic CBOR constrained by a versioned WX-1 schema for canonical metadata.
- JSON may be used for diagnostics/export, not as the canonical on-disk metadata encoding.

## Consequences

Readers and writers must test both byte orders. Exact marker values, field widths, CBOR profile and schema are deferred to the normative wire specification. This ADR does not freeze any byte offsets or signature.
