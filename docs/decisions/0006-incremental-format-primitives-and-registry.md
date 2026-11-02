# ADR 0006: Incremental primitives and parameter registry

- Status: Accepted for v0.3 implementation scope
- Date: 2026-10-29

## Context

v0.2 recorded the architecture but did not provide executable format primitives or registry behavior. The next increment should implement bounded, independently testable pieces without prematurely freezing the file bytes or claiming official tables are bundled.

## Decisions

1. Add explicit-endian, bounds-checked primitive decoding to `wx1-format`.
2. Add a separate `wx1-registry` crate to keep parameter semantics out of the binary parser.
3. Registry identity is namespace/table/version; code resolution also includes provider/centre context when present.
4. Table versions are immutable after insertion. Replacements require a new version.
5. CSV is the first importer. JSON and spreadsheet formats are deferred until a versioned package schema is agreed.
6. Do not bundle third-party tables without verified provenance and redistribution terms.
7. Do not define final file header/footer bytes in this milestone.

## Consequences

The workspace now has real testable code for primitive decoding and in-memory parameter table validation/import. It still cannot read or write a WX-1 data file.
