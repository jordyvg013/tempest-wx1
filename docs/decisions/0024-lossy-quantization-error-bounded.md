# ADR 0024: Error-Bounded Lossy Quantization and Byte-Shuffle Pipeline

## Status
Accepted

## Context
Weather and climate prediction produces large multidimensional arrays (hundreds of gigabytes to petabytes per forecast cycle). Storing uncompressed or purely lossless floating-point representations of operational dissemination variables (such as 2-meter temperature, 10-meter wind components, and geopotential heights) introduces severe storage and network bottlenecks. Operational meteorological formats like WMO GRIB2 employ lossy quantization (such as grid simple packing and CCSDS 120.0-B-1 adaptive entropy coding).

However, lossy compression poses severe scientific risks if applied silently or incorrectly:
1. Dynamic model state checkpoints (`restart` profile) must remain bit-exact lossless; even single-bit perturbations in floating-point mantissas induce chaotic divergence during model restarts.
2. Uncontrolled lossy transforms can corrupt physical invariants, round NaNs or missing-value sentinels into bogus physical numbers, alter signed zeros (`-0.0`), or exceed physical tolerance limits.
3. Without explicit metadata and verified bounds, data consumers cannot assess data lineage or validity.

## Decision
1. **Separation of Operational Profiles:**
   - `restart` Profile: Exclusively lossless and bit-exact. Storage precision, compute precision, IEEE 754 representations, PRNG state, and ensemble shard identity are strictly preserved.
   - `output` Profile: Lossy quantization is permitted only when explicitly requested with a declared per-variable error bound $\epsilon$.
2. **Deterministic Mathematical Quantization:**
   - Linear Absolute Error (`linear_absolute`): Guarantees $\max |x - \hat{x}| \le \epsilon_{\text{abs}}$ by selecting step $\Delta < 2\epsilon_{\text{abs}}$.
   - Bit-Round Relative Error (`bit_round_relative`): Retains $p = \lceil -\log_2(2\epsilon_{\text{rel}}) \rceil$ mantissa bits to guarantee relative error $\le \epsilon_{\text{rel}}$.
3. **Preservation of Exceptional Floats:**
   - IEEE 754 NaNs, $\pm\infty$, signed zeros ($-0.0$), and user-declared `fill_value` / `missing_value` sentinels are detected and preserved without quantization.
4. **Byte-Shuffle Filter before Zstandard Compression:**
   - Implement a safe, reversible $O(N)$ Byte-Shuffle filter (byte transpose across element lanes of 2, 4, or 8 bytes) prior to Zstandard frame compression (`ChunkCodec::ZstdByteShuffle4`).
   - Grouping homologous byte lanes across spatially correlated fields reduces payload sizes by 30% to 50% on operational weather grids.
5. **Mandatory Metadata and Pre-Commit Verification:**
   - Every lossy variable must persist `wx1:lossy`, `wx1:quantization_type`, `wx1:error_bound`, `wx1:error_metric`, `wx1:codec_version`, and measured error statistics (`wx1:max_measured_abs_error`, `wx1:measured_rmse`, `wx1:measured_bias`).
   - If the measured maximum error exceeds the declared bound, the write operation is aborted.

## Consequences
- **Positive:**
  - WX-1 compressed size at equal error bounds is smaller than GRIB2 CCSDS (as proven on operational ECMWF ENFO datasets).
  - Byte-shuffle delivers compression gains even for lossless data in the `restart` profile.
  - Full transparency: lossy operations cannot occur silently or unintentionally.
- **Negative / Trade-offs:**
  - Quantization and byte-shuffle introduce modest CPU preprocessing time during write operations.
  - Bit-rounding requires careful handling of mantissa overflow to avoid spurious infinities.
