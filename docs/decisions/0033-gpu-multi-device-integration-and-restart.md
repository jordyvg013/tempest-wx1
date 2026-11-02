# ADR 0033 — GPU and Multi-Device Integration, Asynchronous Non-Blocking Transfer, and Bit-Exact Restart

## Status
Accepted (Fase 4, Onderdeel 10)

## Context
High-performance numerical weather prediction (NWP) and machine learning atmospheric forecasting models (such as JAX-based regional models, ECMWF AIFS, GraphCast, and neural NWP solvers) increasingly execute their operational timesteps on hardware accelerators (GPUs and TPUs). 

In such systems:
1. **Computational Stalls**: Synchronous I/O halts GPU kernel execution. If a simulation must wait for disk `fsync` or chunk compression on every model step, accelerator utilization drops precipitously.
2. **Device Array Interoperability**: Simulation state resides in accelerator memory (e.g., JAX `ArrayImpl`, PyTorch `torch.Tensor`, or DLPack-compatible tensors). Converting these arrays to host memory must be zero-friction and capability-based without forcing GPU toolchains on CPU users.
3. **Multi-Device / Multi-Rank Sharding**: Large models partition global atmospheric grids across multiple GPUs or MPI ranks ($P_x \times P_y$). Each device computes a local subdomain (often with ghost cells/halos). The storage format must cleanly record partitioned sharded state without data corruption or global write serialization bottlenecks.
4. **Bit-Exact Restart Guarantee**: Checkpoints used to resume interrupted operational simulations must be 100% bit-exact across restarts. Quantization or lossy compression is strictly forbidden on restart snapshots.
5. **Portability of Wire Format**: No GPU-specific proprietary compression codecs (such as nvCOMP-only proprietary streams). Compressed chunks must remain standard, portable Zstandard or LZ4 readable on any CPU.

## Decision

### 1. CPU-First & Capability-Based Device Ingestion
- Core WX-1 Rust and minimal Python packages contain **zero** mandatory GPU dependencies (CUDA, ROCm, OpenCL).
- In Python, `wx1.AsyncWriter` and `wx1.write_array` detect device arrays via:
  - DLPack protocol (`__dlpack__` / `__dlpack_device__`);
  - JAX array detection (`isinstance(data, jax.Array)`);
  - PyTorch / CuPy device protocol (`.cpu()` / `__array__`).
- Staging transfers device data to host memory asynchronously. When JAX is present, `wx1` utilizes non-blocking device-to-host transfers, overlapping transfer of step $t$ with computation of step $t+1$.

### 2. Double-Buffered Asynchronous Writer Pipeline
- `wx1.AsyncWriter` implements double-buffered pipelining for NWP loops:
  - **GPU Thread**: Completes timestep $t$, dispatches device-to-host transfer, immediately begins timestep $t+1$.
  - **Host Staging Queue**: Bounded by `max_pending_generations` (default: 2) and `max_queued_bytes` (default: 256 MiB).
  - **Background Worker Threads**: Compress chunks (Zstd/LZ4), calculate BLAKE3/CRC32C digests, append generations, and flush commit footers (`durable` or `buffered`).
  - Backpressure is applied only when storage I/O bandwidth is saturated beyond the double-buffer window.

### 3. Multi-Device Sharded Checkpointing & Reconstruction
- For distributed multi-device models, WX-1 provides partitioned sharded writers and readers:
  - `ShardInfo`: Encapsulates `rank`, `num_ranks`, `origin: Vec<u64>`, `shape: Vec<u64>`, and `halo: Vec<u64>`.
  - Multiple devices can independently stage their non-overlapping spatial chunks.
  - Slices can be queried either per device (restoring local shard directly) or as a seamless global field reconstructed bit-exact across partition boundaries.

### 4. Bit-Exact Restart Profile
- Checkpoints configured with `profile="restart"`:
  - Enforce lossless compression: ByteShuffle + Zstandard or LZ4.
  - Floating-point quantization is strictly rejected with `ValueError` / `ScientificModelError`.
  - Preserves auxiliary model state: prognostic variables, surface fields, iteration counter, simulation timestamp, and PRNG key state.

### 5. Optional Direct Device Read-Back
- When reading datasets back for inference or resumed simulation:
  - `file.read_array(variable_path, to_device="jax")`: When JAX is installed, returns a device-placed JAX array directly.
  - If the requested device backend is not installed, falls back cleanly to NumPy on CPU with an informative warning/exception.

## Consequences
- **Positive**:
  - Eliminates I/O stalls in GPU NWP models by overlapping compute of step $t+1$ with host compression of step $t$.
  - Seamless support for JAX and DLPack arrays out of the box.
  - Preserves full CPU-only portability: containers written on GPU clusters remain 100% readable on standard CPU machines.
  - Guarantees bit-exact numerical restartability for climate simulations and operational forecast models.
- **Negative / Trade-offs**:
  - Double-buffering consumes an additional $1\times$ host buffer allocation during concurrent model steps.
  - Device-to-host PCIe bus transfer time cannot be eliminated, but is hidden behind GPU compute overlap.
