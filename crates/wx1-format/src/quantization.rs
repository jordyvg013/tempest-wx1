// Copyright 2026 Jordy van Gils
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Error-bounded lossy quantization and byte-shuffle filters (ADR 0024).
//!
//! Numerical weather prediction and climate models have strict operational fidelity requirements:
//! - `restart` profile: strictly lossless and bit-exact (no lossy transforms permitted).
//! - `output` profile: error-bounded lossy quantization allowed with declared tolerances.
//!
//! This module provides deterministic, safe Rust implementations of:
//! 1. `linear_absolute`: Max absolute error $\le \epsilon_{\text{abs}}$.
//! 2. `bit_round_relative`: Max relative error $\le \epsilon_{\text{rel}}$.
//! 3. Preservation of exceptional values: NaNs, Infinities, Signed Zeroes, Fill values.
//! 4. Invertible $O(N)$ Byte-Shuffle / Unshuffle filters for enhanced Zstandard compression.

use crate::ChunkIoError;
use std::fmt;

/// Metrics for evaluating lossy quantization error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorMetric {
    /// Guaranteed absolute error bound: $|x - \hat{x}| \le \epsilon$.
    Absolute,
    /// Guaranteed relative error bound: $\frac{|x - \hat{x}|}{|x|} \le \epsilon$.
    Relative,
}

impl fmt::Display for ErrorMetric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absolute => write!(f, "max_abs_error"),
            Self::Relative => write!(f, "relative_error"),
        }
    }
}

/// Operational profile for dataset variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantizationProfile {
    /// Model restart / checkpoint profile: strictly lossless and bit-exact.
    Restart,
    /// Output dissemination profile: lossy compression permitted with declared bound.
    Output,
}

/// Configuration for error-bounded lossy quantization.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuantizationConfig {
    /// Target error bound $\epsilon > 0$.
    pub error_bound: f64,
    /// Metric to constrain.
    pub metric: ErrorMetric,
    /// Whether exceptional IEEE values (NaN, Inf, -0.0, fill_value) are preserved.
    pub preserve_specials: bool,
}

impl QuantizationConfig {
    /// Creates a new configuration for absolute error bound (e.g. $T \pm 0.01$ K).
    pub fn absolute(error_bound: f64) -> Result<Self, QuantizationError> {
        if error_bound <= 0.0 || error_bound.is_nan() || error_bound.is_infinite() {
            return Err(QuantizationError::InvalidErrorBound(
                "error bound must be positive and finite".to_string(),
            ));
        }
        Ok(Self {
            error_bound,
            metric: ErrorMetric::Absolute,
            preserve_specials: true,
        })
    }

    /// Creates a new configuration for relative error bound (e.g. $10^{-4}$ for humidity).
    pub fn relative(error_bound: f64) -> Result<Self, QuantizationError> {
        if error_bound <= 0.0 || error_bound >= 1.0 || error_bound.is_nan() {
            return Err(QuantizationError::InvalidErrorBound(
                "relative error bound must be in the open interval (0, 1)".to_string(),
            ));
        }
        Ok(Self {
            error_bound,
            metric: ErrorMetric::Relative,
            preserve_specials: true,
        })
    }
}

/// Report containing measured empirical error statistics for verification against declared bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuantizationReport {
    /// Declared error bound.
    pub declared_bound: f64,
    /// Step size used during quantization (if linear).
    pub step_size: f64,
    /// Maximum observed absolute error across all valid values.
    pub max_measured_abs_error: f64,
    /// Maximum observed relative error across all non-zero valid values.
    pub max_measured_rel_error: f64,
    /// Root-mean-square error (RMSE).
    pub rmse: f64,
    /// Mean error (bias: $\frac{1}{N}\sum (\hat{x} - x)$).
    pub bias: f64,
    /// Count of special values preserved without quantization (NaN, Inf, fill values).
    pub special_count: u64,
    /// Total number of elements processed.
    pub total_count: u64,
}

/// Errors occurring during quantization and filtering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuantizationError {
    /// Declared error bound is non-positive, NaN, or out of range.
    InvalidErrorBound(String),
    /// Quantization would violate the declared error bound.
    BoundExceeded { declared: String, measured: String },
    /// Memory allocation failed.
    AllocationFailed,
    /// Quantization is forbidden in the `restart` profile.
    LossyForbiddenInRestart,
    /// Integer conversion overflow.
    IntegerOverflow,
    /// Array length or item size invalid.
    InvalidDimensions,
}

impl fmt::Display for QuantizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidErrorBound(msg) => write!(f, "invalid error bound: {msg}"),
            Self::BoundExceeded { declared, measured } => {
                write!(
                    f,
                    "quantization error exceeded declared bound: declared {declared}, measured {measured}"
                )
            }
            Self::AllocationFailed => write!(f, "allocation failed during quantization"),
            Self::LossyForbiddenInRestart => {
                write!(
                    f,
                    "lossy quantization is strictly forbidden in restart profile"
                )
            }
            Self::IntegerOverflow => write!(f, "integer overflow during quantization"),
            Self::InvalidDimensions => write!(f, "invalid array dimensions or item size"),
        }
    }
}

impl std::error::Error for QuantizationError {}

/// Invertible $O(N)$ Byte-Shuffle filter.
///
/// Transposes contiguous elements of size `item_size` bytes so that homologous
/// byte lanes (e.g. all sign/exponent bytes, all middle mantissa bytes, all low mantissa bytes)
/// are grouped together into contiguous runs.
pub fn byte_shuffle(input: &[u8], item_size: usize) -> Result<Vec<u8>, ChunkIoError> {
    if item_size <= 1 || input.is_empty() {
        return Ok(input.to_vec());
    }
    let n = input.len() / item_size;
    let remainder = input.len() % item_size;
    let total_elements_bytes = n
        .checked_mul(item_size)
        .ok_or(ChunkIoError::LengthOverflow)?;

    let mut output = Vec::new();
    output
        .try_reserve_exact(input.len())
        .map_err(|_| ChunkIoError::AllocationFailed)?;
    output.resize(input.len(), 0);

    for b in 0..item_size {
        let out_offset = b * n;
        for i in 0..n {
            output[out_offset + i] = input[i * item_size + b];
        }
    }

    if remainder > 0 {
        output[total_elements_bytes..].copy_from_slice(&input[total_elements_bytes..]);
    }
    Ok(output)
}

/// Inverts a Byte-Shuffle filter, restoring the original interleaved representation.
pub fn byte_unshuffle(input: &[u8], item_size: usize) -> Result<Vec<u8>, ChunkIoError> {
    if item_size <= 1 || input.is_empty() {
        return Ok(input.to_vec());
    }
    let n = input.len() / item_size;
    let remainder = input.len() % item_size;
    let total_elements_bytes = n
        .checked_mul(item_size)
        .ok_or(ChunkIoError::LengthOverflow)?;

    let mut output = Vec::new();
    output
        .try_reserve_exact(input.len())
        .map_err(|_| ChunkIoError::AllocationFailed)?;
    output.resize(input.len(), 0);

    for b in 0..item_size {
        let in_offset = b * n;
        for i in 0..n {
            output[i * item_size + b] = input[in_offset + i];
        }
    }

    if remainder > 0 {
        output[total_elements_bytes..].copy_from_slice(&input[total_elements_bytes..]);
    }
    Ok(output)
}

/// Quantizes an `f32` slice with a guaranteed error bound.
///
/// Preserves NaNs, $\pm\infty$, signed zeros ($-0.0$), and optional fill values.
/// Returns the quantized array and a cryptographic/statistical verification report.
pub fn quantize_f32_slice(
    data: &[f32],
    config: &QuantizationConfig,
    fill_value: Option<f32>,
) -> Result<(Vec<f32>, QuantizationReport), QuantizationError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(data.len())
        .map_err(|_| QuantizationError::AllocationFailed)?;

    let mut special_count = 0u64;
    let mut max_abs_err = 0.0f64;
    let mut max_rel_err = 0.0f64;
    let mut sum_sq_err = 0.0f64;
    let mut sum_err = 0.0f64;
    let mut valid_count = 0u64;

    match config.metric {
        ErrorMetric::Absolute => {
            // Use the largest dyadic (power-of-two) step size <= 2 * error_bound:
            // Delta = 2^{floor(log2(2 * eps))}.
            // Because Delta is an exact power of two in IEEE 754 binary floating-point,
            // division and multiplication introduce zero mantissa rounding jitter and
            // zero out trailing mantissa bits for optimal Byte-Shuffle + Zstd compression.
            let k = (2.0 * config.error_bound).log2().floor();
            let step = 2.0f64.powf(k);
            if step <= 0.0 || step.is_nan() || step.is_infinite() {
                return Err(QuantizationError::InvalidErrorBound(
                    "computed step size must be positive and finite".to_string(),
                ));
            }

            // Guard against re-quantizing inputs that already lie on a shifted dyadic grid
            // with step >= Delta (such as GRIB2 fields anchored at a message referenceValue R).
            // Re-quantizing such data to a 0.0 anchor saves zero bits while introducing a
            // constant shift of (R mod step).
            if is_already_on_dyadic_step(data, step, config.preserve_specials, fill_value) {
                for &val in data {
                    if config.preserve_specials && is_special_or_fill_f32(val, fill_value) {
                        special_count += 1;
                    }
                    output.push(val);
                }
                return Ok((
                    output,
                    QuantizationReport {
                        declared_bound: config.error_bound,
                        step_size: step,
                        max_measured_abs_error: 0.0,
                        max_measured_rel_error: 0.0,
                        rmse: 0.0,
                        bias: 0.0,
                        special_count,
                        total_count: data.len() as u64,
                    },
                ));
            }

            for &val in data {
                if config.preserve_specials && is_special_or_fill_f32(val, fill_value) {
                    output.push(val);
                    special_count += 1;
                    continue;
                }

                if val == 0.0 {
                    // Preserve exact sign bit (+0.0 vs -0.0)
                    output.push(val);
                    valid_count += 1;
                    continue;
                }

                let val_f64 = val as f64;
                let q = (val_f64 / step).round();
                let recon = (q * step) as f32;
                output.push(recon);

                let diff = (recon as f64) - val_f64;
                let abs_err = diff.abs();
                if abs_err > max_abs_err {
                    max_abs_err = abs_err;
                }
                let val_abs = (val as f64).abs();
                if val_abs > 0.0 {
                    let rel_err = abs_err / val_abs;
                    if rel_err > max_rel_err {
                        max_rel_err = rel_err;
                    }
                }
                sum_sq_err += diff * diff;
                sum_err += diff;
                valid_count += 1;
            }

            if max_abs_err > config.error_bound {
                return Err(QuantizationError::BoundExceeded {
                    declared: format!("{}", config.error_bound),
                    measured: format!("{max_abs_err}"),
                });
            }

            let rmse = if valid_count > 0 {
                (sum_sq_err / valid_count as f64).sqrt()
            } else {
                0.0
            };
            let bias = if valid_count > 0 {
                sum_err / valid_count as f64
            } else {
                0.0
            };

            Ok((
                output,
                QuantizationReport {
                    declared_bound: config.error_bound,
                    step_size: step,
                    max_measured_abs_error: max_abs_err,
                    max_measured_rel_error: max_rel_err,
                    rmse,
                    bias,
                    special_count,
                    total_count: data.len() as u64,
                },
            ))
        }
        ErrorMetric::Relative => {
            // Bit-round: retain p mantissa bits where 2^{-(p+1)} <= error_bound.
            let p = ((-(2.0 * config.error_bound).log2()).ceil() as i32).clamp(1, 23) as u32;
            let shift = 23 - p;
            let half = 1u32 << (shift - 1);
            let mask = (0xFFFF_FFFFu32 << shift) & 0x7FFF_FFFF;

            for &val in data {
                if config.preserve_specials && is_special_or_fill_f32(val, fill_value) {
                    output.push(val);
                    special_count += 1;
                    continue;
                }

                if val == 0.0 {
                    output.push(val);
                    valid_count += 1;
                    continue;
                }

                let u32_bits = val.to_bits();
                let sign = u32_bits & 0x8000_0000;
                let mag = u32_bits & 0x7FFF_FFFF;
                let exp = (mag >> 23) & 0xFF;

                // Subnormal floats or exponents near limits are preserved
                if exp == 0 || exp >= 254 {
                    output.push(val);
                    valid_count += 1;
                    continue;
                }

                let rounded_mag = (mag + half) & mask;
                let recon = f32::from_bits(sign | rounded_mag);
                output.push(recon);

                let diff = (recon - val) as f64;
                let abs_err = diff.abs();
                if abs_err > max_abs_err {
                    max_abs_err = abs_err;
                }
                let val_abs = (val as f64).abs();
                let rel_err = abs_err / val_abs;
                if rel_err > max_rel_err {
                    max_rel_err = rel_err;
                }
                sum_sq_err += diff * diff;
                sum_err += diff;
                valid_count += 1;
            }

            if max_rel_err > config.error_bound {
                return Err(QuantizationError::BoundExceeded {
                    declared: format!("{}", config.error_bound),
                    measured: format!("{max_rel_err}"),
                });
            }

            let rmse = if valid_count > 0 {
                (sum_sq_err / valid_count as f64).sqrt()
            } else {
                0.0
            };
            let bias = if valid_count > 0 {
                sum_err / valid_count as f64
            } else {
                0.0
            };

            Ok((
                output,
                QuantizationReport {
                    declared_bound: config.error_bound,
                    step_size: 2.0f64.powi(-(p as i32)),
                    max_measured_abs_error: max_abs_err,
                    max_measured_rel_error: max_rel_err,
                    rmse,
                    bias,
                    special_count,
                    total_count: data.len() as u64,
                },
            ))
        }
    }
}

/// Checks whether an input slice already lies on a dyadic grid with step >= `step`
/// (even if shifted by a per-message GRIB2 `referenceValue` offset).
fn is_already_on_dyadic_step(
    data: &[f32],
    step: f64,
    preserve_specials: bool,
    fill_value: Option<f32>,
) -> bool {
    let mut prev: Option<f64> = None;
    let mut checked = 0usize;
    let mut exact = 0usize;
    for &val in data {
        if preserve_specials && is_special_or_fill_f32(val, fill_value) {
            continue;
        }
        if !val.is_finite() {
            continue;
        }
        let current = val as f64;
        if let Some(p) = prev {
            let diff = current - p;
            let ratio = diff / step;
            if (ratio - ratio.round()).abs() == 0.0 {
                exact += 1;
            }
            checked += 1;
            if checked >= 4096 {
                break;
            }
        }
        prev = Some(current);
    }
    checked >= 64 && exact * 100 >= checked * 99
}

/// Helper to identify exceptional values (NaN, Inf, or user fill_value).
fn is_special_or_fill_f32(val: f32, fill_value: Option<f32>) -> bool {
    if val.is_nan() || val.is_infinite() {
        return true;
    }
    if let Some(fill) = fill_value {
        if fill.is_nan() {
            return val.is_nan();
        }
        if val == fill {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_shuffle_and_unshuffle_are_exact_bijections() {
        for item_size in [2, 4, 8] {
            let data: Vec<u8> = (0..256).map(|b| b as u8).collect();
            let shuffled = byte_shuffle(&data, item_size).expect("shuffle succeeds");
            assert_eq!(shuffled.len(), data.len());
            let unshuffled = byte_unshuffle(&shuffled, item_size).expect("unshuffle succeeds");
            assert_eq!(unshuffled, data);
        }
    }

    #[test]
    fn byte_shuffle_handles_odd_lengths_with_remainder() {
        let data: Vec<u8> = (0..73).map(|b| b as u8).collect();
        let shuffled = byte_shuffle(&data, 4).expect("shuffle succeeds");
        let unshuffled = byte_unshuffle(&shuffled, 4).expect("unshuffle succeeds");
        assert_eq!(unshuffled, data);
    }

    #[test]
    fn quantize_f32_absolute_enforces_declared_error_bound() {
        let eps = 0.01f64;
        let config = QuantizationConfig::absolute(eps).unwrap();
        let raw: Vec<f32> = (-500..500).map(|i| (i as f32) * 0.137 + 273.15).collect();

        let (quantized, report) = quantize_f32_slice(&raw, &config, None).unwrap();
        assert_eq!(quantized.len(), raw.len());
        assert!(report.max_measured_abs_error <= eps);
        for (orig, recon) in raw.iter().zip(quantized.iter()) {
            let err = (orig - recon).abs() as f64;
            assert!(
                err <= eps,
                "Error {err} exceeded eps {eps} for orig {orig}, recon {recon}"
            );
        }
    }

    #[test]
    fn quantize_f32_preserves_specials_and_fill_value() {
        let eps = 0.01f64;
        let config = QuantizationConfig::absolute(eps).unwrap();
        let fill = -9999.0f32;
        let raw = vec![
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -0.0f32,
            0.0f32,
            fill,
            295.456,
        ];

        let (quantized, report) = quantize_f32_slice(&raw, &config, Some(fill)).unwrap();
        assert_eq!(quantized.len(), raw.len());
        assert!(quantized[0].is_nan());
        assert_eq!(quantized[1], f32::INFINITY);
        assert_eq!(quantized[2], f32::NEG_INFINITY);
        assert_eq!(quantized[3].to_bits(), (-0.0f32).to_bits());
        assert_eq!(quantized[4].to_bits(), (0.0f32).to_bits());
        assert_eq!(quantized[5], fill);
        assert!((quantized[6] - raw[6]).abs() as f64 <= eps);
        assert_eq!(report.special_count, 4); // NaN, +inf, -inf, fill
    }

    #[test]
    fn quantize_f32_relative_enforces_relative_error_bound() {
        let eps_rel = 1e-4f64;
        let config = QuantizationConfig::relative(eps_rel).unwrap();
        let raw = vec![1e-4f32, 0.005f32, 1.2345f32, 100.5f32, 98765.4f32];

        let (quantized, report) = quantize_f32_slice(&raw, &config, None).unwrap();
        assert!(report.max_measured_rel_error <= eps_rel);
        for (orig, recon) in raw.iter().zip(quantized.iter()) {
            let rel_err = ((orig - recon).abs() / orig.abs()) as f64;
            assert!(
                rel_err <= eps_rel,
                "Relative error {rel_err} exceeded bound {eps_rel}"
            );
        }
    }

    #[test]
    fn quantize_f32_preserves_shifted_grib_dyadic_grid_without_requantization_bias() {
        let eps = 0.01f64;
        let config = QuantizationConfig::absolute(eps).unwrap();
        // Simulate a GRIB2 field with arbitrary referenceValue R and step 2^-5 = 0.03125
        let r = 269.979_28_f32;
        let grib_step = 0.03125f32;
        let raw: Vec<f32> = (0..256)
            .map(|i| r + ((i % 37) as f32) * grib_step)
            .collect();

        let (quantized, report) = quantize_f32_slice(&raw, &config, None).unwrap();
        assert_eq!(quantized, raw);
        assert_eq!(report.max_measured_abs_error, 0.0);
        assert_eq!(report.rmse, 0.0);
        assert_eq!(report.bias, 0.0);
    }
}
