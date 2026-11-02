# Contributing to WX-1

WX-1 welcomes contributions to the specification, Rust implementation, Python bindings, interoperability adapters, parameter registries, documentation and tests.

## Before contributing

1. Read `AGENTS.md`, `README.md`, the relevant specification draft and ADRs.
2. For a wire-format change, open or update an ADR and explain compatibility impact before changing bytes.
3. Keep the format language-, platform-, CPU-, GPU- and codec-independent.
4. Add tests for valid input, malformed input, truncation, limits, overflow and compatibility as applicable.
5. Do not add third-party parameter tables or sample datasets without checking redistribution rights and preserving attribution/license metadata.

## Development expectations

- Prefer safe Rust; `unsafe` is forbidden unless the project explicitly changes that rule through review.
- CPU functionality must remain available without GPU libraries or toolchains.
- GPU work must be optional and bounded by explicit memory budgets.
- Lossless storage is the default. Lossy transforms require explicit user opt-in and error metadata.
- Do not claim benchmarks or compatibility that have not been measured and tested.
- Run formatting, tests and Clippy before submitting Rust changes; report any checks that could not be run.

## Issues and pull requests

Describe the use case, expected behavior, test coverage, compatibility implications and any external standards or licenses involved. Large design changes should start with an issue or ADR discussion.
