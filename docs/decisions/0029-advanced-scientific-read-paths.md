# ADR 0029 — Advanced Scientific Read Paths (R1–R9) and Dual-Chunking Layouts

## Status
Accepted (Fase 4, Onderdeel 6)

## Context
Operational numerical weather prediction (NWP), climate modeling, and meteorological machine learning exhibit starkly contrasting data access profiles:
1. **Synoptic Analysis & 2D Slicing (R2, R5)**: Reading horizontal spatial grids at fixed time and elevation (e.g. $(y, x)$ maps or regional bounding boxes like the Netherlands).
2. **Point Forecasting & Station Verification (R4, R8)**: Extracting vertical atmospheric sounding profiles $(l)$ and multi-step time series $(t)$ at discrete geographical points $(y_i, x_i)$, such as airports or automatic weather stations.
3. **Ensemble & Volume Mining (R3, R6, R7)**: Slicing single ensemble members $(e)$ across 50 members, or vertical levels $(l)$ across all forecast timesteps $(t)$.
4. **Cubed-Sphere & Non-Regular Topologies (R9)**: Extracting individual panel faces from $C_N$ cubed-sphere grids without loading the remaining 5 panels.

Under traditional formats, reading a time series of 100 timesteps at one point from a file chunked along $(1, 1, 256, 256)$ forces the reader to decode $100 \times 65{,}536 = 6.55\text{ million}$ floating-point numbers merely to consume 100 values (a 99.998% I/O amplification penalty). Conversely, chunking exclusively for time series destroys 2D spatial map rendering performance.

Furthermore, naive evaluation of 1,000 station queries (R8) by making 1,000 independent slice calls leads to catastrophic repetitive chunk decompression.

## Decision
1. **Point-Scatter Batching Engine (`read_points`)**:
   Implement a specialized batch point reader in `ScientificSnapshot`:
   - Inputs a slice of $K$ multi-dimensional coordinates $[(c_{0}, c_{1}, \dots, c_{d-1})]_k$.
   - Projects each coordinate into its enclosing chunk index and local within-chunk stride offset.
   - Groups point requests by unique chunk index.
   - Decodes each intersecting chunk **exactly once** using parallel batch streaming (`read_chunks_parallel`).
   - Gathers requested points directly into the output vector, preserving query index ordering.

2. **Standardized Read Path Matrix (R1–R9)**:
   Formalize and guarantee algorithmic complexity for all 9 canonical NWP read paths:
   - **R1 (Container Open Latency)**: $O(1)$ lazy open verifying superblock and footer digest; independent of total container size (identical latency on 1 GB vs 100 GB).
   - **R2 (Single 2D Field)**: Contiguous horizontal slice read decoding only intersecting spatial chunks in parallel via `std::thread::scope`.
   - **R3 (Single Level across All Timesteps)**: Strided temporal slice decoding only $(t, \text{level})$ chunks.
   - **R4 (Point Profile Time-Series)**: Vertical column extraction along $(t, l)$ at fixed $(y, x)$, fetching only vertical column chunks.
   - **R5 (Regional Bounding Box)**: Bounded geographic subgrid $(y_0..y_1, x_0..x_1)$ extracting only intersecting tile boundary chunks.
   - **R6 (Full Array Scan)**: Sequential / streaming multi-threaded traversal across all variable chunks.
   - **R7 (Ensemble Member Extraction)**: Selective slicing of member $e$ across $M$ ensemble members without touching members $0..e-1, e+1..M-1$.
   - **R8 (Random Point Scatter)**: Batch-planned 1,000 station query executing in $O(U)$ chunk decodes where $U \le \min(1000, N_{\text{chunks}})$.
   - **R9 (Cubed-Sphere Panel Slice)**: Face index slice $p \in [0..5]$ loading exactly 1 panel face of $C_N$ topology.

3. **Dual-Chunking and Alternate Layout Manifests**:
   Enable variables to declare alternate chunking representations within the dataset metadata (e.g. `/t2m` with spatial chunking $(1, 256, 256)$ and `/t2m_timeseries` with temporal chunking $(100, 16, 16)$). Readers select the representation minimizing total decompressed byte volume for their target access profile.

4. **Wire Compatibility & Backward Compatibility**:
   - No breaking changes to the container header, superblock, or chunk format.
   - Point-scatter extraction and advanced read paths operate on existing v0.4.0 chunk containers.
   - Readers lacking ADR 0029 point-scatter optimizations fall back to standard bounding-box slice reads without data loss.

## Consequences
- Single-point time series and station verification queries (R4, R8) eliminate redundant I/O amplification.
- 1,000 station scatter queries execute in milliseconds rather than seconds.
- Existing WX-1 archives remain 100% bit-exact and forward/backward compatible.
