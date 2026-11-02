# ADR 0007: Deterministic registry export and version diff

- Status: Accepted for v0.3.1
- Date: 2026-10-29

## Context

The first registry milestone could import and validate a CSV package, but users could not export an installed table or review changes between published versions.

## Decisions

1. CSV export uses a fixed documented column order and deterministic row ordering by the complete external code tuple.
2. Fields containing commas, quotes or line breaks are escaped using doubled quotes.
3. Version diffs match parameters only by the complete external code tuple. A changed name, units string, canonical ID or description is a changed definition, not a new identity.
4. Diffing is permitted only between versions sharing the same namespace and table identifier.
5. This ADR does not define overlays, override precedence, JSON schema or official WMO/provider table contents.

## Consequences

Table packages can be round-tripped through the supported CSV profile and version changes can be reviewed without guessing parameter identity.
