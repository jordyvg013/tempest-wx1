# ADR 0028: Indexed Scientific Array Read Planner

- Status: Accepted for implementation (Fase 4)
- Date: 2026-11-02
- Authority: `WX1_ROADMAP_CURRENT.md` (Fase 4, Order 1), `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md` (§4.8)

## Context

In WX-1 v0.4.3, `plan_array_slice` linearly iterated over all `variable.chunks` to find intersecting chunks. While fast for small arrays ($5.0\ \mu\text{s}$ at 4,096 chunks), formal baseline measurements confirmed this causes severe scalability degradation on large grids ($167.0\ \mu\text{s}$ at 65,536 chunks and $6,682.0\ \mu\text{s}$ [6.7 ms] at 1,048,576 chunks), even when the query touches only 4 chunks.

## Decision

1. **Implement Direct Coordinate Grid Mapping ($O(k)$):**
   When a variable is densely and canonically chunked on regular grid boundaries, compute the intersecting chunk index ranges directly from selection bounds:
   $$i_{d,\min} = \lfloor \text{start}_d / \text{chunk\_shape}_d \rfloor$$
   $$i_{d,\max} = \lfloor (\text{start}_d + \text{count}_d - 1) / \text{chunk\_shape}_d \rfloor$$
2. **Direct Row-Major Linear Coordinate Calculation:**
   Iterate only over the sub-grid of intersecting chunks ($k$ elements) and calculate the direct chunk index in $O(1)$ arithmetic steps.
3. **Transparent Fallback:**
   If a variable uses non-uniform chunking or ragged boundaries, fallback to linear intersection checking, guaranteeing bit-exact compatibility across all schema v2 datasets.
4. **Checked Arithmetic and Zero-Allocation:**
   Use checked arithmetic for all index strides and pre-allocate the output vector with capacity $k$.

## Consequences

- Array slice planning for localized region queries (e.g. 100x100 or single-point time series) becomes $O(k)$ instead of $O(N)$.
- Planning time on 1,048,576 chunks drops from 6.7 ms to $< 0.010\text{ ms}$ (a $> 500\times$ speedup).
- The planning overhead becomes negligible compared to chunk I/O and decompression.
