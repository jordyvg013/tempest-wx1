# WX-1 Indexed Scientific Array Read Planner Specification Draft v0.1

- **Authority:** `WX1_ROADMAP_CURRENT.md` (Fase 4, Order 1), `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md` (§4.8)
- **ADR Reference:** `docs/decisions/0028-indexed-array-read-planner.md`
- **Status:** Draft / Implementation

## 1. Context and Problem Statement

The WX-1 scientific dataset reader allows querying arbitrary multidimensional array slices via `read_array_slice(variable_path, selection)`.

Prior to this specification, the read planning step in `plan_array_slice` used an $O(N)$ linear scan over all `variable.chunks`:
```rust
for chunk in &variable.chunks {
    if intersects(selection, chunk) {
        chunk_ids.push(chunk.chunk_id);
    }
}
```

### Formal Linear Baseline Measurements (2026-11-02, Release Profile)
Querying a slice intersecting $k = 4$ chunks:
- **4,096 chunks ($64 \times 64$ grid):** median **0.005 ms** ($5.0\ \mu\text{s}$)
- **65,536 chunks ($256 \times 256$ grid):** median **0.167 ms** ($167.0\ \mu\text{s}$)
- **1,048,576 chunks ($1024 \times 1024$ grid):** median **6.682 ms** ($6,682.0\ \mu\text{s}$, p95: $7,348.0\ \mu\text{s}$)

At 1,048,576 chunks, planning alone consumes **6.7 ms** for a 4-chunk slice, dwarfing the physical read time (0.27 ms).

## 2. Technical Architecture for $O(k)$ Planning

### 2.1 Grid Regularity Detection
A variable's chunks are *grid-regular* if:
1. Every dimension $d \in [0, D)$ has a uniform chunk stride $C_d = \text{chunk\_shape}[d] > 0$.
2. Every chunk origin aligns with a multiple of the chunk shape: $\text{origin}[d] = i_d \times C_d$.
3. The variable covers the dense grid of chunks in canonical row-major order:
   $$\text{linear\_index}(i_0, \dots, i_{D-1}) = \sum_{d=0}^{D-1} i_d \prod_{m=d+1}^{D-1} M_m$$
   where $M_d = \lceil \text{dim\_len}[d] / C_d \rceil$.

### 2.2 $O(k)$ Index Resolution
For a selection with bounds $[S_d, S_d + K_d)$ along dimension $d$:
1. The intersecting chunk range along dimension $d$ is:
   $$i_{d,\min} = \lfloor S_d / C_d \rfloor$$
   $$i_{d,\max} = \lfloor (S_d + K_d - 1) / C_d \rfloor$$
2. The total intersecting chunks count is:
   $$k = \prod_{d=0}^{D-1} (i_{d,\max} - i_{d,\min} + 1)$$
3. Directly compute the linear index for each tuple $(i_0, \dots, i_{D-1})$ in the bounding box:
   $$\text{idx} = \text{linear\_index}(i_0, \dots, i_{D-1})$$
4. Retrieve `variable.chunks[idx].chunk_id` in $O(1)$ per intersecting chunk.

Total planner time complexity is strictly $O(k)$, independent of $N$.

### 2.3 Fallback for Irregular Chunks
If chunk origins or shapes are irregular (e.g., adaptive mesh refinement or sparse ragged tiles), fallback to spatial interval traversal or B+tree secondary index lookup.

## 3. Constraints and Invariants
1. **Bit-exact result:** The resulting `ArrayReadPlan` must produce the identical sequence of `chunk_ids` and `output_bytes` as the linear planner.
2. **Bounds checking:** All index calculations use checked arithmetic (`checked_add`, `checked_mul`, `checked_div`). Overflow immediately returns `ScientificModelError::InvalidSelection` or `LengthOverflow`.
3. **Safe Rust:** Zero unsafe code (`#![forbid(unsafe_code)]`).
