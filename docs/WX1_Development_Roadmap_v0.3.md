# WX-1 development roadmap after v0.3.0

## Completed in v0.3.0
- [x] Safe explicit-endian fixed-width decoding primitives.
- [x] Initial parameter registry model and CSV import.
- [x] Immutable table version insertion and exact external-code lookup.
- [x] Tests for endian behavior, truncation, CSV quoting, duplicate keys and immutable versions.

## Completed in v0.3.1
- [x] Deterministic CSV export and round-trip test.
- [x] Table-version diff (added/removed/changed), matched by complete external code tuple.
- [x] Deterministic registry table identity enumeration.
- [x] Tests for CSV round-trip, diff classifications and incompatible table families.

## Next implementation increments
1. Define and review the versioned registry package schema, then add deterministic JSON import/export and overlay semantics.
2. Add unit/dimension validation and explicit unknown-code preservation.
3. Specify byte-level file header, metadata envelope, chunk record, footer and commit record with test vectors before declaring them normative.
4. Implement bounded chunk descriptors and checked offset/length validation.
5. Add lossless codec and checksum adapters behind stable registries.
6. Implement append-only writer, reader, index pages and recovery tests.
7. Add Python packaging and bindings only after stable Rust APIs exist; keep NumPy/xarray/Dask/MetPy optional.
8. Add bounded execution scheduler and backend capability/fallback interface before any GPU kernels.

Every milestone must update tests, docs and changelog. No planned feature is considered implemented until code and tests exist.
