# WX-1 Appendable Scientific Datasets Specification Draft (v0.1)

- **Date:** 2026-11-01
- **Status:** Draft / Specification for ADR 0022 implementation (Fase 3)
- **Authority:** `WX1_ROADMAP_CURRENT.md`, `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md`

---

## 1. Context and Problem Statement

WX-1 v0.4.3 supports append-only generations with persistent Copy-on-Write (CoW) B+tree indexes for raw chunk storage (ADR 0014, ADR 0015). However, scientific datasets were historically constrained to generation 1:
- `append_generation` explicitly returned `ChunkIoError::ScientificModelCannotAppend` if a scientific model was present.
- `validate_generation_candidate` explicitly failed with `ChunkIoError::ScientificModelMultipleGenerations` if `generation != 1`.

In operational meteorology (e.g., ECMWF IFS, KNMI HARMONIE-AROME, and NWP simulation models), forecasts and continuous observation streams produce data incrementally over time. Requiring a complete dataset rewrite for every new forecast timestep is prohibitive for large grids.

---

## 2. Normative Specification

### 2.1 Multi-Generation Scientific Model Evolution
1. A scientific container MAY contain multiple committed generations ($G = 1, 2, \dots, K$).
2. The `ScientificModelMultipleGenerations` error check is lifted for validly chained generations.
3. Each generation $G$ commits:
   - Zero or more new chunk payloads (appended after the footer of generation $G-1$).
   - A generation directory v2 describing the newly committed chunks of generation $G$.
   - An updated generation metadata payload carrying the updated `ScientificDataset` schema.
   - Updated CoW B+tree indexes (including the mandatory chunk-ID tree and optional variable/time/spatial indexes).
   - An 80-byte `WX1G` generation footer pointing back to generation $G-1$ via `prev_footer_offset`.

### 2.2 Time Dimension Growth Semantics
1. When appending timesteps to a scientific array:
   - The length of the dimension with role `DimensionRole::Time` (or `ForecastReferenceTime`) increases monotonically: $L_{\text{time}}^{(G)} = L_{\text{time}}^{(G-1)} + \Delta t$.
   - Existing spatial and vertical dimension lengths MUST remain unchanged unless an explicit nested grid / domain expansion is declared.
   - Newly committed chunks MUST carry origins corresponding to the newly added time coordinates.
   - Chunks committed in previous generations ($1 \dots G-1$) MUST NOT be rewritten or modified.

### 2.3 Read Across Generation Boundaries
1. `read_array_slice` MUST transparently read and assemble slices that span multiple committed generations.
2. The read planner resolves chunk IDs from the generation snapshot's combined descriptor map across all committed generations up to $G$.
3. Reads remain strictly pinned to the opened snapshot's generation; concurrent or subsequent appends do not affect active readers.

### 2.4 Atomic Commit and Crash Recovery (C1 Guarantee)
1. Incomplete writes (e.g. process termination via `kill -9` or power loss during chunk write) leave the file truncated or unsealed after generation $G-1$'s footer.
2. The reverse recovery scan detects the latest valid footer of generation $G-1$.
3. All data and metadata in generations $1 \dots G-1$ remain completely intact, valid, and bit-exact.
