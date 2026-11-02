# WX-1 Python, scientific functions and compute architecture

- Status: design draft
- Target: bounded-memory scientific processing on CPU, GPU or both

## Package layers

`wx1` is the planned public Python distribution. Native bindings expose safe operations from the Rust core through PyO3; maturin builds platform wheels. The pure-Python layer handles ergonomic APIs, integrations and optional adapters. The binary format itself remains language-independent.

Optional dependency groups are planned:

- `numpy`: arrays and CPU numerical interop;
- `xarray`: labeled dimensions, coordinates and dataset interface;
- `dask`: lazy task scheduling and chunked/out-of-core processing;
- `metpy`: meteorological calculations, units and meteorological conventions;
- `jax`: optional accelerator backend;
- `cupy`: optional alternative CUDA array backend;
- `pandas`: tabular/time-series workflows;
- `pyyaml`, `openpyxl`: optional YAML/XLSX table import.

No optional scientific or GPU dependency is required to open, inspect or validate a WX-1 file.

## MetPy integration

MetPy functions are wrapped/adapted, not copied into the storage core. Adapters validate units, coordinate systems, grid spacing, vertical coordinate and required dimensions. Results carry provenance and declared numerical precision. If a MetPy function cannot operate correctly on an individual chunk, the function metadata must declare required halo, full-axis or multi-pass processing.

WX-1 may add its own functions where needed, but each function must document equations, units, assumptions, missing-data behavior, precision and validation references. A plugin registry may expose user-defined operations. Plugins execute trusted code and are not loaded from untrusted table packages.

## Execution API (illustrative)

```python
import wx1

with wx1.open("forecast.wx1") as ds:
    result = ds["air_temperature_2m"].compute(
        operation="metpy.some_supported_operation",
        execution="auto",       # cpu | gpu | auto | hybrid
        memory_budget={
            "host": "4 GiB",
            "device": "3 GiB",
        },
        chunks="auto",
    )
```

The operation name above illustrates the API shape; it is not a claim that a particular MetPy function is already wrapped.

## 6 GB VRAM operating model

The system must not allocate all 6 GB to WX-1. Runtime overhead, display use, other processes and temporary workspaces consume memory. The user-configured device budget is an upper bound and should default conservatively. A scheduler estimates:

`input tile + output tile + temporary workspace + halo + transfer buffers <= device budget`

If the estimate is unavailable or unreliable, the scheduler uses smaller batches and handles allocation failure safely. Tile sizes can be adapted downward. The scheduler applies backpressure so the reader cannot outrun the compute backend and fill host RAM with queued chunks.

CPU and GPU can operate simultaneously on disjoint chunks. A hybrid scheduler must account for CPU thread limits, disk throughput, decompression throughput, PCIe transfer overhead and output ordering. It should not assume hybrid is faster: for small arrays or transfer-heavy functions, CPU-only may be better.

## JAX versus CuPy

JAX offers composable transformations and accelerator support, but introduces backend/runtime constraints and may compile kernels with nontrivial startup and memory costs. CuPy provides a NumPy-like CUDA interface and can be simpler for existing array kernels, but is CUDA-focused. Neither belongs in the core dependency set.

Recommendation: keep an abstract backend protocol and implement CPU first. Add one GPU backend after a compatibility spike on the intended WSL2/Linux and Windows environments. Evaluate JAX and CuPy with representative kernels, chunk sizes, memory use, installation complexity and correctness. Do not promise that every MetPy function can run on GPU; many depend on NumPy/SciPy behavior or CPU-only libraries.

## Testing

- CPU-only installation and import with no GPU libraries installed.
- Small and large arrays with bounded chunk iteration.
- Low VRAM budgets and allocation failures.
- CPU fallback in `auto`; explicit error in `gpu` mode.
- Disjoint CPU/GPU chunk execution in `hybrid` mode.
- Numerical comparisons against CPU reference results with documented tolerances.
- Functions with halos, reductions, missing values and unit conversions.
- Reproducibility metadata for backend, precision and function version.
