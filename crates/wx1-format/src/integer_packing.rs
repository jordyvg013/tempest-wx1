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

//! Integer-packing lossy/lossless codec with spatial prediction and entropy coding.
//!
//! Provides a compact integer representation for 2D/3D meteorological grids:
//! 1. Affine / dyadic reference value (offset) and scale step:
//!    $U_i = \text{round}\left(\frac{X_i - R}{S}\right)$
//! 2. Spatial predictors:
//!    - None
//!    - 1D horizontal delta: $\Delta_i = U_i - U_{i-1}$
//!    - 2D parallelogram predictor: $\hat{U}_{y,x} = U_{y,x-1} + U_{y-1,x} - U_{y-1,x-1}$
//! 3. Zig-zag residual mapping to unsigned integers.
//! 4. Byte-shuffle for optimal grouping of homologous byte lanes.
//! 5. Zstandard entropy coding.

use crate::quantization::{byte_shuffle, byte_unshuffle, QuantizationError};

/// Spatial predictor mode for integer-packing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegerPredictor {
    /// No predictor (direct quantized integers).
    None,
    /// 1D delta along the fastest-varying coordinate.
    Delta1D,
    /// 2D plane / parallelogram predictor for grid with known row stride (`width`).
    Plane2D { width: usize },
}

/// Compressed buffer produced by integer packing.
#[derive(Debug, Clone, PartialEq)]
pub struct IntegerPackedBuffer {
    /// Minimum reference value subtracted before scaling.
    pub reference_value: f32,
    /// Quantization step size $S$ ($2\epsilon$ or dyadic step).
    pub step: f32,
    /// Minimum bit width needed to represent the max integer value.
    pub min_bit_width: u8,
    /// Predictor employed.
    pub predictor: IntegerPredictor,
    /// Total number of float32 values in original uncompressed array.
    pub count: usize,
    /// Compressed payload bytes (Zstandard stream).
    pub compressed_payload: Vec<u8>,
    /// Number of bytes used per symbol before compression (1, 2, or 4).
    pub item_size: usize,
}

impl IntegerPackedBuffer {
    /// Returns the effective bits per value of the compressed stream.
    pub fn bits_per_value(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            (self.compressed_payload.len() as f64 * 8.0) / (self.count as f64)
        }
    }
}

/// Encodes an `f32` slice using affine integer quantization, spatial prediction,
/// byte shuffle, and Zstandard entropy compression.
pub fn encode_integer_packed(
    data: &[f32],
    step: f32,
    predictor: IntegerPredictor,
    compression_level: i32,
) -> Result<IntegerPackedBuffer, QuantizationError> {
    if data.is_empty() {
        return Ok(IntegerPackedBuffer {
            reference_value: 0.0,
            step,
            min_bit_width: 0,
            predictor,
            count: 0,
            compressed_payload: Vec::new(),
            item_size: 1,
        });
    }

    if step <= 0.0 || step.is_nan() || step.is_infinite() {
        return Err(QuantizationError::InvalidErrorBound(
            "step size must be positive and finite".to_string(),
        ));
    }

    // 1. Find min and max
    let mut min_val = f32::INFINITY;
    let mut max_val = f32::NEG_INFINITY;
    for &v in data {
        if v.is_finite() {
            if v < min_val {
                min_val = v;
            }
            if v > max_val {
                max_val = v;
            }
        }
    }

    if !min_val.is_finite() {
        min_val = 0.0;
    }

    let inv_step = 1.0 / (step as f64);
    let ref_f64 = min_val as f64;

    // 2. Quantize to unsigned integer symbols
    let mut u_vals: Vec<u32> = Vec::with_capacity(data.len());
    let mut max_u = 0u32;
    for &v in data {
        let val_f64 = v as f64;
        let q = ((val_f64 - ref_f64) * inv_step).round();
        let u = if q < 0.0 {
            0u32
        } else if q > u32::MAX as f64 {
            u32::MAX
        } else {
            q as u32
        };
        if u > max_u {
            max_u = u;
        }
        u_vals.push(u);
    }

    let min_bit_width = if max_u == 0 {
        1
    } else {
        (32 - max_u.leading_zeros()) as u8
    };

    // 3. Apply predictor to compute signed residuals
    let n = u_vals.len();
    let mut residuals: Vec<i64> = vec![0i64; n];

    match predictor {
        IntegerPredictor::None => {
            for i in 0..n {
                residuals[i] = u_vals[i] as i64;
            }
        }
        IntegerPredictor::Delta1D => {
            if n > 0 {
                residuals[0] = u_vals[0] as i64;
                for i in 1..n {
                    residuals[i] = (u_vals[i] as i64) - (u_vals[i - 1] as i64);
                }
            }
        }
        IntegerPredictor::Plane2D { width } => {
            if width == 0 || width > n {
                return Err(QuantizationError::InvalidDimensions);
            }
            let height = n / width;
            let remainder = n % width;

            for y in 0..height {
                let row_start = y * width;
                for x in 0..width {
                    let idx = row_start + x;
                    let curr = u_vals[idx] as i64;
                    let pred = if y == 0 && x == 0 {
                        0i64
                    } else if y == 0 {
                        u_vals[idx - 1] as i64
                    } else if x == 0 {
                        u_vals[idx - width] as i64
                    } else {
                        let left = u_vals[idx - 1] as i64;
                        let up = u_vals[idx - width] as i64;
                        let diag = u_vals[idx - width - 1] as i64;
                        left + up - diag
                    };
                    residuals[idx] = curr - pred;
                }
            }
            // Remainder elements (if any) use 1D delta
            if remainder > 0 {
                let start = height * width;
                residuals[start] = (u_vals[start] as i64) - (u_vals[start - 1] as i64);
                for i in (start + 1)..n {
                    residuals[i] = (u_vals[i] as i64) - (u_vals[i - 1] as i64);
                }
            }
        }
    }

    // 4. Zig-zag mapping from signed residual to unsigned integer:
    // 0 -> 0, -1 -> 1, 1 -> 2, -2 -> 3, 2 -> 4, ...
    let mut max_z = 0u64;
    let mut z_vals: Vec<u64> = Vec::with_capacity(n);
    for &res in &residuals {
        let z = if res >= 0 {
            (res as u64) * 2
        } else {
            ((-res as u64) * 2) - 1
        };
        if z > max_z {
            max_z = z;
        }
        z_vals.push(z);
    }

    // Determine packing byte size based on max zig-zag value
    let item_size = if max_z <= 0xFF {
        1
    } else if max_z <= 0xFFFF {
        2
    } else {
        4
    };

    let mut raw_bytes = Vec::with_capacity(n * item_size);
    match item_size {
        1 => {
            for &z in &z_vals {
                raw_bytes.push(z as u8);
            }
        }
        2 => {
            for &z in &z_vals {
                let bytes = (z as u16).to_le_bytes();
                raw_bytes.extend_from_slice(&bytes);
            }
        }
        _ => {
            for &z in &z_vals {
                let bytes = (z as u32).to_le_bytes();
                raw_bytes.extend_from_slice(&bytes);
            }
        }
    }

    // 5. Byte-shuffle to group homologous byte lanes
    let shuffled =
        byte_shuffle(&raw_bytes, item_size).map_err(|_| QuantizationError::AllocationFailed)?;

    // 6. Zstandard compression
    let compressed_payload = zstd::bulk::compress(&shuffled, compression_level)
        .map_err(|_| QuantizationError::AllocationFailed)?;

    Ok(IntegerPackedBuffer {
        reference_value: min_val,
        step,
        min_bit_width,
        predictor,
        count: n,
        compressed_payload,
        item_size,
    })
}

/// Decodes an [`IntegerPackedBuffer`] back into a reconstructed `Vec<f32>`.
pub fn decode_integer_packed(buffer: &IntegerPackedBuffer) -> Result<Vec<f32>, QuantizationError> {
    if buffer.count == 0 {
        return Ok(Vec::new());
    }

    let uncompressed_bytes_len = buffer
        .count
        .checked_mul(buffer.item_size)
        .ok_or(QuantizationError::InvalidDimensions)?;

    // 1. Zstandard decompress
    let shuffled = zstd::bulk::decompress(&buffer.compressed_payload, uncompressed_bytes_len)
        .map_err(|_| QuantizationError::AllocationFailed)?;

    // 2. Byte-unshuffle
    let raw_bytes = byte_unshuffle(&shuffled, buffer.item_size)
        .map_err(|_| QuantizationError::AllocationFailed)?;

    // 3. Unpack zig-zag residuals
    let n = buffer.count;
    let mut residuals = Vec::with_capacity(n);
    match buffer.item_size {
        1 => {
            for &b in &raw_bytes[..n] {
                let z = b as u64;
                let res = if z % 2 == 0 {
                    (z / 2) as i64
                } else {
                    -(z.div_ceil(2) as i64)
                };
                residuals.push(res);
            }
        }
        2 => {
            for chunk in raw_bytes.chunks_exact(2).take(n) {
                let z = u16::from_le_bytes([chunk[0], chunk[1]]) as u64;
                let res = if z % 2 == 0 {
                    (z / 2) as i64
                } else {
                    -(z.div_ceil(2) as i64)
                };
                residuals.push(res);
            }
        }
        _ => {
            for chunk in raw_bytes.chunks_exact(4).take(n) {
                let z = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as u64;
                let res = if z % 2 == 0 {
                    (z / 2) as i64
                } else {
                    -(z.div_ceil(2) as i64)
                };
                residuals.push(res);
            }
        }
    }

    // 4. Invert predictor
    let mut u_vals = vec![0u32; n];
    match buffer.predictor {
        IntegerPredictor::None => {
            for i in 0..n {
                u_vals[i] = residuals[i].max(0) as u32;
            }
        }
        IntegerPredictor::Delta1D => {
            if n > 0 {
                let mut acc = residuals[0];
                u_vals[0] = acc.max(0) as u32;
                for i in 1..n {
                    acc += residuals[i];
                    u_vals[i] = acc.max(0) as u32;
                }
            }
        }
        IntegerPredictor::Plane2D { width } => {
            if width == 0 || width > n {
                return Err(QuantizationError::InvalidDimensions);
            }
            let height = n / width;
            let remainder = n % width;

            for y in 0..height {
                let row_start = y * width;
                for x in 0..width {
                    let idx = row_start + x;
                    let pred = if y == 0 && x == 0 {
                        0i64
                    } else if y == 0 {
                        u_vals[idx - 1] as i64
                    } else if x == 0 {
                        u_vals[idx - width] as i64
                    } else {
                        let left = u_vals[idx - 1] as i64;
                        let up = u_vals[idx - width] as i64;
                        let diag = u_vals[idx - width - 1] as i64;
                        left + up - diag
                    };
                    let reconstructed_u = pred + residuals[idx];
                    u_vals[idx] = reconstructed_u.max(0) as u32;
                }
            }
            if remainder > 0 {
                let start = height * width;
                let mut acc = (u_vals[start - 1] as i64) + residuals[start];
                u_vals[start] = acc.max(0) as u32;
                for i in (start + 1)..n {
                    acc += residuals[i];
                    u_vals[i] = acc.max(0) as u32;
                }
            }
        }
    }

    // 5. Reconstruct float values: X = R + U * step
    let ref_f64 = buffer.reference_value as f64;
    let step_f64 = buffer.step as f64;
    let mut out = Vec::with_capacity(n);
    for u in u_vals {
        let val = (ref_f64 + (u as f64) * step_f64) as f32;
        out.push(val);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_packing_roundtrip_delta1d() {
        let step = 0.25f32;
        let original: Vec<f32> = (0..1000)
            .map(|i| 273.15 + (i as f32) * 0.25 + ((i % 5) as f32) * 0.25)
            .collect();

        let packed = encode_integer_packed(&original, step, IntegerPredictor::Delta1D, 3)
            .expect("encode succeeds");

        assert!(packed.bits_per_value() < 8.0);

        let decoded = decode_integer_packed(&packed).expect("decode succeeds");
        assert_eq!(decoded.len(), original.len());

        for (orig, recon) in original.iter().zip(decoded.iter()) {
            assert!((orig - recon).abs() <= 1e-4);
        }
    }

    #[test]
    fn integer_packing_roundtrip_plane2d() {
        let width = 64;
        let height = 32;
        let step = 0.125f32;
        let mut original = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let temp = 250.0 + (y as f32) * 0.5 + (x as f32) * 0.25;
                original.push(temp);
            }
        }

        let packed = encode_integer_packed(&original, step, IntegerPredictor::Plane2D { width }, 7)
            .expect("encode succeeds");

        // Smooth planar 2D field residuals are virtually all 0
        assert!(packed.bits_per_value() < 2.0);

        let decoded = decode_integer_packed(&packed).expect("decode succeeds");
        assert_eq!(decoded.len(), original.len());

        for (orig, recon) in original.iter().zip(decoded.iter()) {
            assert!((orig - recon).abs() <= 1e-4);
        }
    }
}
