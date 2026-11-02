# WX-1 parameter registry — implementation milestone v0.3

Status: initial executable implementation, not yet a frozen interchange package schema.

## Implemented

- Immutable table identity: namespace + table ID + version.
- External code identity includes optional centre, subcentre, discipline and category plus the required code number.
- Canonical parameter ID is distinct from external code and display names.
- Provenance fields (publisher, source, license) are mandatory package inputs.
- CSV importer with quoted fields, escaped quotes, CRLF and multiline quoted values.
- Required CSV columns: `code`, `canonical_id`, `short_name`, `long_name`, `units`.
- Optional columns: `centre`, `subcentre`, `discipline`, `category`, `description`.
- Full-package validation before insertion; duplicate external keys and canonical IDs are rejected.
- Registry refuses to replace an existing exact table version. A changed table must be imported under a new version.
- Resolution uses the exact table identity and complete external code context; it never resolves by display name alone.

## Deliberate limitations

This release does not yet import JSON, YAML, XLSX, GRIB or NetCDF. It does not bundle WMO or provider tables, because source provenance and redistribution licenses must be verified independently. It does not yet implement overlay/override packages, semantic unit validation, signed packages, persistent registry storage, or a command-line importer. These are follow-up work, not silently assumed capabilities.

## CSV example

```csv
code,canonical_id,short_name,long_name,units,description
0,air_temperature,t,Air temperature,K,"Temperature, near surface"
```

Package identity and provenance are supplied through the API, not guessed from the file name or CSV contents.

## v0.3.1 incremental additions

- Deterministic CSV export emits a fixed column order and sorts rows by the complete external code tuple.
- CSV export quotes commas, quotes and line breaks using RFC-4180 escaping; import/export round-trip is tested.
- `diff_tables` compares two versions of the same namespace/table family and classifies added, removed and changed parameters. Matching uses the complete external code tuple, never names or canonical IDs.
- Registry table identities can be enumerated in deterministic order; installed table versions remain immutable.

These additions do not define JSON package serialization or overlay/override semantics. Those remain separate decisions.


## v0.3.2 additions

- `TableOverlay` explicitly names the exact base table version, target version, publisher, source, license and reason.
- `apply_overlay` creates a new package version; it never mutates the base package or replaces an installed version.
- An overlay may replace definitions only for existing complete external-code keys. It cannot introduce new external codes.
- Duplicate override keys are rejected. The resulting package is fully validated before it is returned.
- For overrides, the built-in unit vocabulary checks dimensional compatibility (for example K to degC is temperature-to-temperature; K to hPa is rejected). Unknown units remain preserved in ordinary source tables, but an override using an unknown unit is rejected rather than guessed.
- The vocabulary is deliberately conservative and is not a substitute for UDUNITS or a complete CF standard-name/unit system.
