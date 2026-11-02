#!/usr/bin/env python3
"""Extract lean test datasets (temperature, u, v on sfc and 2 pressure levels) directly onto ext4."""

from __future__ import annotations
from pathlib import Path
import eccodes
import numpy as np
import sys

# Ensure wx1 is importable
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "python"))
import wx1

EXT4_DIR = Path("/home/j_ord/wx1_bench_ext4")
EXT4_DIR.mkdir(parents=True, exist_ok=True)

SFC_SRC = Path("/mnt/d/Weer/Uitvoer/data/ecmwf/20261029_12z_oper_allvars/crop_nh_allvars/sfc/20261029120000-0h-oper-fc-sfc.grib2")
PL_SRC = Path("/mnt/d/Weer/Uitvoer/data/ecmwf/20261029_12z_oper_allvars/crop_nh_allvars/pl/20261029120000-0h-oper-fc-pl.grib2")

print(f"Target ext4 directory: {EXT4_DIR}")

# 1. Surface: 2t, 10u, 10v
sfc_grib_out = EXT4_DIR / "ecmwf_nh_sfc_lean.grib2"
sfc_arrays = {}

print("Extracting SFC 2t, 10u, 10v...")
with open(SFC_SRC, "rb") as fin, open(sfc_grib_out, "wb") as fout:
    while True:
        gid = eccodes.codes_grib_new_from_file(fin)
        if gid is None: break
        name = eccodes.codes_get(gid, "shortName")
        if name in ["2t", "10u", "10v"]:
            eccodes.codes_write(gid, fout)
            vals = eccodes.codes_get_values(gid).astype(np.float32).reshape(901, 3600)
            sfc_arrays[name] = vals
            print(f"  Extracted SFC {name}: shape={vals.shape}, min={vals.min():.2f}, max={vals.max():.2f}")
        eccodes.codes_release(gid)

# Also create individual ecmwf_nh_sfc_2t.grib2 for integer packing test
sfc_2t_grib = EXT4_DIR / "ecmwf_nh_sfc_2t.grib2"
with open(sfc_grib_out, "rb") as fin, open(sfc_2t_grib, "wb") as fout:
    while True:
        gid = eccodes.codes_grib_new_from_file(fin)
        if gid is None: break
        if eccodes.codes_get(gid, "shortName") == "2t":
            eccodes.codes_write(gid, fout)
        eccodes.codes_release(gid)

# Save SFC as WX-1
sfc_wx1_out = EXT4_DIR / "ecmwf_nh_sfc_2t_restart_lossless.wx1"
wx1.write_array(
    sfc_wx1_out,
    sfc_arrays["2t"],
    dataset_id="ecmwf_nh_sfc_2t",
    variable_path="/field",
    dimensions=["y", "x"],
    chunk_shape=[256, 256],
    dimension_roles=["latitude", "longitude"],
)
print(f"Saved {sfc_wx1_out} ({sfc_wx1_out.stat().st_size:,d} bytes)")

# 2. Pressure Levels: t, u, v @ 500 hPa and 850 hPa (2 levels)
pl_grib_out = EXT4_DIR / "ecmwf_nh_pl_lean.grib2"
pl_t2_grib = EXT4_DIR / "ecmwf_nh_pl_t_2levels.grib2"
pl_arrays = {"t": {}, "u": {}, "v": {}}

print("\nExtracting PL t, u, v @ 500 & 850 hPa...")
with open(PL_SRC, "rb") as fin, open(pl_grib_out, "wb") as fout, open(pl_t2_grib, "wb") as ft2:
    while True:
        gid = eccodes.codes_grib_new_from_file(fin)
        if gid is None: break
        name = eccodes.codes_get(gid, "shortName")
        lvl = eccodes.codes_get(gid, "level")
        if name in ["t", "u", "v"] and lvl in [500, 850]:
            eccodes.codes_write(gid, fout)
            if name == "t":
                eccodes.codes_write(gid, ft2)
            vals = eccodes.codes_get_values(gid).astype(np.float32).reshape(901, 3600)
            pl_arrays[name][lvl] = vals
            print(f"  Extracted PL {name} @ {lvl} hPa: shape={vals.shape}, min={vals.min():.2f}, max={vals.max():.2f}")
        eccodes.codes_release(gid)

# Stack t @ [500, 850] hPa into (2, 901, 3600)
t_2levels = np.stack([pl_arrays["t"][500], pl_arrays["t"][850]], axis=0)
pl_wx1_out = EXT4_DIR / "ecmwf_nh_pl_t14_restart_lossless.wx1"  # Target path for ablation & reads
wx1.write_array(
    pl_wx1_out,
    t_2levels,
    dataset_id="ecmwf_nh_pl_t_2levels",
    variable_path="/field",
    dimensions=["z", "y", "x"],
    chunk_shape=[1, 256, 256],
    dimension_roles=["vertical", "latitude", "longitude"],
)
print(f"Saved {pl_wx1_out} (shape {t_2levels.shape}, {pl_wx1_out.stat().st_size:,d} bytes)")

print("\nLean datasets preparation on ext4 completed successfully!")
