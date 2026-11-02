# ADR 0022: Appendable Scientific Datasets

- Status: Accepted for implementation (Fase 3)
- Date: 2026-11-01
- Authority: `WX1_ROADMAP_CURRENT.md`, `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md`

## Context

WX-1 v0.4.3 supports append-only generations for generic chunks (ADR 0014, ADR 0015), but scientific datasets were restricted to generation 1. Operational weather forecasting and climate model simulations require appending forecast timesteps incrementally without rewriting existing data.

## Decision

1. **Permit Multi-Generation Scientific Datasets:**
   Lift the restrictions that forbade appending to scientific datasets (`ScientificModelCannotAppend`) and that rejected scientific datasets with `generation != 1` (`ScientificModelMultipleGenerations`).
2. **Metadata Evolution per Generation:**
   Allow each committed generation to update the `ScientificDataset` model, specifically expanding the `time` dimension length and registering new `ArrayChunkRef`s for new forecast steps.
3. **Atomic Append Path:**
   Implement `append_scientific_generation` and `append_scientific_generation_with_indexes`.
   The writer appends new chunks, writes an updated directory and index catalog, writes an updated generation footer pointing to the previous generation footer, and flushes durably.
4. **Crash-Consistency (C1):**
   If an append is interrupted before the generation footer is durably synced, recovery automatically rolls back to the previous committed generation without data loss.

## Consequences

- NWP models can stream forecast timesteps into a single WX-1 file as they are computed.
- Zero data rewrite: existing chunks remain bit-exact and immutable.
- Readers pinned to generation $N$ continue reading safely while generation $N+1$ is being appended.
- Transparent cross-generation slice reads: `read_array_slice` handles slices that span chunks from multiple generations.
