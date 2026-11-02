"""Prepare Zarr 3, NetCDF4, and WX-1 test files directly in the D:\\ dataset directories
and run cross-format benchmarks (lossless and lossy separated).

IMPORTANT: All generated files MUST stay on /mnt/d/Weer/Uitvoer/data/... and NEVER
on the WSL ext4 disk (which lives on the Windows C: drive).
"""

from __future__ import annotations

import json
import os
import shutil
import time
from pathlib import Path
from typing import Any

import eccodes
import netCDF4
import numpy as np
import zarr
import wx1

D_OPER_NH = Path("/mnt/d/Weer/Uitvoer/data/ecmwf/20261029_12z_oper_allvars/crop_nh_allvars")
D_KNMI_CY46 = Path("/mnt/d/Weer/Uitvoer/data/knmi/nl/HARM46_V1_P1_2026102815")
D_ENFO_CROP = Path("/mnt/d/Weer/Uitvoer/data/ecmwf/20261026_12z_enfo/crop")


def get_dir_or_file_size(path: Path) -> int:
    if path.is_file():
        return path.stat().st_size
    if path.is_dir():
        return sum(f.stat().st_size for f in path.rglob("*") if f.is_file())
    return 0


def extract_grib_messages(
    src_grib: Path,
    dst_grib: Path,
    short_name: str,
    level: int | None = None,
    type_of_level: str | None = None,
) -> tuple[np.ndarray, list[str], list[int]]:
    """Extract matching GRIB2 messages into a clean single-variable GRIB2 file on D: and return float32 array."""
    slices = []
    raw_msgs = []
    ni, nj = 0, 0
    if dst_grib.exists():
        dst_grib.unlink()
    with open(src_grib, "rb") as fin:
        while True:
            gid = eccodes.codes_grib_new_from_file(fin)
            if gid is None:
                break
            sn = eccodes.codes_get(gid, "shortName")
            lvl = eccodes.codes_get(gid, "level")
            tol = eccodes.codes_get(gid, "typeOfLevel")
            match = (sn == short_name)
            if level is not None and lvl != level:
                match = False
            if type_of_level is not None and tol != type_of_level:
                match = False
            if match:
                ni = int(eccodes.codes_get(gid, "Ni"))
                nj = int(eccodes.codes_get(gid, "Nj"))
                vals = eccodes.codes_get_values(gid).astype("<f4").reshape(nj, ni)
                sort_key = 0
                try:
                    sort_key = int(eccodes.codes_get(gid, "number"))
                except Exception:
                    sort_key = lvl
                msg_bytes = eccodes.codes_get_message(gid)
                slices.append((sort_key, vals, msg_bytes))
            eccodes.codes_release(gid)

    slices.sort(key=lambda x: x[0])
    with open(dst_grib, "wb") as fout:
        for _, _, msg_bytes in slices:
            fout.write(msg_bytes)

    if len(slices) == 1:
        arr = slices[0][1]
        return arr, ["y", "x"], [min(256, nj), min(256, ni)]
    arr = np.stack([s[1] for s in slices], axis=0)
    return arr, ["z", "y", "x"], [1, min(256, nj), min(256, ni)]


def extract_knmi_cy46_timeseries(
    knmi_dir: Path,
    dst_grib: Path,
    short_name: str = "2t",
    max_steps: int = 12,
) -> tuple[np.ndarray, list[str], list[int]]:
    """Extract multi-timestep KNMI HARMONIE CY46 GRIB2 series into a 3D cube (t, y, x) on D:."""
    files = sorted(p for p in knmi_dir.glob("HA46_N20_*_GB") if not p.name.endswith(".idx"))[:max_steps]
    slices = []
    ni, nj = 390, 390
    if dst_grib.exists():
        dst_grib.unlink()
    with open(dst_grib, "wb") as fout:
        for fpath in files:
            with open(fpath, "rb") as fin:
                while True:
                    gid = eccodes.codes_grib_new_from_file(fin)
                    if gid is None:
                        break
                    sn = eccodes.codes_get(gid, "shortName")
                    if sn == short_name:
                        ni = int(eccodes.codes_get(gid, "Ni"))
                        nj = int(eccodes.codes_get(gid, "Nj"))
                        vals = eccodes.codes_get_values(gid).astype("<f4").reshape(nj, ni)
                        slices.append(vals)
                        eccodes.codes_write(gid, fout)
                        eccodes.codes_release(gid)
                        break
                    eccodes.codes_release(gid)
    arr = np.stack(slices, axis=0)
    return arr, ["time", "y", "x"], [1, 195, 195]


def convert_and_benchmark_case(
    case_id: str,
    out_dir: Path,
    arr: np.ndarray,
    dims: list[str],
    chunks: list[int],
    grib_ref_path: Path,
    eps_bound: float = 0.01,
    reps: int = 10,
) -> list[dict[str, Any]]:
    """Convert array to NetCDF4 (uncompressed + zlib), Zarr 3, and WX-1 (restart + lossy) on D: and benchmark."""
    assert str(out_dir).startswith("/mnt/d/"), f"Forbidden non-D: path: {out_dir}"
    out_dir.mkdir(parents=True, exist_ok=True)

    nc_uncomp_path = out_dir / f"{case_id}_uncompressed.nc"
    nc_zlib_path = out_dir / f"{case_id}_zlib.nc"
    zarr_path = out_dir / f"{case_id}.zarr"
    zarr_shuffle_path = out_dir / f"{case_id}_shuffle.zarr"
    wx1_restart_path = out_dir / f"{case_id}_restart_lossless.wx1"
    wx1_lossy_path = out_dir / f"{case_id}_output_lossy.wx1"

    for p in [nc_uncomp_path, nc_zlib_path, wx1_restart_path, wx1_lossy_path]:
        if p.exists():
            p.unlink()
    for zp in [zarr_path, zarr_shuffle_path]:
        if zp.exists():
            shutil.rmtree(zp)

    # Inspect GRIB2 native quantization step (binaryScaleFactor)
    with open(grib_ref_path, "rb") as fin:
        gid = eccodes.codes_grib_new_from_file(fin)
        bsf = int(eccodes.codes_get(gid, "binaryScaleFactor"))
        bpv = int(eccodes.codes_get(gid, "bitsPerValue"))
        packing_type = str(eccodes.codes_get(gid, "packingType"))
        eccodes.codes_release(gid)
    grib_step = float(2.0**bsf) if bpv > 0 else 0.0
    grib_intrinsic_eps = grib_step * 0.5

    # 1. Write NetCDF4 Uncompressed (contiguous=True so HDF5 does not pad edge chunks to 228%)
    t0 = time.perf_counter()
    with netCDF4.Dataset(nc_uncomp_path, "w", format="NETCDF4") as ds:
        for d, s in zip(dims, arr.shape):
            ds.createDimension(d, s)
        v = ds.createVariable("field", "f4", tuple(dims), contiguous=True)
        v[:] = arr
    w_nc_uncomp = (time.perf_counter() - t0) * 1000.0

    # 2. Write NetCDF4 Compressed (zlib complevel=4, shuffle=True)
    t0 = time.perf_counter()
    with netCDF4.Dataset(nc_zlib_path, "w", format="NETCDF4") as ds:
        for d, s in zip(dims, arr.shape):
            ds.createDimension(d, s)
        v = ds.createVariable(
            "field", "f4", tuple(dims), chunksizes=tuple(chunks), zlib=True, complevel=4, shuffle=True
        )
        v[:] = arr
    w_nc_zlib = (time.perf_counter() - t0) * 1000.0

    # 3a. Write Zarr 3 (Zstd level 3 default, no shuffle)
    t0 = time.perf_counter()
    z = zarr.create_array(
        store=str(zarr_path),
        shape=arr.shape,
        chunks=tuple(chunks),
        dtype="<f4",
        overwrite=True,
    )
    z[:] = arr
    w_zarr = (time.perf_counter() - t0) * 1000.0

    # 3b. Write Zarr 3 with Blosc (Zstd level 3 + ByteShuffle)
    t0 = time.perf_counter()
    z_shuf = zarr.create_array(
        store=str(zarr_shuffle_path),
        shape=arr.shape,
        chunks=tuple(chunks),
        dtype="<f4",
        compressors=zarr.codecs.BloscCodec(
            cname="zstd", clevel=3, shuffle="shuffle"
        ),
        overwrite=True,
    )
    z_shuf[:] = arr
    w_zarr_shuffle = (time.perf_counter() - t0) * 1000.0

    # 4. Write WX-1 Restart (Lossless, ByteShuffle + Zstd)
    roles = ["other"] * len(dims)
    if len(dims) == 2:
        roles = ["latitude", "longitude"]
    elif len(dims) == 3:
        roles = ["time" if dims[0] == "time" else ("ensemble" if dims[0] == "z" else "vertical"), "latitude", "longitude"]

    t0 = time.perf_counter()
    wx1.write_array(
        wx1_restart_path,
        arr,
        dataset_id=f"{case_id}_restart",
        variable_path="/field",
        dimensions=dims,
        chunk_shape=chunks,
        dimension_roles=roles,
        profile="restart",
        codec="zstd_shuffle",
    )
    w_wx1_restart = (time.perf_counter() - t0) * 1000.0

    # 5. Write WX-1 Output (Lossy eps_bound, ByteShuffle + Zstd)
    t0 = time.perf_counter()
    wx1.write_array(
        wx1_lossy_path,
        arr,
        dataset_id=f"{case_id}_output",
        variable_path="/field",
        dimensions=dims,
        chunk_shape=chunks,
        dimension_roles=roles,
        profile="output",
        lossy_error_bound=eps_bound,
        codec="zstd_shuffle",
    )
    w_wx1_lossy = (time.perf_counter() - t0) * 1000.0

    # Region selection (100x100 box in last two axes, index 0 in leading axis if 3D)
    y_len, x_len = arr.shape[-2], arr.shape[-1]
    ry0, ryc = y_len // 4, min(100, y_len // 2)
    rx0, rxc = x_len // 4, min(100, x_len // 2)

    records = []

    def bench_reader(fmt_name: str, category: str, path: Path, write_ms: float, open_fn, full_fn, box_fn, extra_meta: dict | None = None):
        open_t, full_t, box_t = [], [], []
        restored = None
        for _ in range(reps):
            t0 = time.perf_counter()
            handle = open_fn()
            open_t.append((time.perf_counter() - t0) * 1000.0)

            # Measure R5 (100x100 region slice) FIRST on the freshly opened handle,
            # before R2 (full array read) can populate any in-memory HDF5 chunk cache!
            t0 = time.perf_counter()
            _ = box_fn(handle)
            box_t.append((time.perf_counter() - t0) * 1000.0)

            t0 = time.perf_counter()
            restored = full_fn(handle)
            full_t.append((time.perf_counter() - t0) * 1000.0)

            if hasattr(handle, "close"):
                handle.close()

        diff = restored - arr
        max_abs = float(np.max(np.abs(diff)))
        rmse = float(np.sqrt(np.mean(diff**2)))
        bias = float(np.mean(diff))
        bit_exact = bool(np.array_equal(restored.view("u4"), arr.view("u4")))
        fsize = get_dir_or_file_size(path)
        rec = {
            "dataset": case_id,
            "shape": list(arr.shape),
            "raw_bytes": int(arr.nbytes),
            "reps": reps,
            "category": category,
            "format": fmt_name,
            "path_on_d": str(path),
            "size_bytes": fsize,
            "pct_of_raw": round(fsize / arr.nbytes * 100.0, 2),
            "write_ms": round(write_ms, 2),
            "r1_open_med_ms": round(float(np.median(open_t)), 3),
            "r1_open_p95_ms": round(float(np.percentile(open_t, 95)), 3),
            "r2_full_med_ms": round(float(np.median(full_t)), 3),
            "r2_full_p95_ms": round(float(np.percentile(full_t, 95)), 3),
            "r5_box_med_ms": round(float(np.median(box_t)), 3),
            "r5_box_p95_ms": round(float(np.percentile(box_t, 95)), 3),
            "bit_exact": bit_exact,
            "max_abs_error": round(max_abs, 6),
            "rmse": round(rmse, 6),
            "bias": round(bias, 6),
        }
        if extra_meta:
            rec.update(extra_meta)
        records.append(rec)
        print(
            f"  [{category:8s} | {fmt_name:22s}] size={fsize:10,d} B ({rec['pct_of_raw']:6.2f}%) | "
            f"open={rec['r1_open_med_ms']:6.3f} (p95 {rec['r1_open_p95_ms']:6.3f}) ms | "
            f"full={rec['r2_full_med_ms']:7.2f} (p95 {rec['r2_full_p95_ms']:7.2f}) ms | "
            f"box={rec['r5_box_med_ms']:6.3f} (p95 {rec['r5_box_p95_ms']:6.3f}) ms | "
            f"exact={str(bit_exact):5s} | max_err={max_abs:.6f}"
        )

    is_3d = (arr.ndim == 3)

    # Benchmark NetCDF4 Uncompressed (contiguous)
    bench_reader(
        "NETCDF4_UNCOMP",
        "lossless",
        nc_uncomp_path,
        w_nc_uncomp,
        lambda: netCDF4.Dataset(nc_uncomp_path, "r"),
        lambda ds: np.asarray(ds.variables["field"][:], dtype="<f4"),
        lambda ds: np.asarray(
            ds.variables["field"][0, ry0:ry0 + ryc, rx0:rx0 + rxc]
            if is_3d
            else ds.variables["field"][ry0:ry0 + ryc, rx0:rx0 + rxc],
            dtype="<f4",
        ),
    )

    # Benchmark NetCDF4 Zlib
    bench_reader(
        "NETCDF4_ZLIB4",
        "lossless",
        nc_zlib_path,
        w_nc_zlib,
        lambda: netCDF4.Dataset(nc_zlib_path, "r"),
        lambda ds: np.asarray(ds.variables["field"][:], dtype="<f4"),
        lambda ds: np.asarray(
            ds.variables["field"][0, ry0:ry0 + ryc, rx0:rx0 + rxc]
            if is_3d
            else ds.variables["field"][ry0:ry0 + ryc, rx0:rx0 + rxc],
            dtype="<f4",
        ),
    )

    # Benchmark Zarr 3 (Zstd no shuffle)
    bench_reader(
        "ZARR3_ZSTD",
        "lossless",
        zarr_path,
        w_zarr,
        lambda: zarr.open_array(store=str(zarr_path), mode="r"),
        lambda z_arr: np.asarray(z_arr[:], dtype="<f4"),
        lambda z_arr: np.asarray(
            z_arr[0, ry0:ry0 + ryc, rx0:rx0 + rxc] if is_3d else z_arr[ry0:ry0 + ryc, rx0:rx0 + rxc],
            dtype="<f4",
        ),
    )

    # Benchmark Zarr 3 (Blosc Zstd + Shuffle)
    bench_reader(
        "ZARR3_BLOSC_SHUFFLE",
        "lossless",
        zarr_shuffle_path,
        w_zarr_shuffle,
        lambda: zarr.open_array(store=str(zarr_shuffle_path), mode="r"),
        lambda z_arr: np.asarray(z_arr[:], dtype="<f4"),
        lambda z_arr: np.asarray(
            z_arr[0, ry0:ry0 + ryc, rx0:rx0 + rxc] if is_3d else z_arr[ry0:ry0 + ryc, rx0:rx0 + rxc],
            dtype="<f4",
        ),
    )

    # Benchmark WX-1 Restart (Lossless)
    bench_reader(
        "WX1_RESTART_SHUFFLE",
        "lossless",
        wx1_restart_path,
        w_wx1_restart,
        lambda: wx1.open_file(wx1_restart_path),
        lambda wf: wf.read_array("/field"),
        lambda wf: wf.read_array_slice(
            "/field",
            [0, ry0, rx0] if is_3d else [ry0, rx0],
            [1, ryc, rxc] if is_3d else [ryc, rxc],
        ),
    )

    # Benchmark GRIB2 reference (decode via ecCodes inside timed loop)
    def read_grib_all(_):
        out = []
        with open(grib_ref_path, "rb") as fin:
            while True:
                gid = eccodes.codes_grib_new_from_file(fin)
                if gid is None:
                    break
                vals = eccodes.codes_get_values(gid).astype("<f4").reshape(y_len, x_len)
                out.append(vals)
                eccodes.codes_release(gid)
        return out[0] if len(out) == 1 else np.stack(out, axis=0)

    def read_grib_first_box(_):
        with open(grib_ref_path, "rb") as fin:
            gid = eccodes.codes_grib_new_from_file(fin)
            vals = eccodes.codes_get_values(gid).astype("<f4").reshape(y_len, x_len)
            eccodes.codes_release(gid)
            return vals[ry0:ry0 + ryc, rx0:rx0 + rxc]

    def open_grib():
        f = open(grib_ref_path, "rb")
        gid = eccodes.codes_grib_new_from_file(f)
        eccodes.codes_release(gid)
        f.close()
        return f

    bench_reader(
        "GRIB2_NATIVE",
        "lossy_or_packed",
        grib_ref_path,
        0.0,
        open_grib,
        read_grib_all,
        read_grib_first_box,
        extra_meta={
            "grib_packing": packing_type,
            "grib_bits_per_value": bpv,
            "grib_step_k": grib_step,
            "grib_intrinsic_eps_k": grib_intrinsic_eps,
        },
    )

    # Benchmark WX-1 Output (Lossy with guaranteed bound eps=0.01)
    bench_reader(
        f"WX1_OUTPUT_EPS_{eps_bound}",
        "lossy_or_packed",
        wx1_lossy_path,
        w_wx1_lossy,
        lambda: wx1.open_file(wx1_lossy_path),
        lambda wf: wf.read_array("/field"),
        lambda wf: wf.read_array_slice(
            "/field",
            [0, ry0, rx0] if is_3d else [ry0, rx0],
            [1, ryc, rxc] if is_3d else [ryc, rxc],
        ),
        extra_meta={"declared_eps_k": eps_bound},
    )

    return records


def main():
    all_records = []

    # 1. ECMWF 0.1 Oper Allvars NH - Surface (2t, 901x3600)
    print("\n=== 1. ECMWF 0.1 NH Oper Surface (2t, 901x3600) ===")
    nh_sfc_dir = D_OPER_NH / "sfc" / "test_formats"
    nh_sfc_dir.mkdir(parents=True, exist_ok=True)
    nh_sfc_grib = nh_sfc_dir / "ecmwf_nh_sfc_2t.grib2"
    arr, dims, chunks = extract_grib_messages(
        D_OPER_NH / "sfc" / "20261029120000-0h-oper-fc-sfc.grib2",
        nh_sfc_grib,
        short_name="2t",
    )
    all_records.extend(convert_and_benchmark_case("ecmwf_nh_sfc_2t", nh_sfc_dir, arr, dims, chunks, nh_sfc_grib))

    # 2. ECMWF 0.1 Oper Allvars NH - Pressure Levels (t across 14 levels, 14x901x3600 = 181.6 MB raw!)
    print("\n=== 2. ECMWF 0.1 NH Oper Pressure Levels (t, 14x901x3600) ===")
    nh_pl_dir = D_OPER_NH / "pl" / "test_formats"
    nh_pl_dir.mkdir(parents=True, exist_ok=True)
    nh_pl_grib = nh_pl_dir / "ecmwf_nh_pl_t_14levels.grib2"
    arr, dims, chunks = extract_grib_messages(
        D_OPER_NH / "pl" / "20261029120000-0h-oper-fc-pl.grib2",
        nh_pl_grib,
        short_name="t",
    )
    all_records.extend(convert_and_benchmark_case("ecmwf_nh_pl_t14", nh_pl_dir, arr, dims, chunks, nh_pl_grib, reps=5))

    # 3. KNMI HARMONIE CY46 NL (2t across 12 forecast timesteps, 12x390x390)
    print("\n=== 3. KNMI HARMONIE CY46 NL (2t, 12x390x390) ===")
    knmi_dir = D_KNMI_CY46 / "test_formats"
    knmi_dir.mkdir(parents=True, exist_ok=True)
    knmi_grib = knmi_dir / "knmi_harm46_2t_12steps.grib2"
    arr, dims, chunks = extract_knmi_cy46_timeseries(D_KNMI_CY46, knmi_grib, short_name="2t", max_steps=12)
    all_records.extend(convert_and_benchmark_case("knmi_harm46_2t_12steps", knmi_dir, arr, dims, chunks, knmi_grib))

    # 4. ECMWF 0.1 ENFO Crop - Surface (2t across 50 ensemble members, 50x261x661)
    print("\n=== 4. ECMWF 0.1 ENFO Crop Surface (2t, 50x261x661) ===")
    enfo_sfc_dir = D_ENFO_CROP / "sfc" / "test_formats"
    enfo_sfc_dir.mkdir(parents=True, exist_ok=True)
    enfo_sfc_grib = enfo_sfc_dir / "ecmwf_enfo_sfc_2t_50members.grib2"
    arr, dims, chunks = extract_grib_messages(
        D_ENFO_CROP / "sfc" / "20261026120000-0h-enfo-ef-sfc.grib2",
        enfo_sfc_grib,
        short_name="2t",
    )
    all_records.extend(convert_and_benchmark_case("ecmwf_enfo_sfc_2t", enfo_sfc_dir, arr, dims, chunks, enfo_sfc_grib))

    # 5. ECMWF 0.1 ENFO Crop - Pressure Levels (t@500 across 50 ensemble members, 50x261x661)
    print("\n=== 5. ECMWF 0.1 ENFO Crop Pressure Levels (t@500, 50x261x661) ===")
    enfo_pl_dir = D_ENFO_CROP / "pl" / "test_formats"
    enfo_pl_dir.mkdir(parents=True, exist_ok=True)
    enfo_pl_grib = enfo_pl_dir / "ecmwf_enfo_pl_t500_50members.grib2"
    arr, dims, chunks = extract_grib_messages(
        D_ENFO_CROP / "pl" / "20261026120000-0h-enfo-ef-pl.grib2",
        enfo_pl_grib,
        short_name="t",
        level=500,
    )
    all_records.extend(convert_and_benchmark_case("ecmwf_enfo_pl_t500", enfo_pl_dir, arr, dims, chunks, enfo_pl_grib))

    out_json = Path("bench/results/cross_format_real_5datasets_v2.json")
    out_json.write_text(json.dumps(all_records, indent=2))
    # Also write d_drive_all_datasets_benchmark.json for continuity
    Path("bench/results/d_drive_all_datasets_benchmark.json").write_text(json.dumps(all_records, indent=2))
    print(f"\nSaved full benchmark results to {out_json}")


if __name__ == "__main__":
    main()
