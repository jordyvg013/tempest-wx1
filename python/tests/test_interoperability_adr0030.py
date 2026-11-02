"""Comprehensive test suite for ADR 0030: External Format Interoperability & Lossless Metadata Preservation."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

import numpy as np
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import wx1
from wx1.adapters import grib2, netcdf, zarr


EXT4_BENCH_DIR = Path("/home/j_ord/wx1_bench_ext4")


@pytest.fixture
def ext4_grib_sfc():
    path = EXT4_BENCH_DIR / "ecmwf_nh_sfc_2t.grib2"
    if not path.exists():
        pytest.skip(f"Ext4 lean GRIB2 file not found: {path}")
    return path


@pytest.fixture
def ext4_grib_pl():
    path = EXT4_BENCH_DIR / "ecmwf_nh_pl_t_2levels.grib2"
    if not path.exists():
        pytest.skip(f"Ext4 lean GRIB2 file not found: {path}")
    return path


def test_grib2_sfc_roundtrip_lossless_metadata(tmp_path, ext4_grib_sfc):
    """Test GRIB2 2D surface import -> WX-1 -> export to GRIB2 roundtrip with full metadata preservation."""
    pytest.importorskip("eccodes")

    out_wx1 = tmp_path / "sfc_test.wx1"
    out_grib = tmp_path / "sfc_roundtrip.grib2"

    # 1. Import GRIB2 to WX-1
    with wx1.from_grib(ext4_grib_sfc, out_wx1, codec="zstd_shuffle") as f:
        assert f.dataset_id.startswith("ecmwf_nh_sfc_2t")
        assert len(f.variables) == 1
        var = f.variables[0]
        assert var["path"] == "/2t"
        assert var["shape"] == [901, 3600]
        assert var["dimensions"] == ["latitude", "longitude"]

        # Check metadata preservation
        attrs = f.attributes
        assert attrs["wx1:source_format"] == "grib2"
        assert attrs["wx1:source_file"] == ext4_grib_sfc.name
        assert attrs["grib:shortName"] == "2t"
        assert attrs["grib:discipline"] == "0"
        assert attrs["grib:parameterCategory"] == "0"
        assert attrs["grib:parameterNumber"] == "0"
        assert attrs["grib:typeOfLevel"] == "heightAboveGround"
        assert attrs["grib:level"] == "2"
        assert attrs["units"] == "K"
        assert attrs["cf:standard_name"] == "2t"

        wx1_data = f.read_array(var["path"])
        assert wx1_data.shape == (901, 3600)

    # 2. Export WX-1 to GRIB2
    exported_path = wx1.to_grib(out_wx1, out_grib, template_grib=ext4_grib_sfc)
    assert exported_path.exists()

    # 3. Verify exported GRIB2 against original data
    import eccodes
    with open(ext4_grib_sfc, "rb") as f_orig, open(out_grib, "rb") as f_rt:
        gid_orig = eccodes.codes_grib_new_from_file(f_orig)
        gid_rt = eccodes.codes_grib_new_from_file(f_rt)

        assert eccodes.codes_get(gid_rt, "shortName") == "2t"
        assert eccodes.codes_get(gid_rt, "level") == 2
        assert eccodes.codes_get(gid_rt, "typeOfLevel") == "heightAboveGround"

        vals_orig = eccodes.codes_get_values(gid_orig)
        vals_rt = eccodes.codes_get_values(gid_rt)

        diff = np.max(np.abs(vals_orig - vals_rt))
        assert diff == 0.0, f"GRIB2 roundtrip should be bit-exact, got max diff: {diff}"

        eccodes.codes_release(gid_orig)
        eccodes.codes_release(gid_rt)


def test_grib2_multilevel_pl_roundtrip(tmp_path, ext4_grib_pl):
    """Test GRIB2 multi-level isobaric import (3D array) -> WX-1 -> GRIB2."""
    pytest.importorskip("eccodes")

    out_wx1 = tmp_path / "pl_test.wx1"
    out_grib = tmp_path / "pl_roundtrip.grib2"

    # 1. Import multi-level GRIB2
    with wx1.from_grib(ext4_grib_pl, out_wx1, codec="zstd_shuffle") as f:
        assert len(f.variables) == 1
        var = f.variables[0]
        assert var["path"] == "/t"
        assert var["shape"] == [2, 901, 3600]
        assert var["dimensions"] == ["level", "latitude", "longitude"]

        attrs = f.attributes
        assert attrs["grib:shortName"] == "t"
        assert attrs["grib:typeOfLevel"] == "isobaricInhPa"
        assert attrs["grib:levels"] == "500,850"

        data_3d = f.read_array(var["path"])
        assert data_3d.shape == (2, 901, 3600)

    # 2. Export back to GRIB2
    exported_path = wx1.to_grib(out_wx1, out_grib)
    assert exported_path.exists()

    # 3. Read back messages and verify
    import eccodes
    read_messages = []
    with open(out_grib, "rb") as f_rt:
        while True:
            gid = eccodes.codes_grib_new_from_file(f_rt)
            if gid is None:
                break
            lvl = eccodes.codes_get(gid, "level")
            sname = eccodes.codes_get(gid, "shortName")
            vals = eccodes.codes_get_values(gid)
            read_messages.append((lvl, sname, vals))
            eccodes.codes_release(gid)

    assert len(read_messages) == 2
    assert read_messages[0][0] == 500
    assert read_messages[0][1] == "t"
    assert read_messages[1][0] == 850
    assert read_messages[1][1] == "t"


def test_netcdf4_roundtrip_with_fill_and_cf_attrs(tmp_path):
    """Test NetCDF-4 import -> WX-1 -> export to NetCDF-4 with _FillValue and CF conventions."""
    pytest.importorskip("netCDF4")
    import netCDF4

    src_nc = tmp_path / "source.nc"
    out_wx1 = tmp_path / "nc_test.wx1"
    out_nc = tmp_path / "roundtrip.nc"

    # 1. Create synthetic source NetCDF
    with netCDF4.Dataset(src_nc, "w", format="NETCDF4") as ds:
        ds.createDimension("time", 3)
        ds.createDimension("lat", 10)
        ds.createDimension("lon", 20)
        ds.title = "Synthetic NWP Simulation"
        ds.institution = "WX-1 Project"

        v = ds.createVariable("temperature", "f4", ("time", "lat", "lon"), fill_value=-9999.0)
        v.units = "K"
        v.standard_name = "air_temperature"
        v.long_name = "Air Temperature at 2m"

        raw_data = np.linspace(240.0, 310.0, 3 * 10 * 20, dtype=np.float32).reshape(3, 10, 20)
        # Inject fill values
        raw_data[0, 0, 0] = -9999.0
        raw_data[2, 5, 5] = -9999.0
        v[:] = raw_data

    # 2. Convert to WX-1
    with wx1.from_netcdf(src_nc, out_wx1, variable_name="temperature", codec="zstd_shuffle") as f:
        assert len(f.variables) == 1
        var = f.variables[0]
        assert var["path"] == "/temperature"
        assert var["shape"] == [3, 10, 20]
        assert var["dimensions"] == ["time", "lat", "lon"]

        attrs = f.attributes
        assert attrs["wx1:source_format"] == "netcdf4"
        assert attrs["units"] == "K"
        assert attrs["cf:standard_name"] == "air_temperature"
        assert attrs["cf:long_name"] == "Air Temperature at 2m"
        assert attrs["_FillValue"] == "-9999.0"
        assert attrs["netcdf:global:title"] == "Synthetic NWP Simulation"

        wx1_data = f.read_array(var["path"])
        assert wx1_data[0, 0, 0] == -9999.0

    # 3. Export back to NetCDF-4
    exported_nc = wx1.to_netcdf(out_wx1, out_nc)
    assert exported_nc.exists()

    # 4. Verify roundtrip in NetCDF-4
    with netCDF4.Dataset(out_nc, "r") as ds_check:
        assert "temperature" in ds_check.variables
        v_check = ds_check.variables["temperature"]
        assert v_check.units == "K"
        assert v_check.standard_name == "air_temperature"
        assert v_check.long_name == "Air Temperature at 2m"
        assert v_check.shape == (3, 10, 20)
        assert ds_check.title == "Synthetic NWP Simulation"

        rt_data = v_check[:]
        # Unmask and check exact values
        if hasattr(rt_data, "filled"):
            arr_data = rt_data.filled(v_check._FillValue)
        else:
            arr_data = np.asarray(rt_data)

        diff = np.max(np.abs(raw_data - arr_data))
        assert diff == 0.0, f"NetCDF-4 roundtrip should be bit-exact, got max diff: {diff}"


def test_zarr_roundtrip_with_chunking_and_attrs(tmp_path):
    """Test Zarr import -> WX-1 -> export to Zarr with chunks and metadata."""
    pytest.importorskip("zarr")
    import zarr as zr

    src_zarr = tmp_path / "source.zarr"
    out_wx1 = tmp_path / "zarr_test.wx1"
    out_zarr = tmp_path / "roundtrip.zarr"

    # 1. Create source Zarr group
    root = zr.open_group(str(src_zarr), mode="w")
    root.attrs["model_version"] = "v2.1"

    data_orig = np.arange(1000, dtype=np.float32).reshape(10, 10, 10)
    if hasattr(root, "create_array"):
        z_arr = root.create_array("u10", shape=(10, 10, 10), chunks=(5, 5, 5), dtype="float32")
    else:
        z_arr = root.create_dataset("u10", shape=(10, 10, 10), chunks=(5, 5, 5), dtype="float32")

    z_arr[:] = data_orig
    z_arr.attrs["_ARRAY_DIMENSIONS"] = ["time", "lat", "lon"]
    z_arr.attrs["units"] = "m s**-1"
    z_arr.attrs["standard_name"] = "eastward_wind"

    # 2. Convert to WX-1
    with wx1.from_zarr(src_zarr, out_wx1, variable_name="u10", codec="zstd_shuffle") as f:
        assert len(f.variables) == 1
        var = f.variables[0]
        assert var["path"] == "/u10"
        assert var["shape"] == [10, 10, 10]
        assert var["dimensions"] == ["time", "lat", "lon"]

        attrs = f.attributes
        assert attrs["wx1:source_format"] == "zarr"
        assert attrs["units"] == "m s**-1"
        assert attrs["cf:standard_name"] == "eastward_wind"
        assert attrs["zarr:group:model_version"] == "v2.1"

    # 3. Export back to Zarr
    exported_zarr = wx1.to_zarr(out_wx1, out_zarr, chunks=(5, 5, 5))
    assert exported_zarr.exists()

    # 4. Verify roundtrip in Zarr
    root_rt = zr.open_group(str(out_zarr), mode="r")
    assert "u10" in root_rt
    z_rt = root_rt["u10"]
    assert z_rt.shape == (10, 10, 10)
    assert z_rt.chunks == (5, 5, 5)
    assert z_rt.attrs["units"] == "m s**-1"
    assert z_rt.attrs["standard_name"] == "eastward_wind"
    assert root_rt.attrs["model_version"] == "v2.1"

    diff = np.max(np.abs(data_orig - z_rt[:]))
    assert diff == 0.0, f"Zarr roundtrip should be bit-exact, got max diff: {diff}"


def test_cli_convert_roundtrip(tmp_path, ext4_grib_sfc):
    """Test CLI conversion command (python -m wx1 convert) across formats."""
    pytest.importorskip("eccodes")
    pytest.importorskip("netCDF4")

    wx1_file = tmp_path / "cli_converted.wx1"
    nc_file = tmp_path / "cli_converted.nc"

    # 1. GRIB2 -> WX-1 via CLI convert
    cmd1 = [
        sys.executable,
        "-m",
        "wx1",
        "convert",
        str(ext4_grib_sfc),
        str(wx1_file),
        "--codec",
        "zstd_shuffle",
    ]
    env = dict(os.environ, PYTHONPATH=str(Path(__file__).resolve().parents[1]))
    res1 = subprocess.run(cmd1, capture_output=True, text=True, env=env)
    assert res1.returncode == 0, f"CLI GRIB2->WX-1 failed: {res1.stderr}"
    assert wx1_file.exists()

    # 2. WX-1 info command
    cmd_info = [sys.executable, "-m", "wx1", "info", str(wx1_file)]
    res_info = subprocess.run(cmd_info, capture_output=True, text=True, env=env)
    assert res_info.returncode == 0
    assert "Detected format: wx1" in res_info.stdout
    assert "grib:shortName: 2t" in res_info.stdout

    # 3. WX-1 -> NetCDF-4 via CLI convert
    cmd2 = [sys.executable, "-m", "wx1", "convert", str(wx1_file), str(nc_file)]
    res2 = subprocess.run(cmd2, capture_output=True, text=True, env=env)
    assert res2.returncode == 0, f"CLI WX-1->NetCDF failed: {res2.stderr}"
    assert nc_file.exists()


def test_error_handling_and_validation(tmp_path):
    """Test error semantics for missing files, invalid variables, and forbidden configurations."""
    # Non-existent input file
    with pytest.raises(FileNotFoundError):
        wx1.from_grib(tmp_path / "missing.grib2", tmp_path / "out.wx1")

    with pytest.raises(FileNotFoundError):
        wx1.from_netcdf(tmp_path / "missing.nc", tmp_path / "out.wx1")

    with pytest.raises(FileNotFoundError):
        wx1.from_zarr(tmp_path / "missing.zarr", tmp_path / "out.wx1")
