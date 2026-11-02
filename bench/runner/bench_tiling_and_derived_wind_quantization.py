#!/usr/bin/env python3
"""Benchmark Spatial Tiling vs Horizontal Chunking & Derived Wind Physics Quantization.

Part 1: Chunk Topology Optimization
- Horizontal Map Chunking: [1, 721, 1440]
- Spatially Tiled Chunking: [120, 32, 32]
- Workloads: Full 2D Map (R2), Single Station 120-Step Time-Series (R4), Regional Box Europe (R5)

Part 2: Derived Meteorological Quantities under Wind Quantization
- Lossless Ground Truth
- Lossy 0.01 m/s (profile="output", bound=0.01)
- Lossy 0.05 m/s (profile="output", bound=0.05)
- Physics evaluated:
  * Wind Speed: V = sqrt(u^2 + v^2)
  * Meteorological Wind Direction: 0..360 deg
  * Relative Vorticity on Spherical Earth: zeta = (1 / (R*cos(phi))) * (dv/dlambda - d(u*cos(phi))/dphi)
"""

from __future__ import annotations

import time
from pathlib import Path
import netCDF4 as nc
import numpy as np
import wx1

NC_FILE = Path("/mnt/d/Weer/Uitvoer/data/era5/sfc/era5_sfc_instant.nc")
OUTPUT_DIR = Path("/tmp/tiling_and_physics_bench")
OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

R_EARTH = 6371000.0  # Earth radius in meters


def main():
    print("=" * 80)
    print("BENCHMARK: CHUNK TOPOLOGY & DERIVED WIND PHYSICS UNDER QUANTIZATION")
    print("=" * 80)

    assert NC_FILE.exists(), f"Missing {NC_FILE}"
    with nc.Dataset(NC_FILE, "r") as ds:
        u_raw = np.asarray(ds.variables["u10"][:], dtype=np.float32)
        v_raw = np.asarray(ds.variables["v10"][:], dtype=np.float32)
        lats = np.asarray(ds.variables["latitude"][:], dtype=np.float64)
        lons = np.asarray(ds.variables["longitude"][:], dtype=np.float64)

    print(f"Dataset shape: {u_raw.shape} ({u_raw.size:,} points per variable)")

    # =========================================================================
    # PART 1: Spatial Tiling vs Horizontal Chunking
    # =========================================================================
    print("\n" + "=" * 80)
    print("PART 1: SPATIAL TILING VS HORIZONTAL CHUNKING BENCHMARK")
    print("=" * 80)

    f_map_path = OUTPUT_DIR / "u10_map_chunks.wx1"
    f_tile_path = OUTPUT_DIR / "u10_tile_chunks.wx1"

    for p in [f_map_path, f_tile_path]:
        if p.exists():
            p.unlink()

    print("Writing horizontal map-optimized container [1, 721, 1440]...")
    wx1.write_array(
        f_map_path,
        u_raw,
        dataset_id="u10_map",
        variable_path="/u10",
        dimensions=["time", "latitude", "longitude"],
        chunk_shape=[1, 721, 1440],
        codec="zstd_shuffle",
        durability="buffered",
    )

    print("Writing station-optimized spatially tiled container [120, 32, 32]...")
    wx1.write_array(
        f_tile_path,
        u_raw,
        dataset_id="u10_tile",
        variable_path="/u10",
        dimensions=["time", "latitude", "longitude"],
        chunk_shape=[120, 32, 32],
        codec="zstd_shuffle",
        durability="buffered",
    )

    size_map = f_map_path.stat().st_size / 1e6
    size_tile = f_tile_path.stat().st_size / 1e6
    print(f"File size - Map chunks [1, 721, 1440]:   {size_map:.2f} MB")
    print(f"File size - Tiled chunks [120, 32, 32]:  {size_tile:.2f} MB")

    f_map = wx1.open_file(f_map_path)
    f_tile = wx1.open_file(f_tile_path)

    # Workload R2: Full 2D Map Access at t=60
    # Map container reads 1 chunk (4.15 MB raw)
    # Tiled container reads 23 x 45 = 1035 chunks (each contains 120 steps)
    times_r2_map = []
    times_r2_tile = []
    for _ in range(10):
        t0 = time.perf_counter()
        _sl = f_map.read_array_slice("/u10", start=[60, 0, 0], count=[1, 721, 1440])
        times_r2_map.append((time.perf_counter() - t0) * 1000)

        t0 = time.perf_counter()
        _sl = f_tile.read_array_slice("/u10", start=[60, 0, 0], count=[1, 721, 1440])
        times_r2_tile.append((time.perf_counter() - t0) * 1000)

    med_r2_map = float(np.median(times_r2_map))
    med_r2_tile = float(np.median(times_r2_tile))

    # Workload R4: Single Station 120-Step Time-Series (De Bilt: lat=52.1N -> 152, lon=5.18E -> 21)
    # Map container must read 120 separate chunks (all across the file)
    # Tiled container reads exactly 1 chunk [120, 32, 32]!
    times_r4_map = []
    times_r4_tile = []
    for _ in range(10):
        t0 = time.perf_counter()
        ts_m = f_map.read_array_slice("/u10", start=[0, 152, 21], count=[120, 1, 1])
        times_r4_map.append((time.perf_counter() - t0) * 1000)

        t0 = time.perf_counter()
        ts_t = f_tile.read_array_slice("/u10", start=[0, 152, 21], count=[120, 1, 1])
        times_r4_tile.append((time.perf_counter() - t0) * 1000)

    med_r4_map = float(np.median(times_r4_map))
    med_r4_tile = float(np.median(times_r4_tile))

    # Workload R5: Regional Box Europe (lat 45..55N -> 140..180, lon 0..10E -> 0..40, 120 timesteps)
    # Box shape: [120, 41, 41]
    times_r5_map = []
    times_r5_tile = []
    for _ in range(5):
        t0 = time.perf_counter()
        _sl = f_map.read_array_slice("/u10", start=[0, 140, 0], count=[120, 41, 41])
        times_r5_map.append((time.perf_counter() - t0) * 1000)

        t0 = time.perf_counter()
        _sl = f_tile.read_array_slice("/u10", start=[0, 140, 0], count=[120, 41, 41])
        times_r5_tile.append((time.perf_counter() - t0) * 1000)

    med_r5_map = float(np.median(times_r5_map))
    med_r5_tile = float(np.median(times_r5_tile))

    print(f"\n[Workload R2: Full 2D Map (721x1440)]")
    print(f"  Map-optimized [1, 721, 1440]:    {med_r2_map:7.2f} ms")
    print(f"  Spatially tiled [120, 32, 32]:  {med_r2_tile:7.2f} ms")
    print(f"  -> Map chunking is {med_r2_tile / med_r2_map:.1f}x FASTER for 2D maps!")

    print(f"\n[Workload R4: Single Station 120-Step Time-Series (De Bilt)]")
    print(f"  Map-optimized [1, 721, 1440]:    {med_r4_map:7.2f} ms")
    print(f"  Spatially tiled [120, 32, 32]:  {med_r4_tile:7.2f} ms")
    print(f"  -> Spatially tiled chunking is {med_r4_map / med_r4_tile:.1f}x FASTER for station time-series!")

    print(f"\n[Workload R5: Regional Box Europe 120 Steps (41x41)]")
    print(f"  Map-optimized [1, 721, 1440]:    {med_r5_map:7.2f} ms")
    print(f"  Spatially tiled [120, 32, 32]:  {med_r5_tile:7.2f} ms")
    print(f"  -> Spatially tiled chunking is {med_r5_map / med_r5_tile:.1f}x FASTER for regional 3D extraction!")

    # Cleanup temp files
    f_map_path.unlink()
    f_tile_path.unlink()

    # =========================================================================
    # PART 2: Derived Meteorological Quantities under Wind Quantization
    # =========================================================================
    print("\n" + "=" * 80)
    print("PART 2: DERIVED METEOROLOGICAL QUANTITIES UNDER WIND QUANTIZATION")
    print("=" * 80)

    # Create lossy containers for u and v
    bounds = [0.01, 0.05]
    lossy_u = {}
    lossy_v = {}

    for b in bounds:
        p_u = OUTPUT_DIR / f"u10_lossy_{b}.wx1"
        p_v = OUTPUT_DIR / f"v10_lossy_{b}.wx1"
        for p in [p_u, p_v]:
            if p.exists():
                p.unlink()

        wx1.write_array(
            p_u, u_raw, dataset_id=f"u_{b}", variable_path="/u10",
            dimensions=["time", "lat", "lon"], chunk_shape=[3, 721, 1440],
            codec="zstd_shuffle", profile="output", lossy_error_bound=b, durability="buffered"
        )
        wx1.write_array(
            p_v, v_raw, dataset_id=f"v_{b}", variable_path="/v10",
            dimensions=["time", "lat", "lon"], chunk_shape=[3, 721, 1440],
            codec="zstd_shuffle", profile="output", lossy_error_bound=b, durability="buffered"
        )

        f_u = wx1.open_file(p_u)
        f_v = wx1.open_file(p_v)
        lossy_u[b] = f_u.read_array("/u10")
        lossy_v[b] = f_v.read_array("/v10")

        p_u.unlink()
        p_v.unlink()

    # 1. Direct wind error check
    for b in bounds:
        err_u = np.max(np.abs(u_raw - lossy_u[b]))
        err_v = np.max(np.abs(v_raw - lossy_v[b]))
        print(f"Declared bound: {b} m/s -> Actual max error: u = {err_u:.6e} m/s, v = {err_v:.6e} m/s")

    # 2. Wind Speed: V = sqrt(u^2 + v^2)
    print("\n--- 2. Wind Speed: V = sqrt(u^2 + v^2) ---")
    ws_truth = np.sqrt(u_raw**2 + v_raw**2)
    print(f"Ground truth wind speed range: min={ws_truth.min():.2f} m/s, max={ws_truth.max():.2f} m/s, mean={ws_truth.mean():.2f} m/s")

    for b in bounds:
        ws_lossy = np.sqrt(lossy_u[b]**2 + lossy_v[b]**2)
        diff_ws = np.abs(ws_truth - ws_lossy)
        max_err_ws = float(np.max(diff_ws))
        rmse_ws = float(np.sqrt(np.mean(diff_ws**2)))
        print(f"Bound {b} m/s:")
        print(f"  Max Wind Speed Error: {max_err_ws:.6f} m/s (rel error: {max_err_ws / ws_truth.max() * 100:.3f}%)")
        print(f"  RMSE Wind Speed:      {rmse_ws:.6f} m/s")

    # 3. Meteorological Wind Direction: dir = (270 - atan2(v, u)*180/pi) % 360
    # Evaluate where wind speed > 1.0 m/s (excluding calm wind where angle is ill-conditioned)
    print("\n--- 3. Wind Direction (Degrees, for wind speed > 1.0 m/s) ---")
    mask_active = ws_truth > 1.0
    dir_truth = (270.0 - np.rad2deg(np.arctan2(v_raw, u_raw))) % 360.0

    for b in bounds:
        dir_lossy = (270.0 - np.rad2deg(np.arctan2(lossy_v[b], lossy_u[b]))) % 360.0
        # Angular difference taking 360 wrap-around into account
        ang_diff = np.abs(dir_truth - dir_lossy)
        ang_diff = np.where(ang_diff > 180.0, 360.0 - ang_diff, ang_diff)
        diff_dir_active = ang_diff[mask_active]

        max_err_dir = float(np.max(diff_dir_active))
        mean_err_dir = float(np.mean(diff_dir_active))
        p99_err_dir = float(np.percentile(diff_dir_active, 99.0))
        print(f"Bound {b} m/s:")
        print(f"  Mean Direction Error: {mean_err_dir:.3f}°")
        print(f"  99th percentile:      {p99_err_dir:.3f}°")
        print(f"  Max Direction Error:  {max_err_dir:.3f}°")

    # 4. Spherical Relative Vorticity: zeta = 1/(R*cos(phi)) * (dv/dlambda - d(u*cos(phi))/dphi)
    print("\n--- 4. Relative Vorticity on Spherical Earth (1/s) ---")
    # Evaluate at single synoptic timestep t=0 across all lat/lon
    t_idx = 0
    u0 = u_raw[t_idx].astype(np.float64)
    v0 = v_raw[t_idx].astype(np.float64)

    phi = np.deg2rad(lats)  # shape (721,)
    cos_phi = np.cos(phi)
    # Clip cos_phi near poles to avoid division by zero (between 89N and 89S)
    valid_lat = np.abs(lats) < 88.0

    dphi = np.deg2rad(np.abs(lats[1] - lats[0]))  # 0.25 deg in rad
    dlambda = np.deg2rad(np.abs(lons[1] - lons[0]))

    def calc_vorticity(u_field, v_field):
        # dv / dlambda (along axis 1, longitude)
        dv_dlambda = np.gradient(v_field, dlambda, axis=1)

        # d(u * cos(phi)) / dphi (along axis 0, latitude)
        # Note: lats are ordered 90 to -90, so phi decreases with index
        u_cos = u_field * cos_phi[:, None]
        # Since lats decrease with index, d(u_cos)/dphi = -gradient along axis 0 / dphi
        du_cos_dphi = -np.gradient(u_cos, dphi, axis=0)

        # zeta = (dv_dlambda - du_cos_dphi) / (R * cos(phi))
        zeta = (dv_dlambda - du_cos_dphi) / (R_EARTH * cos_phi[:, None])
        return zeta

    vort_truth = calc_vorticity(u0, v0)
    vort_valid_truth = vort_truth[valid_lat, :]

    vort_max = float(np.max(np.abs(vort_valid_truth)))
    vort_std = float(np.std(vort_valid_truth))
    print(f"Ground truth vorticity scale: max={vort_max:.4e} 1/s, std={vort_std:.4e} 1/s (typical synoptic: ~1e-4 1/s)")

    for b in bounds:
        u_loss = lossy_u[b][t_idx].astype(np.float64)
        v_loss = lossy_v[b][t_idx].astype(np.float64)
        vort_lossy = calc_vorticity(u_loss, v_loss)

        diff_vort = np.abs(vort_truth[valid_lat, :] - vort_lossy[valid_lat, :])
        max_err_vort = float(np.max(diff_vort))
        rmse_vort = float(np.sqrt(np.mean(diff_vort**2)))
        rel_rmse = (rmse_vort / vort_std) * 100.0

        print(f"Bound {b} m/s:")
        print(f"  Max Vorticity Error:  {max_err_vort:.4e} 1/s")
        print(f"  RMSE Vorticity Error: {rmse_vort:.4e} 1/s (Relative RMSE: {rel_rmse:.2f}% of physical vorticity std)")


if __name__ == "__main__":
    main()
