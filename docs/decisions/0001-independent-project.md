# ADR 0001: WX-1 is an independent project

- Status: Accepted
- Date: 2026-10-29

## Context

WX-1 was initially developed inside a combined Tempest workspace. Its purpose, however, is a language-independent scientific storage format and engine. Keeping it inside the Tempest repository risks coupling the file format, release cadence, dependencies, and public interfaces to one programming language.

## Decision

WX-1 receives its own repository/workspace, specification, CLI, tests, versioning, and release process. Tempest will later consume WX-1 through a documented API or file-format implementation, like any other client. The WX-1 wire format must remain independent of Rust and Tempest internals.

The existing `WX1X` eight-byte prefix is retained only as a legacy experimental probe for continuity with the prototype. It is explicitly not normative, not stable, and not a complete file signature. No byte-level design decision is frozen by this ADR.

## Consequences

- The standalone workspace contains only WX-1 crates and documentation.
- Tempest-specific crates and examples are excluded.
- Important wire-format decisions remain open until separate ADRs and a reviewed specification exist.
- Compatibility and format versioning are managed by WX-1 itself.
