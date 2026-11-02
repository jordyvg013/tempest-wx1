#!/usr/bin/env python3
"""ADR 0026 Rigorous Benchmark Suite:
1. Realistic noisy 3D meteorological float32 fields (seeded from real ECMWF 0.1 oper/ENFO fields on D:
   plus multi-scale baroclinic wave + realistic full-mantissa turbulent perturbations) on C96, C192 (60 levels),
   and C384.
2. W5 in the formal specification definition:
   - Multi-timestep cubed-sphere append loop (C192, 60 vertical levels, 6 tiles, halo=3 -> shape 6x60x198x198 = 56.45 MB/step).
   - Fixed-duration NWP model compute step (stub model step = 500.0 ms per step).
   - Measures per-step I/O stall on the model thread as % of model step time, both Buffered and Durable.
3. Cross-format R9 tile reading comparison on identical C192 60-level cubed-sphere data:
   - WX-1 (`zstd_shuffle`)
   - Zarr 3 (`BloscCodec(cname="zstd", clevel=3, shuffle="shuffle")`)
   - NetCDF4 (`zlib=True, complevel=4, shuffle=True`)
   Measures R1 (open), R9 (read 1 tile interior vs with halos), and R9 (read all 6 tiles), with median & p95.

Strictly stores all temporary and output files on the Windows D: drive
(/mnt/d/Weer/Uitvoer/data/wx1_bench_scratch) and NEVER on the WSL C: virtual disk.
"""

from __future__ import annotations

import json
import os
import shutil
import sys
import time
from pathlib import Path

import netCDF4 as nc
import numpy as np
import zarr

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "python"))

import wx1  # noqa: E402


SCRATCH_ROOT = Path(os.environ.get("WX1_SCRATCH", "/mnt/d/Weer/Uitvoer/data/wx1_bench_scratch"))
RESULTS_JSON = ROOT / "bench" / "results" / "adr0026_cubed_sphere_w5_r9.json"


def get_dir_or_file_size(path: Path) -> int:
    if path.is_file():
        return path.stat().st_size
    if path.is_dir():
        return sum(f.stat().st_size for f in path.rglob("*") if f.is_file())
    return 0


def make_realistic_noisy_cubed_sphere(n_levels: int, c_res: int, halo: int, seed: int = 2026) -> tuple[np.ndarray, str]:
    """Build a realistic 4D cubed-sphere float32 field (6, n_levels, c_res+2*halo, c_res+2*halo)
    by tiling real ECMWF 0.1 NH pressure-level temperature anomalies from D:\\ plus full-mantissa
    Gaussian sub-grid turbulence so all 23 mantissa bits are active (not a smooth synthetic cosine).
    """
    ny = c_res + 2 * halo
    nx = c_res + 2 * halo
    rng = np.random.RandomState(seed)

    real_nc = Path("/mnt/d/Weer/Uitvoer/data/ecmwf/20261029_12z_oper_allvars/crop_nh_allvars/pl/test_formats/ecmwf_nh_pl_t14_uncompressed.nc")
    arr = np.empty((6, n_levels, ny, nx), dtype="<f4")

    if real_nc.exists():
        with nc.Dataset(real_nc, "r") as ds:
            real_pl = np.asarray(ds.variables["field"][:], dtype="<f4")  # (14, 901, 3600)
        for tile in range(6):
            y0 = (tile * 110) % max(1, (real_pl.shape[1] - ny))
            x0 = (tile * 550) % max(1, (real_pl.shape[2] - nx))
            for k in range(n_levels):
                src_k = k % real_pl.shape[0]
                patch = real_pl[src_k, y0:y0 + ny, x0:x0 + nx]
                # Add vertical lapse rate + continuous float32 NWP sub-grid turbulence (sigma=0.15 K)
                noise = rng.normal(0.0, 0.15, (ny, nx)).astype("<f4")
                arr[tile, k] = patch - np.float32(k * 0.65) + noise
        desc = f"Real ECMWF 0.1 NH PL Patches + Continuous NWP Turbulence (6x{n_levels}x{ny}x{nx})"
    else:
        for tile in range(6):
            for k in range(n_levels):
                arr[tile, k] = rng.normal(265.0 - k * 0.7, 8.5, (ny, nx)).astype("<f4")
        desc = f"Synthetic Noisy Gaussian Turbulence (6x{n_levels}x{ny}x{nx})"

    return arr, desc


def run_r9_cross_format_comparison(c_res: int = 192, n_levels: int = 60, halo: int = 3, reps: int = 7) -> dict:
    """Compare WX-1, Zarr 3 (Blosc+ByteShuffle), and NetCDF4 (zlib4+shuffle) on C192 L60 cubed-sphere."""
    out_dir = SCRATCH_ROOT / f"adr0026_cross_C{c_res}_L{n_levels}"
    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    arr, data_desc = make_realistic_noisy_cubed_sphere(n_levels=n_levels, c_res=c_res, halo=halo)
    ny, nx = arr.shape[2], arr.shape[3]
    raw_bytes = int(arr.nbytes)
    # 1 chunk per (tile, 15 levels, ny, nx) -> 6 * 4 = 24 chunks (~2.35 MB raw per chunk)
    chunk_shape = [1, 15, ny, nx]

    wx1_path = out_dir / f"cs_C{c_res}_L{n_levels}.wx1"
    zarr_shuf_path = out_dir / f"cs_C{c_res}_L{n_levels}_blosc_shuffle.zarr"
    nc_zlib_path = out_dir / f"cs_C{c_res}_L{n_levels}_zlib4.nc"

    # 1. Write WX-1 (8 workers, zstd_shuffle)
    t0 = time.perf_counter()
    wx1.write_array(
        wx1_path,
        arr,
        dataset_id=f"cs-C{c_res}-L{n_levels}",
        variable_path="/t",
        dimensions=["tile", "lev", "y", "x"],
        chunk_shape=chunk_shape,
        codec="zstd_shuffle",
        workers=8,
        durability="durable",
        grid_family="cubed_sphere",
        c_resolution=c_res,
        halo_width=halo,
        stagger_horizontal="center",
        stagger_vertical="full_level",
        tile_dimension="tile",
    )
    w_wx1_ms = (time.perf_counter() - t0) * 1000.0

    # 2. Write Zarr 3 with Blosc (zstd level 3 + byte shuffle)
    t0 = time.perf_counter()
    z_arr = zarr.create_array(
        store=str(zarr_shuf_path),
        shape=arr.shape,
        chunks=tuple(chunk_shape),
        dtype="<f4",
        compressors=zarr.codecs.BloscCodec(cname="zstd", clevel=3, shuffle="shuffle"),
        overwrite=True,
    )
    z_arr[:] = arr
    w_zarr_ms = (time.perf_counter() - t0) * 1000.0

    # 3. Write NetCDF4 (zlib complevel=4 + shuffle)
    t0 = time.perf_counter()
    with nc.Dataset(nc_zlib_path, "w", format="NETCDF4") as ds:
        for d, s in zip(["tile", "lev", "y", "x"], arr.shape):
            ds.createDimension(d, s)
        v = ds.createVariable(
            "t", "f4", ("tile", "lev", "y", "x"),
            chunksizes=tuple(chunk_shape), zlib=True, complevel=4, shuffle=True
        )
        v[:] = arr
    w_nc_ms = (time.perf_counter() - t0) * 1000.0

    formats_out = {}

    def bench_one(fmt_name: str, path: Path, write_ms: float, open_fn, read_1tile_int_fn, read_1tile_full_fn, read_all6_fn):
        r1_open, r9_int, r9_full, r9_all6 = [], [], [], []
        for r in range(reps):
            tile_id = r % 6
            t0 = time.perf_counter()
            h = open_fn()
            r1_open.append((time.perf_counter() - t0) * 1000.0)

            # Read 1 tile interior FIRST (before full read warms any HDF5 chunk cache)
            t0 = time.perf_counter()
            sub_int = read_1tile_int_fn(h, tile_id)
            r9_int.append((time.perf_counter() - t0) * 1000.0)

            if hasattr(h, "close"):
                h.close()

            # Reopen fresh handle for 1 tile with halos
            h2 = open_fn()
            t0 = time.perf_counter()
            sub_full = read_1tile_full_fn(h2, tile_id)
            r9_full.append((time.perf_counter() - t0) * 1000.0)

            # Read all 6 tiles
            t0 = time.perf_counter()
            all6 = read_all6_fn(h2)
            r9_all6.append((time.perf_counter() - t0) * 1000.0)
            if hasattr(h2, "close"):
                h2.close()

        assert sub_int.shape[-3:] == (n_levels, c_res, c_res)
        assert sub_full.shape[-3:] == (n_levels, ny, nx)
        assert np.array_equal(all6.view("u4"), arr.view("u4"))

        fsize = get_dir_or_file_size(path)
        formats_out[fmt_name] = {
            "size_bytes": fsize,
            "pct_of_raw": round(fsize / raw_bytes * 100.0, 2),
            "write_ms": round(write_ms, 2),
            "r1_open_med_ms": round(float(np.median(r1_open)), 3),
            "r1_open_p95_ms": round(float(np.percentile(r1_open, 95)), 3),
            "r9_1tile_interior_med_ms": round(float(np.median(r9_int)), 3),
            "r9_1tile_interior_p95_ms": round(float(np.percentile(r9_int, 95)), 3),
            "r9_1tile_with_halos_med_ms": round(float(np.median(r9_full)), 3),
            "r9_1tile_with_halos_p95_ms": round(float(np.percentile(r9_full, 95)), 3),
            "r9_all_6tiles_med_ms": round(float(np.median(r9_all6)), 3),
            "r9_all_6tiles_p95_ms": round(float(np.percentile(r9_all6, 95)), 3),
        }
        print(
            f"[R9 Cross-Format C{c_res} L{n_levels}] {fmt_name:22s}: "
            f"size={fsize/1e6:6.2f} MB ({fsize/raw_bytes*100:5.1f}%) | "
            f"R1={formats_out[fmt_name]['r1_open_med_ms']:5.2f} ms | "
            f"R9_1tile_int={formats_out[fmt_name]['r9_1tile_interior_med_ms']:6.2f} ms | "
            f"R9_all6={formats_out[fmt_name]['r9_all_6tiles_med_ms']:6.2f} ms"
        )

    bench_one(
        "WX1_ZSTD_SHUFFLE",
        wx1_path,
        w_wx1_ms,
        lambda: wx1.open_file(wx1_path),
        lambda wf, tid: wf.read_tile_slice("/t", tid, include_halos=False),
        lambda wf, tid: wf.read_tile_slice("/t", tid, include_halos=True),
        lambda wf: wf.read_array("/t"),
    )
    bench_one(
        "ZARR3_BLOSC_SHUFFLE",
        zarr_shuf_path,
        w_zarr_ms,
        lambda: zarr.open_array(str(zarr_shuf_path), mode="r"),
        lambda za, tid: np.asarray(za[tid, :, halo:halo + c_res, halo:halo + c_res]),
        lambda za, tid: np.asarray(za[tid, :, :, :]),
        lambda za: np.asarray(za[:]),
    )
    bench_one(
        "NETCDF4_ZLIB4_SHUFFLE",
        nc_zlib_path,
        w_nc_ms,
        lambda: nc.Dataset(nc_zlib_path, "r"),
        lambda ds, tid: np.asarray(ds.variables["t"][tid, :, halo:halo + c_res, halo:halo + c_res]),
        lambda ds, tid: np.asarray(ds.variables["t"][tid, :, :, :]),
        lambda ds: np.asarray(ds.variables["t"][:]),
    )

    shutil.rmtree(out_dir, ignore_errors=True)
    return {
        "data_source": data_desc,
        "c_resolution": c_res,
        "n_levels": n_levels,
        "halo_width": halo,
        "shape": list(arr.shape),
        "chunk_shape": chunk_shape,
        "raw_bytes": raw_bytes,
        "repeats": reps,
        "formats": formats_out,
    }


def run_w5_append_stall_benchmark(c_res: int = 192, n_levels: int = 60, halo: int = 3, n_steps: int = 6, model_step_ms: float = 500.0) -> dict:
    """Measure W5 per-timestep append on C192 L60 (6x60x198x198 = 56.45 MB/step):
    - Stub model step with fixed compute duration `model_step_ms` (500.0 ms).
    - Measures per-step I/O stall on the model thread as % of `model_step_ms`, for both
      Buffered and Durable async commits (8 staging workers).
    """
    w5_dir = SCRATCH_ROOT / f"adr0026_w5_C{c_res}_L{n_levels}"
    if w5_dir.exists():
        shutil.rmtree(w5_dir)
    w5_dir.mkdir(parents=True, exist_ok=True)

    arr, data_desc = make_realistic_noisy_cubed_sphere(n_levels=n_levels, c_res=c_res, halo=halo)
    ny, nx = arr.shape[2], arr.shape[3]
    step_raw_bytes = int(arr.nbytes)
    spatial_shape = [6, n_levels, ny, nx]
    spatial_chunk_shape = [1, 15, ny, nx]  # 24 chunks per step

    modes = {}
    for dur in ("buffered", "durable"):
        out_path = w5_dir / f"w5_append_{dur}.wx1"
        out_path.unlink(missing_ok=True)

        step_stall_ms_list = []
        step_total_ms_list = []

        t_loop_start = time.perf_counter()
        with wx1.AsyncWriter(
            out_path,
            dataset_id=f"w5-c{c_res}-l{n_levels}-{dur}",
            variable_path="/t",
            spatial_dimensions=["tile", "lev", "y", "x"],
            spatial_shape=spatial_shape,
            spatial_chunk_shape=spatial_chunk_shape,
            codec="zstd_shuffle",
            workers=8,
            max_pending_generations=4,
            max_queued_bytes=512 * 1024 * 1024,
            durability=dur,
        ) as writer:
            for step in range(n_steps):
                t_step0 = time.perf_counter()
                # Fixed stub model step of `model_step_ms` (500 ms)
                target_end = t_step0 + (model_step_ms / 1000.0)
                # Mutate 1 slice slightly so each step has distinct data
                arr[0, 0, 0, 0] = np.float32(270.0 + step)
                while time.perf_counter() < target_end:
                    time.sleep(0.002)
                t_compute_done = time.perf_counter()
                compute_actual_ms = (t_compute_done - t_step0) * 1000.0

                # Submit step to AsyncWriter (parallel staging + enqueue)
                t_sub0 = time.perf_counter()
                writer.submit_step(arr)
                sub_ms = (time.perf_counter() - t_sub0) * 1000.0

                step_stall_ms_list.append(sub_ms)
                step_total_ms_list.append(compute_actual_ms + sub_ms)

            tel = writer.flush()
        total_wall_ms = (time.perf_counter() - t_loop_start) * 1000.0
        file_bytes = out_path.stat().st_size

        with wx1.open_file(out_path) as wf:
            assert wf.generation == n_steps

        out_path.unlink(missing_ok=True)

        med_stall_ms = float(np.median(step_stall_ms_list))
        p95_stall_ms = float(np.percentile(step_stall_ms_list, 95))
        stall_pct_med = (med_stall_ms / model_step_ms) * 100.0
        stall_pct_p95 = (p95_stall_ms / model_step_ms) * 100.0
        queue_backpressure_ms = tel["total_enqueue_stall_ns"] / 1e6

        modes[dur] = {
            "durability": dur,
            "n_steps": n_steps,
            "stub_model_step_ms": model_step_ms,
            "per_step_io_stall_ms": [round(x, 2) for x in step_stall_ms_list],
            "median_io_stall_ms": round(med_stall_ms, 2),
            "p95_io_stall_ms": round(p95_stall_ms, 2),
            "median_io_stall_pct_of_step": round(stall_pct_med, 2),
            "p95_io_stall_pct_of_step": round(stall_pct_p95, 2),
            "queue_backpressure_stall_ms": round(queue_backpressure_ms, 3),
            "total_loop_wall_ms": round(total_wall_ms, 2),
            "effective_throughput_gb_s": round((step_raw_bytes * n_steps / 1e9) / (total_wall_ms / 1000.0), 3),
            "committed_file_bytes": file_bytes,
            "telemetry": tel,
        }
        print(
            f"[W5 Append C{c_res} L{n_levels} ({dur:8s})] step_raw={step_raw_bytes/1e6:.2f} MB | "
            f"model_step={model_step_ms:.0f} ms | med_io_stall={med_stall_ms:.2f} ms ({stall_pct_med:.2f}%) | "
            f"queue_wait={queue_backpressure_ms:.3f} ms"
        )

    shutil.rmtree(w5_dir, ignore_errors=True)
    return {
        "data_source": data_desc,
        "c_resolution": c_res,
        "n_levels": n_levels,
        "halo_width": halo,
        "step_shape": spatial_shape,
        "step_raw_bytes": step_raw_bytes,
        "modes": modes,
    }


def run_scaling_c96_c192_c384() -> list[dict]:
    """Run noisy float32 scaling across C96 (60L), C192 (60L), and C384 (20L)."""
    out = []
    for c_res, n_lev in [(96, 60), (192, 60), (384, 20)]:
        arr, desc = make_realistic_noisy_cubed_sphere(n_levels=n_lev, c_res=c_res, halo=3)
        ny, nx = arr.shape[2], arr.shape[3]
        raw_bytes = int(arr.nbytes)
        chunk_shape = [1, min(15, n_lev), min(ny, 198), min(nx, 198)]
        p = SCRATCH_ROOT / f"scale_C{c_res}_L{n_lev}.wx1"
        p.unlink(missing_ok=True)
        t0 = time.perf_counter()
        wx1.write_array(
            p,
            arr,
            dataset_id=f"scale-c{c_res}",
            variable_path="/t",
            dimensions=["tile", "lev", "y", "x"],
            chunk_shape=chunk_shape,
            codec="zstd_shuffle",
            workers=8,
            durability="buffered",
            grid_family="cubed_sphere",
            c_resolution=c_res,
            halo_width=3,
            stagger_horizontal="center",
            stagger_vertical="full_level",
            tile_dimension="tile",
        )
        w_ms = (time.perf_counter() - t0) * 1000.0
        fsize = p.stat().st_size
        with wx1.open_file(p) as wf:
            t0 = time.perf_counter()
            _ = wf.read_tile_slice("/t", 0, include_halos=False)
            r9_1tile_ms = (time.perf_counter() - t0) * 1000.0
        p.unlink(missing_ok=True)
        item = {
            "c_resolution": c_res,
            "n_levels": n_lev,
            "cells_per_level_6n2": 6 * c_res * c_res,
            "allocated_shape": list(arr.shape),
            "raw_bytes": raw_bytes,
            "compressed_bytes": fsize,
            "pct_of_raw": round(fsize / raw_bytes * 100.0, 2),
            "write_8w_buffered_ms": round(w_ms, 2),
            "write_throughput_mb_s": round((raw_bytes / 1e6) / (w_ms / 1000.0), 2),
            "r9_1tile_interior_ms": round(r9_1tile_ms, 3),
        }
        print(
            f"[Noisy Scaling C{c_res:3d} L{n_lev:2d}] raw={raw_bytes/1e6:6.2f} MB -> {fsize/1e6:6.2f} MB ({item['pct_of_raw']:5.1f}%) | "
            f"write={w_ms:6.2f} ms ({item['write_throughput_mb_s']:6.1f} MB/s) | R9_1tile={r9_1tile_ms:5.2f} ms"
        )
        out.append(item)
    return out


def main():
    SCRATCH_ROOT.mkdir(parents=True, exist_ok=True)
    r9_cross = run_r9_cross_format_comparison(c_res=192, n_levels=60, halo=3, reps=7)
    w5_stall = run_w5_append_stall_benchmark(c_res=192, n_levels=60, halo=3, n_steps=6, model_step_ms=500.0)
    scaling = run_scaling_c96_c192_c384()

    payload = {
        "benchmark": "ADR 0026 Rigorous Cubed-Sphere W5 Stall & R9 Cross-Format Benchmark",
        "scratch_root": str(SCRATCH_ROOT),
        "r9_cross_format_c192_l60": r9_cross,
        "w5_append_stall_c192_l60": w5_stall,
        "noisy_scaling_c96_c192_c384": scaling,
    }
    RESULTS_JSON.parent.mkdir(parents=True, exist_ok=True)
    RESULTS_JSON.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    print(f"\nSaved rigorous ADR 0026 benchmark results to {RESULTS_JSON}")


if __name__ == "__main__":
    main()
