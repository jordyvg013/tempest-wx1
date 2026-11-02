# WX-1 standalone development roadmap v0.1

## Purpose

WX-1 is an open, language-independent binary container and storage engine for large scientific datasets, with meteorology as an important target workload. The repository is independent from Tempest. Rust is the initial implementation language only; the file format must be implementable from its specification in other languages.

## Non-goals for this checkpoint

- No stable or frozen WX-1 wire format.
- No complete file reader/writer.
- No chosen metadata serialization, checksum, or default codec.
- No promise of compatibility with NetCDF, HDF5, Zarr, or GRIB yet.
- No Tempest language/runtime dependency.

## Design requirements

1. Explicit fixed-width integer fields and explicit byte order in any future wire format.
2. No dependence on Rust struct layout, pointers, host endianness, CPU/GPU, or codec internals.
3. Bounded metadata, nesting, chunk sizes, decompressed output, and memory use.
4. Checked offset/length arithmetic before seeking, allocating, or reading.
5. Chunked streaming and partial reads; never require the complete dataset in RAM or VRAM.
6. Crash-safe publication with a clearly validated commit/footer protocol.
7. Checksums for corruption detection; cryptographic authentication is a separate feature.
8. Lossless compression as the default. Lossy transforms must be explicit and carry documented error semantics.
9. Unknown future metadata and parameter identifiers are retained where possible, not guessed.
10. Scientific metadata must represent units, dimensions, coordinate systems, temporal/statistical meaning, missing values, provenance, and quality.

## Proposed repository stages

### Stage A — specification and threat model
- Define terminology, conformance levels, versioning, compatibility, limits, and threat model.
- Decide byte order, header/footer layout, metadata encoding, checksums, codec registry, and commit semantics through separate ADRs.
- Review the binary layout independently of the Rust implementation.

### Stage B — minimal wire-format prototype
- Implement bounded header/footer parsing and serialization.
- Add metadata root and chunk directory.
- Add one lossless codec only after benchmark and licensing review.
- Add checksums and a validation CLI.
- Test round trips, truncation, overflow, malicious lengths, unknown features, and crash interruption.

### Stage C — scientific data model
- Define variable, dimension, coordinate, time, vertical-coordinate, grid, missing-value, provenance, and quality metadata.
- Define meteorological parameter namespaces and versioned registries without hard-coding one vendor's codes as universal identifiers.

### Stage D — interoperability and performance
- Streaming read/write and subset/crop queries.
- Import/export adapters for GRIB, NetCDF, HDF5, and Zarr, with explicit reporting of semantic information that cannot be represented.
- Reproducible benchmarks on representative weather workloads.

### Stage E — bindings and backends
- C ABI first if justified, then Python and other language bindings.
- CPU-only functionality remains complete without GPU toolchains.
- Optional GPU codecs/operations use bounded buffers and never assume all data fits VRAM.

## Current checkpoint

This repository only extracts the old experimental prefix probe into an independent workspace. That prefix is explicitly legacy and non-normative. It is not a file format and must not be used as the basis for external data exchange. The next work item is the normative specification and ADR review, not adding more fields to this legacy prefix.
