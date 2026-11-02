# ADR 0003: Multiple logical indexes

- Status: Accepted as project design direction; page layout remains open
- Date: 2026-10-29

## Decision

WX-1 supports multiple logical indexes over shared stable chunk identities: variable, time, spatial/grid, vertical coordinate, model/provider and chunk-location indexes. B+tree is the baseline ordered-index structure. A specialized spatial index may be used where justified. Index pages are loaded on demand and subject to explicit cache limits.

## Consequences

The file format must advertise available index capabilities. Exact key ordering, page layout, balancing and optional-index semantics require normative specification and test vectors. Readers must remain able to report missing optional indexes and unsupported required index features clearly.
