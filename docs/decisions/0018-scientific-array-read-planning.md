# ADR 0018 — inspectable scientific array read planning

- **Status:** Proposed for v0.4.2; accept after WSL verification and API review.
- **Date:** 2026-10-30

## Context

v0.4.0/v0.4.1 provide multidimensional scientific arrays and `ScientificSnapshot::read_array_slice`. The reader already avoids decoding non-intersecting chunks, but selection validation and chunk choice are internal to the read operation. External importers and schedulers cannot inspect the chunk work before asking the reader to materialize the output.

The next weather-data stages will need explicit separation between selection semantics, I/O planning, chunk decoding and later scientific interpretation. However, calendar, vertical-coordinate, CRS and provider-specific query semantics are not yet sufficiently specified to make those queries part of this increment.

## Decision

Add the experimental `ArrayReadPlan` value type and `ScientificSnapshot::plan_array_slice`. The plan exposes the variable path, requested shape, datatype, payload byte order, checked output byte count and ordered IDs of intersecting chunks. It does not expose file offsets, bypass snapshot validation, decode data, or change the wire format.

`read_array_slice` must call the same planner and decode only the returned IDs. This avoids divergence between an inspection API and actual I/O selection. Selection shape/rank/bounds and the existing 64 MiB output limit are validated before output allocation.

## Alternatives considered

1. **Keep planning private.** Rejected because callers cannot estimate or schedule the I/O work before reading.
2. **Expose raw file offsets and stored ranges.** Rejected because physical directory details should remain behind the validated snapshot/reader boundary and could couple callers to a provisional wire layout.
3. **Use persistent indexes immediately.** Deferred because the current persistent logical indexes have opaque caller-defined key semantics; calendar, physical vertical and CRS-aware spatial keys are not defined yet.
4. **Add a stable importer contract now.** Deferred until this experimental plan is exercised and exact metadata/provenance preservation behavior is specified.

## Consequences

- Existing file bytes and reader output semantics are unchanged.
- The API is additive but experimental; no stable v1.0 compatibility promise is made.
- Initial planning cost is linear in the variable's chunk-reference count.
- Selected compressed chunks are still fully decoded within the existing chunk bound.
- Large-chunk-count performance benchmarks and indexed planning remain required before making scalability claims.
