# WX-1 expanded chunk and metadata limits — specification draft

- Status: Draft; source implementation and WSL verification pending
- Related decision: ADR 0034

## Goals

Allow the planned scientific planner stress case with 1,048,576 chunk references to pass the same scientific model validation and metadata serialization path used by the implementation, without changing descriptor or directory field widths.

## Proposed limits

| Limit | Historical candidate | Proposed Phase-2 candidate |
|---|---:|---:|
| Scientific/generation chunk references | 1000000 | 1048576 |
| Encoded metadata | 16 MiB | 32 MiB (`33554432` bytes) |

These values are proposed implementation limits, not verified results. Independent limits such as maximum index pages, per-chunk bytes, decoded array output and generation-chain length remain separate and must not be increased by implication.

## Validation and allocation rules

- Validate declared counts and byte lengths before allocation.
- Use checked arithmetic for products, offsets and aggregate counts.
- Use fallible reservation for large count-derived vectors.
- Apply the intended profile's limits consistently in writer, reader, append, directory parser, index catalog/tree parser and scientific metadata decoder.
- Keep old-profile acceptance rules explicit; do not accidentally apply the enlarged cap to older profiles if profile-specific compatibility is required.

## Benchmark semantics

The 1,048,576 case validates and serializes an in-memory scientific model before measuring `plan_array_slice`. It is not a promise that a file with 1,048,576 payload chunks will be generated or that the whole storage path has been benchmarked at that count. The 4,096 historical baseline remains `6228.0 ns` per plan (`6.228 us`) and the 65,536/1,048,576 measurements remain `not measured` until the user's WSL run.

## Wire compatibility

No descriptor or index-page layout change is intended. However, a larger accepted resource limit changes reader compatibility for newly written large files. The implementation must explicitly decide whether an on-disk profile/header-minor signal is required. Existing 72-byte and 40-byte descriptors must not be changed as part of this limit expansion.

## Acceptance

The draft is accepted only after boundary tests, old-profile regression tests, workspace tests, Clippy and the in-memory benchmark scale cases pass in the target WSL environment.
