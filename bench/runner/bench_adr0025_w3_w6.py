#!/usr/bin/env python3
"""ADR 0025 Benchmark Suite: W3 (Parallel Worker Scaling) & W6 (NWP Loop Async vs Sync Writer).

Strictly stores all temporary and output files on the Windows D: drive
(/mnt/d/Weer/Uitvoer/data/wx1_bench_scratch) and NEVER on the WSL C: virtual disk.
"""

from __future__ import annotations

import hashlib
import json
import os
import sys
import time
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "python"))

import wx1  # noqa: E402


SCRATCH_ROOT = Path(os.environ.get("WX1_SCRATCH", "/mnt/d/Weer/Uitvoer/data/wx1_bench_scratch"))
RESULTS_JSON = ROOT / "bench" / "results" / "adr0025_w3_w6_benchmark.json"


def load_real_cube() -> tuple[np.ndarray, str]:
    """Load the 3D ENFO 2t (50, 261, 661) float32 array from D: drive."""
    enfo_nc = Path("/mnt/d/Weer/Uitvoer/data/ecmwf/20261026_12z_enfo/crop/sfc/test_formats/ecmwf_enfo_sfc_2t_uncompressed.nc")
    if enfo_nc.exists():
        import netCDF4 as nc
        with nc.Dataset(enfo_nc, "r") as ds:
            arr = np.ascontiguousarray(ds.variables["field"][:], dtype="<f4")
        return arr, f"ecmwf_enfo_sfc_2t ({arr.shape[0]}x{arr.shape[1]}x{arr.shape[2]}, {arr.nbytes / 1e6:.2f} MB)"
    # Fallback synthetic 3D cube of identical size if file missing
    rng = np.random.RandomState(42)
    arr = rng.normal(280.0, 10.0, (50, 261, 661)).astype("<f4")
    return arr, "synthetic_3d (50x261x661, 34.50 MB)"


def run_w3_parallel_scaling(cube: np.ndarray, source_desc: str) -> dict:
    """Measure W3 write throughput and bit-for-bit identity across 1, 2, 4, 8, 12 workers."""
    w3_dir = SCRATCH_ROOT / "adr0025_w3"
    w3_dir.mkdir(parents=True, exist_ok=True)

    worker_counts = [1, 2, 4, 8, 12]
    chunk_shape = [1, 131, 331]  # 50 * 2 * 2 = 200 chunks (~173 KB raw per chunk)
    raw_bytes = int(cube.nbytes)
    repeats = 5

    results = {}
    baseline_sha256 = None
    baseline_median_ms = None

    for w in worker_counts:
        for dur in ("buffered", "durable"):
            key = f"workers_{w}_{dur}"
            timings_ms = []
            out_path = w3_dir / f"test_w{w}_{dur}.wx1"
            for r in range(repeats):
                if out_path.exists():
                    out_path.unlink()
                t0 = time.perf_counter()
                wx1.write_array(
                    out_path,
                    cube,
                    dataset_id="w3-scaling",
                    variable_path="/t2m",
                    dimensions=["number", "y", "x"],
                    chunk_shape=chunk_shape,
                    codec="zstd_shuffle",
                    workers=w,
                    durability=dur,
                )
                elapsed_ms = (time.perf_counter() - t0) * 1000.0
                timings_ms.append(elapsed_ms)

            digest = hashlib.sha256(out_path.read_bytes()).hexdigest()
            file_size = out_path.stat().st_size
            if baseline_sha256 is None:
                baseline_sha256 = digest
            bit_exact_match = (digest == baseline_sha256)

            # Verify full validation and bit-exact readback once per worker configuration
            with wx1.open_file(out_path) as f:
                f.validate()
                restored = f.read_array("/t2m")
                readback_exact = bool(np.array_equal(restored.view("u4"), cube.view("u4")))

            out_path.unlink(missing_ok=True)
            timings_ms.sort()
            med_ms = float(np.median(timings_ms))
            p95_ms = float(np.percentile(timings_ms, 95))
            throughput_mb_s = (raw_bytes / 1e6) / (med_ms / 1000.0)

            if w == 1 and dur == "buffered":
                baseline_median_ms = med_ms

            results[key] = {
                "workers": w,
                "durability": dur,
                "median_ms": round(med_ms, 3),
                "p95_ms": round(p95_ms, 3),
                "throughput_mb_s": round(throughput_mb_s, 2),
                "speedup_vs_1w_buffered": round(baseline_median_ms / med_ms, 2) if baseline_median_ms else 1.0,
                "file_bytes": file_size,
                "sha256": digest,
                "bit_exact_file_match": bit_exact_match,
                "readback_bit_exact": readback_exact,
            }
            print(
                f"[W3] workers={w:2d} ({dur:8s}): median={med_ms:7.2f} ms | p95={p95_ms:7.2f} ms | "
                f"throughput={throughput_mb_s:6.1f} MB/s | sha256_match={bit_exact_match}"
            )

    return {
        "source": source_desc,
        "shape": list(cube.shape),
        "chunk_shape": chunk_shape,
        "num_chunks": 200,
        "raw_bytes": raw_bytes,
        "repeats": repeats,
        "runs": results,
    }


def simulate_nwp_compute_step(state: np.ndarray, step: int) -> np.ndarray:
    """Simulate a realistic ~180 ms NWP explicit multi-substep stencil integration on a 3D field in memory."""
    out = state.copy()
    for substep in range(12):
        out[:, 1:-1, 1:-1] = (
            np.float32(0.999) * out[:, 1:-1, 1:-1]
            + np.float32(0.00025)
            * (
                out[:, :-2, 1:-1]
                + out[:, 2:, 1:-1]
                + out[:, 1:-1, :-2]
                + out[:, 1:-1, 2:]
            )
            + np.float32((step + substep) * 0.0001)
        )
    return out


def run_w6_nwp_loop_overhead(cube: np.ndarray) -> dict:
    """Measure W6 NWP loop wall time and compute-thread I/O stall across:
    1. compute_only (no I/O)
    2. sync_write (blocking write per output step)
    3. async_writer_buffered (ADR 0025 AsyncWriter with parallel staging + background commit)
    4. async_writer_durable (ADR 0025 AsyncWriter with background fsync)
    5. async_writer_backpressure (tiny queue to verify backpressure telemetry)
    """
    w6_dir = SCRATCH_ROOT / "adr0025_w6"
    w6_dir.mkdir(parents=True, exist_ok=True)

    # Use a 16-level 3D field (16 x 261 x 661 = 11.04 MB per step) over 6 forecast steps
    field0 = np.ascontiguousarray(cube[:16, :, :], dtype="<f4")
    spatial_shape = list(field0.shape)
    spatial_chunk_shape = [4, 131, 331]  # 4 * 2 * 2 = 16 chunks per step
    n_steps = 6
    step_raw_bytes = int(field0.nbytes)

    # Warmup compute once
    _ = simulate_nwp_compute_step(field0, 0)

    # 1. Compute-only baseline
    t0 = time.perf_counter()
    state = field0
    for s in range(n_steps):
        state = simulate_nwp_compute_step(state, s)
    compute_only_ms = (time.perf_counter() - t0) * 1000.0
    print(f"[W6] compute_only ({n_steps} steps): {compute_only_ms:.2f} ms ({compute_only_ms / n_steps:.2f} ms/step)")

    # 2. Synchronous blocking write per step (writing each step synchronously with 1 worker)
    sync_dir = w6_dir / "sync_steps"
    sync_dir.mkdir(parents=True, exist_ok=True)
    t0 = time.perf_counter()
    state = field0
    sync_io_blocking_ms = 0.0
    for s in range(n_steps):
        state = simulate_nwp_compute_step(state, s)
        t_io = time.perf_counter()
        step_file = sync_dir / f"step_{s:02d}.wx1"
        wx1.write_array(
            step_file,
            state,
            dataset_id="w6-sync",
            variable_path="/t",
            dimensions=["z", "y", "x"],
            chunk_shape=spatial_chunk_shape,
            codec="zstd_shuffle",
            workers=1,
            durability="durable",
        )
        sync_io_blocking_ms += (time.perf_counter() - t_io) * 1000.0
    sync_total_ms = (time.perf_counter() - t0) * 1000.0
    for p in sync_dir.glob("*.wx1"):
        p.unlink(missing_ok=True)
    print(
        f"[W6] sync_write_1w_durable: total={sync_total_ms:.2f} ms | "
        f"compute_thread_io_wait={sync_io_blocking_ms:.2f} ms"
    )

    # 3. AsyncWriter (8 workers, buffered, queue=4)
    async_buf_path = w6_dir / "nwp_async_buffered.wx1"
    async_buf_path.unlink(missing_ok=True)
    t0 = time.perf_counter()
    state = field0
    submit_blocking_buf_ms = 0.0
    with wx1.AsyncWriter(
        async_buf_path,
        dataset_id="w6-async-buf",
        variable_path="/t",
        spatial_dimensions=["z", "y", "x"],
        spatial_shape=spatial_shape,
        spatial_chunk_shape=spatial_chunk_shape,
        codec="zstd_shuffle",
        workers=8,
        max_pending_generations=4,
        max_queued_bytes=256 * 1024 * 1024,
        durability="buffered",
    ) as writer:
        for s in range(n_steps):
            state = simulate_nwp_compute_step(state, s)
            t_sub = time.perf_counter()
            writer.submit_step(state)
            submit_blocking_buf_ms += (time.perf_counter() - t_sub) * 1000.0
        tel_buf = writer.flush()
    async_buf_total_ms = (time.perf_counter() - t0) * 1000.0

    with wx1.open_file(async_buf_path) as f:
        assert f.generation == n_steps
        f.validate()
        last_step = f.read_array_slice("/t", [n_steps - 1, 0, 0, 0], [1] + spatial_shape)[0]
        assert np.array_equal(last_step.view("u4"), state.view("u4"))
    async_buf_path.unlink(missing_ok=True)
    print(
        f"[W6] async_writer_8w_buffered: total={async_buf_total_ms:.2f} ms | "
        f"submit_call_ms={submit_blocking_buf_ms:.2f} ms | "
        f"queue_stall_ms={tel_buf['total_enqueue_stall_ns'] / 1e6:.3f} ms"
    )

    # 4. AsyncWriter (8 workers, durable fsync in background thread, queue=4)
    async_dur_path = w6_dir / "nwp_async_durable.wx1"
    async_dur_path.unlink(missing_ok=True)
    t0 = time.perf_counter()
    state = field0
    submit_blocking_dur_ms = 0.0
    with wx1.AsyncWriter(
        async_dur_path,
        dataset_id="w6-async-dur",
        variable_path="/t",
        spatial_dimensions=["z", "y", "x"],
        spatial_shape=spatial_shape,
        spatial_chunk_shape=spatial_chunk_shape,
        codec="zstd_shuffle",
        workers=8,
        max_pending_generations=4,
        max_queued_bytes=256 * 1024 * 1024,
        durability="durable",
    ) as writer:
        for s in range(n_steps):
            state = simulate_nwp_compute_step(state, s)
            t_sub = time.perf_counter()
            writer.submit_step(state)
            submit_blocking_dur_ms += (time.perf_counter() - t_sub) * 1000.0
        tel_dur = writer.flush()
    async_dur_total_ms = (time.perf_counter() - t0) * 1000.0
    async_dur_path.unlink(missing_ok=True)
    print(
        f"[W6] async_writer_8w_durable: total={async_dur_total_ms:.2f} ms | "
        f"submit_call_ms={submit_blocking_dur_ms:.2f} ms | "
        f"queue_stall_ms={tel_dur['total_enqueue_stall_ns'] / 1e6:.3f} ms"
    )

    # 5. Forced backpressure test (max_pending_generations=1, no compute delay between submissions)
    async_bp_path = w6_dir / "nwp_async_bp.wx1"
    async_bp_path.unlink(missing_ok=True)
    with wx1.AsyncWriter(
        async_bp_path,
        dataset_id="w6-async-bp",
        variable_path="/t",
        spatial_dimensions=["z", "y", "x"],
        spatial_shape=spatial_shape,
        spatial_chunk_shape=spatial_chunk_shape,
        codec="zstd_shuffle",
        workers=8,
        max_pending_generations=1,
        max_queued_bytes=1024,  # tiny byte cap so any 2nd in-flight step blocks on Condvar
        durability="durable",
    ) as writer:
        for s in range(n_steps):
            writer.submit_step(field0)
        tel_bp = writer.flush()
    async_bp_path.unlink(missing_ok=True)
    print(
        f"[W6] forced_backpressure (queue=1): backpressure_events={tel_bp['backpressure_events']} | "
        f"total_stall_ms={tel_bp['total_enqueue_stall_ns'] / 1e6:.2f} ms | "
        f"max_stall_ms={tel_bp['max_enqueue_stall_ns'] / 1e6:.2f} ms"
    )

    return {
        "n_steps": n_steps,
        "spatial_shape": spatial_shape,
        "spatial_chunk_shape": spatial_chunk_shape,
        "step_raw_bytes": step_raw_bytes,
        "total_raw_bytes": step_raw_bytes * n_steps,
        "compute_only_ms": round(compute_only_ms, 2),
        "sync_write_1w_durable": {
            "total_wall_ms": round(sync_total_ms, 2),
            "compute_thread_io_wait_ms": round(sync_io_blocking_ms, 2),
            "overhead_vs_compute_pct": round((sync_total_ms - compute_only_ms) / compute_only_ms * 100.0, 1),
        },
        "async_writer_8w_buffered": {
            "total_wall_ms": round(async_buf_total_ms, 2),
            "submit_call_ms": round(submit_blocking_buf_ms, 2),
            "queue_stall_ms": round(tel_buf["total_enqueue_stall_ns"] / 1e6, 3),
            "speedup_vs_sync_total": round(sync_total_ms / async_buf_total_ms, 2),
            "compute_wait_reduction_factor": round(sync_io_blocking_ms / max(0.001, submit_blocking_buf_ms), 2),
            "telemetry": tel_buf,
        },
        "async_writer_8w_durable": {
            "total_wall_ms": round(async_dur_total_ms, 2),
            "submit_call_ms": round(submit_blocking_dur_ms, 2),
            "queue_stall_ms": round(tel_dur["total_enqueue_stall_ns"] / 1e6, 3),
            "speedup_vs_sync_total": round(sync_total_ms / async_dur_total_ms, 2),
            "compute_wait_reduction_factor": round(sync_io_blocking_ms / max(0.001, submit_blocking_dur_ms), 2),
            "telemetry": tel_dur,
        },
        "forced_backpressure_queue_1": {
            "backpressure_events": tel_bp["backpressure_events"],
            "total_stall_ms": round(tel_bp["total_enqueue_stall_ns"] / 1e6, 2),
            "max_stall_ms": round(tel_bp["max_enqueue_stall_ns"] / 1e6, 2),
            "telemetry": tel_bp,
        },
    }


def main():
    SCRATCH_ROOT.mkdir(parents=True, exist_ok=True)
    cube, desc = load_real_cube()
    print(f"Loaded benchmark cube: {desc}")
    w3 = run_w3_parallel_scaling(cube, desc)
    w6 = run_w6_nwp_loop_overhead(cube)

    payload = {
        "benchmark": "ADR 0025 Parallel Staging & Async Writer (W3 & W6)",
        "scratch_root": str(SCRATCH_ROOT),
        "w3_parallel_scaling": w3,
        "w6_nwp_loop": w6,
    }
    RESULTS_JSON.parent.mkdir(parents=True, exist_ok=True)
    RESULTS_JSON.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    print(f"\nSaved ADR 0025 benchmark report to {RESULTS_JSON}")


if __name__ == "__main__":
    main()
