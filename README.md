# WX-1: Open Scientific & Meteorological Container Format

[![Crates.io](https://img.shields.io/badge/crates.io-v1.0.0--rc1-orange.svg)](https://crates.io)
[![Python](https://img.shields.io/badge/python-3.10%20%7C%203.11%20%7C%203.12%20%7C%203.13-blue.svg)](https://pypi.org)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-lightgrey.svg)](https://www.rust-lang.org)
[![Safety](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-lang/rfcs)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

**WX-1** is an independent, high-performance, language-agnostic scientific storage format and engine specifically engineered for numerical weather prediction (NWP), climate reanalysis, and AI-driven weather modeling (GraphCast, FourCastNet, Pangu-Weather, GenCast).

It is written in 100% safe Rust with `#![forbid(unsafe_code)]`, featuring a stable C-ABI (`include/wx1.h`), native Python PyO3 bindings, an `xarray` backend engine (`engine="wx1"`), and bidirectional adapters for WMO GRIB Edition 1 & 2, NetCDF-4 (HDF5), and Zarr v2/v3.

---

## Key Highlights

* **100% Bit-Exact & Deterministic**: Strict mathematical preservation of raw IEEE 754 bit-patterns (`.view(np.uint32)`) for reproducible NWP restarts.
* **4D Spatio-Temporal Atmospheric Tensors**: First-class support for `[time, level, latitude, longitude]` grids, isobaric pressure levels, hybrid sigma-pressure levels, ensembles, and unstructured meshes (UGRID).
* **Per-Variable Pluggable Codecs**: Select `zstd_shuffle`, pure `zstd`, `lz4`, or bounded error quantization (`profile="output"`) individually per meteorological variable.
* **Access-Optimized Chunk Topologies**:
  * *Horizontal Map Chunking* (`[1, 721, 1440]`): Global synoptic weather maps in **8.86 ms** (30× faster than tiled storage).
  * *Spatial Tile Chunking* (`[120, 32, 32]`): 120-step local station time-series (e.g. De Bilt) in **0.94 ms** (300× faster than map-oriented formats).
* **Power-Loss & Crash Resilience**: Two-phase atomic commits with CRC32C chunk integrity, secondary forward-index recovery, and corrupt-header recovery.
* **Direct GPU Acceleration**: Zero-copy / zero-overhead tensor streaming to NVIDIA GPUs via DLPack, JAX, and PyTorch (verified on CUDA 12).

---

## Benchmark Evidence: Authentic ECMWF ERA5 Reanalysis

Benchmarks executed on an authentic ECMWF ERA5 dataset (0.25° global equirectangular grid, $721 \times 1440 = 1.038.240$ points/slice):

### 1. Surface Reanalysis (7 Variables, 120 Timesteps, $124.588.800$ floats/variable)

| Variable | Description | Raw (f32) | NetCDF-4 (zlib) | WX-1 (Zstd+Shuffle) | WX-1 vs NetCDF-4 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **`tp`** | Total precipitation | 498.36 MB | 108.44 MB | **99.58 MB** | **-8.86 MB (-8.2%)** |
| **`t2m`** | 2m temperature | 498.36 MB | ~186.22 MB | **174.56 MB** | **-11.66 MB (-6.3%)** |
| **`msl`** | Mean sea level pressure | 498.36 MB | ~186.22 MB | **160.27 MB** | **-25.95 MB (-13.9%)** |
| **`sp`** | Surface pressure | 498.36 MB | ~186.22 MB | **163.73 MB** | **-22.49 MB (-12.1%)** |
| **`sst`** | Sea surface temp (NaN mask) | 498.36 MB | ~186.22 MB | **44.03 MB** | **-142.19 MB (-76.4%)** |
| **`u10`** | 10m U-wind component | 498.36 MB | ~186.22 MB | **245.49 MB** | +59.27 MB |
| **`v10`** | 10m V-wind component | 498.36 MB | ~186.22 MB | **246.88 MB** | +60.66 MB |
| **TOTAL** | **All 7 Variables** | **3,488.52 MB** | **1,225.78 MB** | **1,134.55 MB** | **-91.23 MB (-7.4%)** |

* **Compared to GRIB1**: The original `era5_sfc_all.grib` is **1,675.46 MB**. WX-1 is **540.91 MB smaller (-32.3%)**.
* **Bit-Exactness**: All $7 \times 124.588.800 = 872.121.600$ float32 values are 100% bit-exact ($\Delta_{\max} = 0.0$ via `.view(np.uint32)`).
* **Read Slicing Latency**:
  * Container Open: WX-1 **2.97 ms** vs NetCDF-4 57.91 ms (**19.5× faster**).
  * 2D Global Field Read (R2): WX-1 **159.08 ms** vs NetCDF-4 883.33 ms (**5.55× faster**).

### 2. 4D Pressure Level Atmosphere (25.36 GB GRIB1, 13,200 Messages, 10..1000 hPa)

* **4D Temperature (`t`, `[120, 11, 721, 1440]`)**: 5.48 GB raw compressed to **1.87 GB** (2.92× lossless compression).
  * Global T500 hPa 2D slice read: **24.57 ms**.
  * Full 3D column (11 levels × 721 × 1440): **223.04 ms**.
  * Virtual Atmospheric Radiosonde Sounding (De Bilt, 52.1°N, 5.18°E): **178.98 ms**.
* **4D Geopotential (`z`, `[120, 11, 721, 1440]`)**: 5.48 GB raw compressed to **1.51 GB** (3.63× lossless compression).
  * Global Z500 hPa 2D slice read: **38.66 ms**.

### 3. Fair Quantization & Wind Physics Benchmark

Under an identical declared error bound ($\pm 0.01\text{ m/s}$ max error):
* **NetCDF-4 Lossy** (`least_significant_digit=2`): **161.51 MB**
* **WX-1 Lossy** (`lossy_error_bound=0.01`): **146.20 MB** (**15.31 MB smaller than NetCDF-4**)
* **Physical Impact on Derived Atmospheric Quantities**:
  * Wind Speed ($V = \sqrt{u^2+v^2}$): RMSE = $0.0045\text{ m/s}$ (relative error: 0.031%).
  * Wind Direction ($0..360^\circ$): Mean error = $0.049^\circ$ (99% percentile: $0.272^\circ$).
  * Spherical Relative Vorticity ($\zeta$): RMSE error is **1.45%** of physical synoptic vorticity standard deviation.

---

## Quickstart

### Python

```bash
pip install tempest-wx1
```

```python
import wx1
import numpy as np
import xarray as xr

# 1. Write a 4D meteorological array
temperature = np.random.randn(120, 11, 721, 1440).astype(np.float32)
wx1.write_array(
    "forecast.wx1",
    temperature,
    dataset_id="era5_t",
    variable_path="/t",
    dimensions=["time", "level", "latitude", "longitude"],
    dimension_roles=["time", "vertical", "latitude", "longitude"],
    chunk_shape=[1, 1, 721, 1440],  # 1 level/step per chunk
    codec="zstd_shuffle",
    profile="restart",               # strictly lossless
    attributes={"units": "K", "long_name": "temperature"}
)

# 2. Fast sub-volume slice reading
with wx1.open_file("forecast.wx1") as f:
    # Read T500 hPa slice at time step 0
    t500 = f.read_array_slice("/t", start=[0, 6, 0, 0], count=[1, 1, 721, 1440])
    print("T500 shape:", t500.shape)

# 3. Native xarray integration
ds = xr.open_dataset("forecast.wx1", engine="wx1")
print(ds["t"])

# 4. Multi-format conversion
wx1.from_grib("input.grib", "converted.wx1", variable_name="t")
wx1.to_netcdf("converted.wx1", "output.nc")
wx1.to_zarr("converted.wx1", "output.zarr")
```

### Rust

Add to your `Cargo.toml`:
```toml
[dependencies]
wx1-format = "1.0.0-rc1"
```

```rust
use std::path::Path;
use wx1_format::scientific::{ScientificContainerReader, ArrayReadPlan};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let container = ScientificContainerReader::open(Path::new("forecast.wx1"))?;
    let snapshot = container.snapshot()?;
    
    // Read arbitrary multidimensional slice without full decompression
    let slice_bytes = snapshot.read_array_slice(
        "/t",
        &[0, 6, 0, 0],         // start
        &[1, 1, 721, 1440],     // count
    )?;
    
    println!("Read slice bytes: {}", slice_bytes.len());
    Ok(())
}
```

### C & Fortran (C-ABI FFI)

```c
#include "wx1.h"
#include <stdio.h>
#include <stdlib.h>

int main() {
    wx1_file_t* file = NULL;
    wx1_error_t* err = NULL;

    if (wx1_open_file("forecast.wx1", &file, &err) != WX1_OK) {
        fprintf(stderr, "Error: %s\n", wx1_error_message(err));
        wx1_error_free(err);
        return 1;
    }

    uint64_t start[4] = {0, 6, 0, 0};
    uint64_t count[4] = {1, 1, 721, 1440};
    float data[721 * 1440];

    wx1_read_array_slice(file, "/t", start, count, 4, data, sizeof(data), &err);
    printf("Read T500: %f K\n", data[0]);

    wx1_close_file(file);
    return 0;
}
```

Compile with:
```bash
gcc main.c -lwx1_ffi -Ldist -Iinclude -Wl,-rpath,dist -o main
```

---

## Workspace Structure

* [`crates/wx1-format/`](crates/wx1-format/): Core pure safe-Rust storage engine, chunk pipeline, CBOR v2 schema, and recovery mechanics.
* [`crates/wx1-ffi/`](crates/wx1-ffi/): Stable C-ABI implementation exporting [`include/wx1.h`](include/wx1.h).
* [`crates/wx1-cli/`](crates/wx1-cli/): Standalone CLI tool (`wx1 inspect`, `wx1 repair`, `wx1 validate-scientific`).
* [`crates/wx1-registry/`](crates/wx1-registry/): Generation and catalog registry.
* [`python/wx1/`](python/wx1/): High-level Python package, `xarray` engine, and adapters for GRIB, NetCDF-4, and Zarr.
* [`bench/runner/`](bench/runner/): Reproducible benchmark scripts for cross-format, hardware GPU, and ERA5 evaluations.
* [`reports/`](reports/): Exhaustive benchmark reports with raw measured numbers and hardware specifications.

---

## Architecture Decisions (ADRs)

1. [ADR 0016: CBOR Schema v2 Scientific Data Model](docs/decisions/0016-schema-v2-scientific-data-model.md)
2. [ADR 0022: Unified Lossy Compression Pipeline](docs/decisions/0022-unified-lossy-compression-pipeline-and-chunk-format.md)
3. [ADR 0025: Parallel Staging & Async Commit](docs/decisions/0025-parallel-staging-and-merge-commit-pipeline.md)
4. [ADR 0026: Non-Regular Grids & Cubed Sphere Staggering](docs/decisions/0026-non-regular-grids-cubed-sphere-halos-and-staggering.md)
5. [ADR 0028: O(1) Indexed Scientific Array Planning](docs/decisions/0028-indexed-scientific-array-planner.md)
6. [ADR 0029: Advanced Read Patterns (Point & Region Slicing)](docs/decisions/0029-advanced-point-time-region-and-tile-read-paths.md)
7. [ADR 0030: External Format Adapters (GRIB2, NetCDF4, Zarr)](docs/decisions/0030-grib2-and-geotiff-external-format-adapters.md)
8. [ADR 0031: Robustness, Security Hardening & Golden Vectors](docs/decisions/0031-fuzzing-golden-vectors-and-security-hardening.md)
9. [ADR 0032: Stable C ABI & Cross-Language FFI](docs/decisions/0032-stable-c-abi-cbindgen-and-ffi-layer.md)
10. [ADR 0033: GPU Tensor Ingestion & Zero-Copy DLPack](docs/decisions/0033-gpu-multi-device-streaming-and-dlpack-interoperability.md)

---

## Verifying & Running Tests

Run the complete test suite:

```bash
# 1. Rust Workspace Test Suite (163 tests)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# 2. C-ABI FFI Integration Test
gcc -Wall -Wextra -Werror -Iinclude tests/c_integration/test_c_abi.c -Ldist -lwx1_ffi -Wl,-rpath,dist -o /tmp/test_c_abi
/tmp/test_c_abi

# 3. Python & Hardware CUDA Test Suite (56 tests)
pytest python/tests/ -v
```

---

## License

Licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE) or http://www.apache.org/licenses/LICENSE-2.0).
Copyright 2026 Jordy van Gils.

---

## Acknowledgments & Data Attribution

* Meteorological validation and benchmark datasets contain modified **Copernicus Climate Change Service (C3S) information [2026]**.
* Neither the European Commission nor ECMWF is responsible for any use that may be made of the information or data it contains.
* Reference: Hersbach, H. et al. (2020): *The ERA5 global reanalysis*. Quarterly Journal of the Royal Meteorological Society, 146(730), 1999–2049.
