# WX-1 Parameter and Custom Table Registry — draft

## Goals

The registry maps external meteorological/scientific codes to stable WX-1 parameter identities without losing original codes or source context. It supports maintained public tables and private/user tables.

## Table package identity

Every table package declares:

- namespace (for example `wmo.grib2`, `knmi.harmonie`, `ecmwf.local`, `user.example_model`);
- table ID and immutable table version;
- WX-1 table-schema version;
- publisher/owner and provenance;
- license and attribution text or reference;
- creation/publication timestamp;
- optional content digest/signature;
- entries and optional parent/overlay references.

A namespace is not a global claim of ownership until registered under the project's future namespace policy. User namespaces can be local/private and collision-safe.

## Entry semantics

An entry may contain canonical WX-1 parameter ID, external code tuple, long/short name, description, unit, physical dimension, standard name mapping, vertical-coordinate meaning, temporal/statistical processing, level type, valid range, missing-value semantics, deprecation status and replacement reference.

The identity of a parameter is not its display name or numeric code alone. External code resolution uses namespace + table ID + table version + code tuple + any required centre/subcentre/local-table context.

## Import formats

Required initial import formats proposed:

- CSV with an explicit column mapping or documented WX-1 CSV schema;
- JSON with a versioned WX-1 table schema.

Optional import adapters:

- YAML (PyYAML extra);
- XLSX (openpyxl extra);
- source-specific GRIB/NetCDF metadata extraction adapters.

Native WX-1 table packages provide deterministic archival and distribution. Importers treat input as data only; no macros, scripts, expressions or executable payloads are accepted.

## Validation and conflict rules

- Validate schema version, required fields, field types, size limits and encoding.
- Validate code uniqueness within the table's declared key space.
- Validate unit syntax and known dimensional consistency where possible.
- Flag semantic collisions; never silently merge parameters with different temporal, vertical or statistical meaning.
- Unknown fields are preserved in a namespaced extension area where safe.
- Existing published table versions are immutable; edits create a new version.
- An overlay explicitly references a parent and records additions/replacements. Overrides are visible and auditable.
- No user table may impersonate a trusted official namespace.

## Embedding and portability

A WX-1 dataset records exact table package identities and content digests. To preserve meaning offline, it embeds the definitions required to interpret stored variables or embeds a content-addressed immutable snapshot. External lookup is an optimization, never the sole source of essential metadata.

## Licensing

The WX-1 software license does not automatically apply to imported WMO, national meteorological service, commercial or research tables. Import, redistribution and embedding must respect the source license. Table packages carry their own license and attribution metadata. A reader may use a table locally without implying permission to redistribute it.
