# WX-1 metadata profile — experimental draft v0.3.4

**Status:** experimental implementation profile; not a stable interoperability contract.

## Scope

This milestone implements the first end-to-end metadata-only container path. It does not yet define scientific chunks, directory pages, indexes, integrity records, commit/footer records, append generations, or recovery.

## Container

The 32-byte header remains the experimental layout in ADR 0009. New files use little-endian by default; the reader accepts both byte orders. The metadata payload starts at byte 32. For this metadata-only profile, `directory_offset` MUST be zero and the file length MUST equal `32 + metadata_len`. Metadata is limited to 16 MiB before allocation.

## Deterministic CBOR schema version 1

The payload is a definite-length CBOR map with exactly three entries, in this order:

| Integer key | Value | Meaning |
|---:|---|---|
| 0 | unsigned integer `1` | Metadata schema version |
| 1 | UTF-8 text | Non-empty dataset identifier |
| 2 | map of UTF-8 text to UTF-8 text | Extensible string attributes |

The attributes map uses deterministic CBOR key ordering: shorter encoded keys first, then bytewise lexical order. All lengths use the shortest CBOR additional-information representation. Indefinite-length items, duplicate keys, non-text attribute keys/values, non-canonical ordering, unknown top-level keys, trailing bytes, and unsupported schema versions are rejected.

The current profile intentionally supports only string-valued attributes. Rich typed scientific metadata, unknown-field preservation, and schema evolution will be specified before interoperability is claimed.

## Reader/writer behavior

`write_container` writes the header and encoded metadata and flushes the supplied writer. It does not truncate a pre-existing stream. File callers should create/truncate the destination first and should use a temporary file plus atomic rename when publishing a new file.

`read_container` determines the stream length, reads and validates the complete header, validates metadata bounds before allocating, then strictly decodes the metadata payload. It rejects nonzero directory offsets and trailing file bytes because chunk directories are not implemented in this milestone.
