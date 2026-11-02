# ADR 0008: Explicit parameter overlays and conservative unit safety

- Status: Accepted for v0.3.2
- Date: 2026-10-29

## Context

Users need local corrections and provider-specific extensions without silently changing upstream WMO/provider definitions. A parameter's unit is semantic data: changing a unit to an unrelated physical dimension must not pass unnoticed.

## Decisions

1. An overlay identifies one exact base table version and a distinct target version, and carries publisher, source, license and a non-empty reason.
2. Applying an overlay returns a new table package. The original is immutable. Installation remains subject to the registry's existing no-overwrite rule.
3. This first overlay API may replace existing external-code definitions only; it cannot add codes. New definitions belong in a separately namespaced table package.
4. Matching uses the complete external code tuple. Duplicate keys in one overlay are rejected.
5. Known units are checked using a conservative built-in dimension vocabulary. Unknown units remain preserved in imported base tables, but overrides involving unknown units are rejected because compatibility cannot be proven.
6. This is not a full unit algebra or a claim of CF/UDUNITS conformance. A future adapter may integrate a vetted unit database without changing the preservation rule.

## Consequences

Local overrides are reviewable and versioned, and cannot silently corrupt the upstream table or change a parameter's known physical dimension.
