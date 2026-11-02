# ADR 0015: Persistent copy-on-write B+tree indexes

- Status: Accepted for experimental v0.3.9 implementation; not a stable interoperability contract
- Date: 2026-10-30

## Context

ADR 0003 selects a B+tree as the baseline ordered index over stable chunk IDs. v0.3.8 adds append-only generations and a pinned snapshot, but all lookup structures are in memory. The primary chunk directory must remain authoritative, and indexes must not expose unvalidated offsets. The index roots must be committed atomically with the directory and data.

## Decision

1. Retain the v0.3.8 directory-v2 72-byte descriptor unchanged. Add a generation-profile minor version 5 and `WX1G` footer version 2 (80 bytes) carrying an index-catalog page offset.
2. Use immutable, fixed 4096-byte `WX1B` pages. Each page contains a BLAKE3 digest over all 4096 bytes with the digest field zeroed. All fields use the container byte order.
3. Implement one generic ordered B+tree keyed by a fixed 32-byte opaque key plus a stable chunk ID. Duplicate logical keys are allowed for multiple chunks; duplicate composite mappings are rejected.
4. Provide logical index namespaces for chunk ID, variable, time ordinal, vertical ordinal, spatial tile ID and model/provider. The mandatory chunk-ID tree is built/updated automatically. Secondary entries are supplied explicitly by callers.
5. Persist a one-page index catalog as the final page of every v0.3.9 generation. The new footer points to that page. Catalog roots for unchanged indexes may point to pages in earlier committed generations.
6. Implement path-copy insertion: pages on a modified search path are written anew; unmodified subtrees are reused. Published pages are never overwritten. Deletion, compaction and vacuum are deferred.
7. Validate every index tree against page regions covered by committed generation footers, verify all referenced chunk IDs against the primary directory chain, and require the chunk-ID index to match that directory exactly. A corrupt index is a specific open/validation error; do not trust its offsets or silently ignore it.
8. Keep support for header minor 4 / footer v1 (v0.3.8) and legacy v1 directory / optional `WX1C` files. New files use header minor 5 / footer v2.
9. Expose exact-key and inclusive byte-key range queries on a generation-pinned snapshot. The API returns validated chunk IDs, never arbitrary file offsets.
10. Keep time/calendar, physical vertical coordinates, CRS-aware spatial queries and model/provider semantics explicitly opaque until the v0.4 scientific schema defines canonical keys. No semantic inference is permitted in v0.3.9.

## Alternatives considered

- **Put index pointers in the mutable header:** rejected because it violates the append-only generation root model established by ADR 0014.
- **Expand the 72-byte chunk descriptor:** rejected because indexes are separate lookup structures and do not belong in each physical storage descriptor.
- **Use one flat sorted table:** rejected because it requires linear page scans or full table rewrites for updates and does not provide the ordered tree lookup selected by ADR 0003.
- **Use an R-tree or Hilbert index immediately:** rejected because CRS, coordinate order, longitude wrapping and regular-grid semantics are not defined yet.
- **Treat an index as authoritative for payload offsets:** rejected because corruption could redirect reads; all returned chunk IDs must resolve through the primary directory.
- **Silently ignore corrupt indexes and scan:** rejected for v0.3.9 because the mandatory chunk-ID index and committed catalog are part of the generation integrity contract. Missing optional logical indexes remain distinguishable from corruption.

## Consequences

- New v0.3.9 generation files are not readable by v0.3.8 readers; v0.3.9 readers retain v0.3.8 read compatibility.
- The generation digest now covers new B+tree/catalog pages; each page also has an independent BLAKE3 digest.
- Appends can share unchanged index subtrees and rewrite only modified paths plus a new catalog.
- The mandatory chunk-ID tree grows with visible chunks; this adds file size and validation work.
- Index validation adds page reads during open. Query paths are bounded by page/depth/result limits and remain pinned to a snapshot.
- The time, vertical and spatial keyspaces provide explicit caller-defined identifiers only. They do not yet provide meteorological time interval, level-unit or CRS-aware spatial semantics.
- Fuzzing, independent wire-format vectors, large-scale performance measurements and power-loss tests remain required before format stabilization.
