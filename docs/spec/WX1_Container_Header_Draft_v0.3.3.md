# WX-1 container header — experimental draft v0.3.3

**Status:** experimental, non-normative. See ADR 0009. Do not treat this draft as a stable interchange contract.

The experimental fixed header is 32 bytes. Magic is `WX1\0`. Integer fields are encoded using the declared byte order marker (1 little-endian, 2 big-endian). Fields and offsets are defined in `docs/decisions/0009-experimental-container-header.md`.

The reader rejects truncated headers, unsupported major versions, unknown nonzero flags, invalid header lengths, nonzero reserved bits, metadata ranges beyond the file, directory offsets that overlap metadata, and directory offsets beyond the file. The metadata payload is parsed by the v0.3.4 metadata profile. The trailing raw chunk directory is parsed by the experimental v0.3.5 profile; it is not a stable interoperability contract.

The v0.3.4 and v0.3.5 experimental profiles add deterministic metadata CBOR, a raw chunk directory, and CRC32C. A future normative specification must still define mandatory/optional feature negotiation, compression, indexes, commit/footer records, crash recovery, and independent interoperability test vectors before a complete stable format is advertised.
