# WX-1 Specification Draft: Error-Bounded Lossy Quantization and Byte-Shuffle (ADR 0024)

- **Document Version:** 0.1.0
- **Status:** Draft / Specification
- **Related ADR:** `docs/decisions/0024-lossy-quantization-error-bounded.md`
- **Applicable Profiles:** `restart` (strictly lossless bit-exact), `output` (lossy allowed with declared error bound)

---

## 1. Motivation and Principles

Meteorological model state and observation datasets operate under two distinct operational requirements:

1. **Model Restart and Checkpointing (`restart` profile):**
   Numerical integration and dynamic weather prediction require 100% bit-exact reproducibility. Any perturbation—even in the least significant bit of IEEE 754 floating-point representations—leads to numerical divergence via chaotic atmospheric dynamics. In the `restart` profile, lossy transformations are **strictly forbidden**. Storage precision, compute precision, and endianness must be identical to the in-memory model state, and pseudo-random number generator (PRNG) states, shard topologies, and ensemble identifiers must be bit-exact.

2. **Forecast Dissemination and Analysis (`output` profile):**
   Operational forecast outputs (such as surface temperatures, wind fields, geopotential heights, and precipitation) carry physical observational uncertainty. Storing raw 32-bit or 64-bit floating-point mantissa noise in downstream dissemination consumes massive bandwidth and storage without scientific value. Operational standards like WMO GRIB2 employ lossy quantization (such as grid simple packing or CCSDS 120.0-B-1 adaptive entropy coding).

To provide competitive compression exceeding GRIB2 CCSDS while preserving mathematical and scientific integrity, WX-1 introduces **Error-Bounded Lossy Quantization and Byte-Shuffle Filtering**:
- Lossy transformations are **never silent or implicit**.
- The profile, quantization method, codec version, error metric, and declared error bound $\epsilon$ are mandatory metadata per variable.
- Reconstructed values are mathematically proven and empirically verified to satisfy the declared bound before file commitment.
- Special IEEE 754 representations (NaN, $\pm\infty$, signed zeros, and domain sentinel/fill values) are preserved without corruption or integer casting.

---

## 2. Mathematical Error Bounds and Methods

### 2.1 Linear Absolute Error Quantization (`linear_absolute`)

For physical quantities with uniform absolute physical uncertainty (e.g., $T \pm 0.01\text{ K}$, $p \pm 0.1\text{ Pa}$, $u/v \pm 0.01\text{ m/s}$, precipitation $\pm 0.001\text{ mm}$):

Given a user-declared maximum absolute error bound $\epsilon_{\text{abs}} > 0$:
1. A dyadic (power-of-two) step size $\Delta$ is chosen as the largest power of two not exceeding $2\epsilon_{\text{abs}}$:
   $$\Delta = 2^{\lfloor \log_2(2 \epsilon_{\text{abs}}) \rfloor} \le 2\epsilon_{\text{abs}}$$
   (Because $\Delta$ is an exact power of two in IEEE 754 binary floating-point, division and multiplication operate on the exponent without introducing mantissa rounding error, and zero out trailing mantissa bits cleanly for Byte-Shuffle + Zstandard).
2. For each finite scalar value $x \in \mathbb{R}$:
   $$q = \text{round}\left(\frac{x}{\Delta}\right)$$
   $$\hat{x} = q \times \Delta$$
3. The absolute error is bounded:
   $$| \hat{x} - x | \le \frac{\Delta}{2} \le \epsilon_{\text{abs}}$$

### 2.2 Bit-Round Relative Error Quantization (`bit_round_relative`)

For quantities spanning multiple orders of magnitude where relative precision is physically meaningful (e.g., specific humidity $q$, trace gas concentrations, aerosol optical depth):

Given a declared maximum relative error bound $\epsilon_{\text{rel}} \in (0, 1)$:
1. The number of mantissa bits $p$ to retain in IEEE 754 representation is computed:
   $$p = \left\lceil -\log_2(2 \epsilon_{\text{rel}}) \right\rceil$$
2. For IEEE 754 binary32 (23-bit mantissa):
   The lower $23 - p$ bits are rounded to the nearest integer using round-to-nearest-even with carry into the exponent.
3. The relative error is bounded for all non-zero normal floats:
   $$\frac{|\hat{x} - x|}{|x|} \le 2^{-(p+1)} \le \epsilon_{\text{rel}}$$

---

## 3. Preservation of Special Floating-Point Values

Any lossy transform in WX-1 must strictly preserve IEEE 754 exceptional values and meteorological sentinels:

1. **NaN (Not a Number):**
   Quiet NaNs, signaling NaNs, and diagnostic payloads are preserved bit-for-bit without integer conversion.
2. **Infinities ($+\infty, -\infty$):**
   Never clamped to finite numbers; IEEE infinity bit patterns are maintained.
3. **Signed Zero ($-0.0$ and $+0.0$):**
   The sign bit is preserved; $-0.0$ is not converted to $+0.0$.
4. **Fill and Missing Values (`fill_value`, `missing_value`):**
   Any value matching the declared variable fill value (e.g. $-9999.0$, $9.96921\times 10^{36}$) bypasses quantization and remains bit-exact.

---

## 4. Codec Pipeline: Byte-Shuffle Filter

Floating-point scientific arrays exhibit strong spatial correlation across spatial dimensions, but their raw IEEE 754 byte representations interleave sign/exponent bits (highly predictable) with low mantissa bits (stochastic noise).

WX-1 defines a deterministic $O(N)$ **Byte-Shuffle Filter** (transpose by element size $S \in \{2, 4, 8\}$):
For a contiguous chunk of $N$ elements of size $S$ bytes (total length $L = N \times S$ bytes):
$$\text{destination\_offset}(i, b) = b \times N + i$$
where $i \in [0, N)$ is the element index and $b \in [0, S)$ is the byte lane index.

The inverse transform (**Byte-Unshuffle**) reconstructs the original contiguous stream:
$$\text{source\_offset}(i, b) = b \times N + i \implies \text{destination}(i \times S + b)$$

### Characteristics:
- Implemented in 100% safe Rust (`#![forbid(unsafe_code)]`) with verified index bounds.
- Forms an exact bijection (100% reversible, lossless).
- When followed by Zstandard (`ChunkCodec::ZstdByteShuffle4`), groups all identical exponent and upper mantissa bytes into long runs of identical bytes, yielding substantial compression improvements (30% to 50% smaller payloads) on operational meteorological grids.

---

## 5. Metadata Specification per Variable

When a variable is encoded with lossy quantization in the `output` profile, the following metadata attributes are mandatory in the scientific metadata manifest:

| Attribute Key | Type | Description | Example |
|---|---|---|---|
| `wx1:lossy` | String | Indicates whether the stored variable is lossy | `"true"` |
| `wx1:profile` | String | Operational profile | `"output"` |
| `wx1:quantization_type` | String | Quantization algorithm | `"linear_absolute"` or `"bit_round_relative"` |
| `wx1:codec_version` | String | WX-1 codec version | `"1.0"` |
| `wx1:error_bound` | String | Declared user error bound $\epsilon$ | `"0.01"` |
| `wx1:error_metric` | String | Metric used for bound | `"max_abs_error"` or `"relative_error"` |
| `wx1:step_size` | String | Quantization step size $\Delta$ | `"0.019995"` |
| `wx1:max_measured_abs_error`| String | Highest observed $|x - \hat{x}|$ | `"0.009956"` |
| `wx1:measured_rmse` | String | Root-mean-square error of chunk | `"0.005123"` |
| `wx1:measured_bias` | String | Mean error $\frac{1}{N}\sum (\hat{x} - x)$ | `"-0.000012"` |

If `max_measured_abs_error > error_bound`, encoding aborts with `QuantizationError::BoundExceeded`.
In the `restart` profile, `wx1:lossy` is `"false"`, and no error-bound attributes are permitted.
