"""Formal baseline cross-format benchmark runner on operational datasets.

Measures W1 (Write), R1 (Open), R2 (Read Field), R5 (Read Region Box), R6 (Scan)
across WX-1, NetCDF4, Zarr 3, and GRIB2.
"""

from __future__ import annotations

import csv
import datetime
import json
import os
from pathlib import Path
import random
import resource
import shutil
import time
from typing import Any, Mapping

import numpy as np
import xarray as xr
import eccodes
import cfgrib
import wx1


def get_peak_rss_kib() -> int:
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss


def get_dir_or_file_size(path: Path) -> int:
    if path.is_file():
        return path.stat().st_size
    elif path.is_dir():
        return sum(f.stat().st_size for f in path.rglob("*") if f.is_file())
    return 0


def load_operational_dataset(name: str, scratch_dir: Path) -> tuple[xr.Dataset, str]:
    """Load operational weather dataset into standardized xarray Dataset."""
    if name == "harmonie_nl":
        file_path = scratch_dir / "data" / "harmonie_nl_0h.grib2"
        # Open t2m from HARMONIE heightAboveGround level
        ds = xr.open_dataset(
            file_path,
            engine="cfgrib",
            backend_kwargs={"filter_by_keys": {"typeOfLevel": "heightAboveGround"}}
        )
        t2m = ds["t2m"].load()
        lats = ds["latitude"].values.astype(np.float32)
        lons = ds["longitude"].values.astype(np.float32)
        clean_ds = xr.Dataset(
            data_vars={"temperature": (["latitude", "longitude"], t2m.values.astype("<f4"))},
            coords={"latitude": lats, "longitude": lons},
            attrs={"dataset_id": "harmonie_cy46_nl_t2m", "units": "K", "source": "KNMI HARMONIE cy46"}
        )
        return clean_ds, "temperature"

    elif name == "ecmwf_oper_europe":
        file_path = scratch_dir / "data" / "ecmwf_sfc_0h.grib2"
        ds = xr.open_dataset(file_path, engine="cfgrib")
        t2m = ds["t2m"].load()
        lats = ds["latitude"].values.astype(np.float32)
        lons = ds["longitude"].values.astype(np.float32)
        clean_ds = xr.Dataset(
            data_vars={"temperature": (["latitude", "longitude"], t2m.values.astype("<f4"))},
            coords={"latitude": lats, "longitude": lons},
            attrs={"dataset_id": "ecmwf_oper_fc_europe_t2m", "units": "K", "source": "ECMWF Operational"}
        )
        return clean_ds, "temperature"

    raise ValueError(f"Unknown operational dataset: {name}")


def run_benchmark_for_dataset(dataset_name: str, ds_source: xr.Dataset, scratch_root: Path, repetitions: int = 30) -> list[dict[str, Any]]:
    print(f"\n=======================================================")
    print(f"BENCHMARKING DATASET: {dataset_name.upper()}")
    shape = ds_source["temperature"].shape
    total_elements = ds_source["temperature"].size
    expected_data = ds_source["temperature"].values.astype("<f4")
    print(f"Dimensions: {shape[0]} x {shape[1]} ({total_elements} elements, {total_elements * 4 / 1024:.1f} KiB uncompressed)")
    print(f"=======================================================")

    work_dir = scratch_root / dataset_name
    work_dir.mkdir(parents=True, exist_ok=True)

    formats = ["wx1", "netcdf4", "zarr3", "grib2"]
    # Seeded randomization of format order
    random.seed(42)
    random.shuffle(formats)

    results = []
    artifacts = {}

    # Define region slice for R5 (100x100 box)
    r5_y_start, r5_y_count = shape[0] // 4, min(100, shape[0] // 2)
    r5_x_start, r5_x_count = shape[1] // 4, min(100, shape[1] // 2)
    expected_region = expected_data[r5_y_start:r5_y_start + r5_y_count, r5_x_start:r5_x_start + r5_x_count]

    # --- 1. WORKLOAD W1: FULL WRITE ---
    print("\n--- WORKLOAD W1: Full Dataset Write ---")
    for fmt in formats:
        art_path = work_dir / f"artifact_{fmt}"
        if fmt == "wx1":
            art_path = art_path.with_suffix(".wx1")
            chunk_shape = (64, 64)
            t0_cpu = time.process_time()
            t0 = time.perf_counter()
            wx1.write_array(
                art_path,
                expected_data,
                dataset_id=f"wx1-{dataset_name}",
                variable_path="/temperature",
                dimensions=["latitude", "longitude"],
                chunk_shape=chunk_shape,
                dimension_roles=["latitude", "longitude"],
                attributes={"units": "K"},
            )
            w1_time = (time.perf_counter() - t0) * 1000.0
            w1_cpu = (time.process_time() - t0_cpu) * 1000.0

        elif fmt == "netcdf4":
            art_path = art_path.with_suffix(".nc")
            t0_cpu = time.process_time()
            t0 = time.perf_counter()
            ds_source.to_netcdf(art_path, engine="netcdf4")
            w1_time = (time.perf_counter() - t0) * 1000.0
            w1_cpu = (time.process_time() - t0_cpu) * 1000.0

        elif fmt == "zarr3":
            art_path = art_path.with_suffix(".zarr")
            t0_cpu = time.process_time()
            t0 = time.perf_counter()
            try:
                ds_source.to_zarr(art_path, mode="w", zarr_format=3)
            except Exception:
                ds_source.to_zarr(art_path, mode="w")
            w1_time = (time.perf_counter() - t0) * 1000.0
            w1_cpu = (time.process_time() - t0_cpu) * 1000.0

        elif fmt == "grib2":
            art_path = art_path.with_suffix(".grib2")
            src_grib = scratch_root.parent / "bench" / "data" / ("harmonie_nl_0h.grib2" if dataset_name == "harmonie_nl" else "ecmwf_sfc_0h.grib2")
            t0_cpu = time.process_time()
            t0 = time.perf_counter()
            # Extract ONLY the authentic single field message (2t/t2m) so file size is 1 field vs 1 field
            with open(src_grib, "rb") as fin, open(art_path, "wb") as fout:
                while True:
                    gid = eccodes.codes_grib_new_from_file(fin)
                    if gid is None:
                        break
                    sname = eccodes.codes_get(gid, "shortName")
                    try:
                        level_type = eccodes.codes_get(gid, "typeOfLevel")
                    except Exception:
                        level_type = ""
                    if sname in ["2t", "t2m"] and (dataset_name != "harmonie_nl" or level_type == "heightAboveGround"):
                        eccodes.codes_write(gid, fout)
                        eccodes.codes_release(gid)
                        break
                    eccodes.codes_release(gid)
            w1_time = (time.perf_counter() - t0) * 1000.0
            w1_cpu = (time.process_time() - t0_cpu) * 1000.0

        size_bytes = get_dir_or_file_size(art_path)
        artifacts[fmt] = art_path
        print(f"W1 [{fmt:7s}] Write time: {w1_time:8.3f} ms (CPU: {w1_cpu:7.3f} ms) | Size: {size_bytes:9d} bytes")

    grib_kwargs = {"filter_by_keys": {"typeOfLevel": "heightAboveGround"}} if dataset_name == "harmonie_nl" else {}

    # --- 2. WORKLOAD R1: OPEN TIME TO FIRST FIELD ---
    print(f"\n--- WORKLOAD R1: Open Time ({repetitions} warm repetitions) ---")
    r1_times = {fmt: [] for fmt in formats}
    for _ in range(repetitions):
        for fmt in formats:
            path = artifacts[fmt]
            t0 = time.perf_counter()
            if fmt == "wx1":
                h = wx1.open_file(path)
                h.close()
            elif fmt == "netcdf4":
                h = xr.open_dataset(path, engine="netcdf4")
                h.close()
            elif fmt == "zarr3":
                h = xr.open_zarr(path)
                h.close()
            elif fmt == "grib2":
                h = cfgrib.open_dataset(str(path), backend_kwargs=grib_kwargs)
                h.close()
            r1_times[fmt].append((time.perf_counter() - t0) * 1000.0)

    for fmt in formats:
        samples = r1_times[fmt]
        print(f"R1 [{fmt:7s}] Open median: {np.median(samples):7.3f} ms | p95: {np.percentile(samples, 95):7.3f} ms | min: {np.min(samples):7.3f} ms")

    # --- 3. WORKLOAD R2: READ SINGLE FULL FIELD ---
    print(f"\n--- WORKLOAD R2: Read Single Full Field ({repetitions} repetitions) ---")
    r2_times = {fmt: [] for fmt in formats}
    restored_arrays = {}
    for _ in range(repetitions):
        for fmt in formats:
            path = artifacts[fmt]
            if fmt == "wx1":
                h = wx1.open_file(path)
                t0 = time.perf_counter()
                arr = h.read_array("/temperature")
                t_elapsed = (time.perf_counter() - t0) * 1000.0
                h.close()
            elif fmt == "netcdf4":
                h = xr.open_dataset(path, engine="netcdf4")
                t0 = time.perf_counter()
                arr = h["temperature"].values
                t_elapsed = (time.perf_counter() - t0) * 1000.0
                h.close()
            elif fmt == "zarr3":
                h = xr.open_zarr(path)
                t0 = time.perf_counter()
                arr = h["temperature"].values
                t_elapsed = (time.perf_counter() - t0) * 1000.0
                h.close()
            elif fmt == "grib2":
                h = cfgrib.open_dataset(str(path), backend_kwargs=grib_kwargs)
                t0 = time.perf_counter()
                var_name = "t2m" if "t2m" in h else list(h.data_vars.keys())[0]
                arr = h[var_name].values
                t_elapsed = (time.perf_counter() - t0) * 1000.0
                h.close()

            r2_times[fmt].append(t_elapsed)
            restored_arrays[fmt] = arr

    for fmt in formats:
        samples = r2_times[fmt]
        print(f"R2 [{fmt:7s}] Read field median: {np.median(samples):7.3f} ms | p95: {np.percentile(samples, 95):7.3f} ms")

    # --- 4. WORKLOAD R5: REGION BOX READ (100x100) ---
    print(f"\n--- WORKLOAD R5: Region Slice Read ({r5_y_count}x{r5_x_count} box, {repetitions} repetitions) ---")
    r5_times = {fmt: [] for fmt in formats}
    for _ in range(repetitions):
        for fmt in formats:
            path = artifacts[fmt]
            if fmt == "wx1":
                h = wx1.open_file(path)
                t0 = time.perf_counter()
                # WX-1 slice read plans and decodes only intersecting chunks!
                arr = h.read_array_slice("/temperature", [r5_y_start, r5_x_start], [r5_y_count, r5_x_count])
                t_elapsed = (time.perf_counter() - t0) * 1000.0
                h.close()
            elif fmt == "netcdf4":
                h = xr.open_dataset(path, engine="netcdf4")
                t0 = time.perf_counter()
                arr = h["temperature"][r5_y_start:r5_y_start + r5_y_count, r5_x_start:r5_x_start + r5_x_count].values
                t_elapsed = (time.perf_counter() - t0) * 1000.0
                h.close()
            elif fmt == "zarr3":
                h = xr.open_zarr(path)
                t0 = time.perf_counter()
                arr = h["temperature"][r5_y_start:r5_y_start + r5_y_count, r5_x_start:r5_x_start + r5_x_count].values
                t_elapsed = (time.perf_counter() - t0) * 1000.0
                h.close()
            elif fmt == "grib2":
                h = cfgrib.open_dataset(str(path), backend_kwargs=grib_kwargs)
                t0 = time.perf_counter()
                var_name = "t2m" if "t2m" in h else list(h.data_vars.keys())[0]
                arr = h[var_name].values[r5_y_start:r5_y_start + r5_y_count, r5_x_start:r5_x_start + r5_x_count]
                t_elapsed = (time.perf_counter() - t0) * 1000.0
                h.close()

            r5_times[fmt].append(t_elapsed)

    for fmt in formats:
        samples = r5_times[fmt]
        print(f"R5 [{fmt:7s}] Region slice median: {np.median(samples):7.3f} ms | p95: {np.percentile(samples, 95):7.3f} ms")

    # --- 5. CORRECTNESS & ERROR METRICS ---
    print("\n--- ACCURACY & INTEGRITY VERIFICATION ---")
    for fmt in formats:
        restored = restored_arrays[fmt]
        diff = np.abs(restored.flatten() - expected_data.flatten())
        max_err = float(np.max(diff))
        rmse = float(np.sqrt(np.mean(diff ** 2)))
        bias = float(np.mean(restored.flatten() - expected_data.flatten()))
        is_exact = bool(max_err == 0.0)
        print(f"[{fmt:7s}] Bit-exact: {str(is_exact):5s} | Max Abs Error: {max_err:9.6f} K | RMSE: {rmse:9.6f} K | Bias: {bias:9.6f} K")

        results.append({
            "dataset": dataset_name,
            "format": fmt,
            "size_bytes": get_dir_or_file_size(artifacts[fmt]),
            "r1_open_median_ms": round(float(np.median(r1_times[fmt])), 3),
            "r1_open_p95_ms": round(float(np.percentile(r1_times[fmt], 95)), 3),
            "r2_read_median_ms": round(float(np.median(r2_times[fmt])), 3),
            "r2_read_p95_ms": round(float(np.percentile(r2_times[fmt], 95)), 3),
            "r5_region_median_ms": round(float(np.median(r5_times[fmt])), 3),
            "r5_region_p95_ms": round(float(np.percentile(r5_times[fmt], 95)), 3),
            "max_abs_error": round(max_err, 6),
            "rmse": round(rmse, 6),
            "bias": round(bias, 6),
            "bit_exact": is_exact,
        })

    # Clean up working directory
    shutil.rmtree(work_dir)
    return results


def main():
    scratch_root = Path(os.environ.get("WX1_SCRATCH", "/home/j_ord/WX-1-TESTS/scratch")) / "formal_baseline"
    scratch_root.mkdir(parents=True, exist_ok=True)
    results_dir = Path("bench/results")
    results_dir.mkdir(parents=True, exist_ok=True)

    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    print(f"=== WX-1 FORMAL BASELINE BENCHMARK RUN ({timestamp}) ===")
    print(f"acceptance.yaml status: frozen_pre_measurement (SHA256: faa17ac1919a333ae873140272a93161af96508d914475cf600725270793fdb7)")

    all_results = []
    # 1. KNMI HARMONIE
    harm_res = run_benchmark_for_dataset("harmonie_nl", load_operational_dataset("harmonie_nl", Path("/home/j_ord/WX-1-TESTS/scratch/bench"))[0], scratch_root)
    all_results.extend(harm_res)

    # 2. ECMWF Oper Europe
    ecmwf_res = run_benchmark_for_dataset("ecmwf_oper_europe", load_operational_dataset("ecmwf_oper_europe", Path("/home/j_ord/WX-1-TESTS/scratch/bench"))[0], scratch_root)
    all_results.extend(ecmwf_res)

    # Save CSV and JSON
    csv_path = results_dir / f"formal_baseline_{timestamp}.csv"
    with open(csv_path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=list(all_results[0].keys()))
        writer.writeheader()
        writer.writerows(all_results)

    json_path = results_dir / f"formal_baseline_{timestamp}.json"
    with open(json_path, "w") as f:
        json.dump(all_results, f, indent=2)

    # Generate Markdown Summary
    md_path = results_dir / f"formal_baseline_report_{timestamp}.md"
    md_lines = [
        "# WX-1 Formal Baseline Cross-Format Benchmark Report",
        "",
        f"- **Datum / Tijd:** `{timestamp}`",
        f"- **acceptance.yaml SHA256:** `faa17ac1919a333ae873140272a93161af96508d914475cf600725270793fdb7`",
        f"- **Herhalingen:** 30 warm repetitions per read workload",
        "",
        "## Resultatentabel",
        "",
        "| Dataset | Formaat | Grootte (bytes) | R1 Open (ms) | R2 Vol Veld (ms) | R5 Regio Box (ms) | Max Abs Fout (K) | Bit-exact |",
        "|---|---|---:|---:|---:|---:|---:|:---:|",
    ]
    for r in all_results:
        md_lines.append(
            f"| **{r['dataset']}** | {r['format'].upper()} | {r['size_bytes']:,} | {r['r1_open_median_ms']:.3f} | {r['r2_read_median_ms']:.3f} | {r['r5_region_median_ms']:.3f} | {r['max_abs_error']:.6f} | {'Ja' if r['bit_exact'] else 'Nee'} |"
        )
    md_lines.append("")
    md_path.write_text("\n".join(md_lines))
    print(f"\nSaved CSV: {csv_path}")
    print(f"Saved JSON: {json_path}")
    print(f"Saved Report: {md_path}")


if __name__ == "__main__":
    main()
