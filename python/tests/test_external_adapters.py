from __future__ import annotations

import sys
from pathlib import Path

import numpy as np
import pytest
import xarray as xr

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from wx1.adapters import grib2, netcdf, zarr


def test_netcdf_local_scipy_fixture_roundtrip(tmp_path):
    pytest.importorskip("scipy")
    source = xr.Dataset(
        {"temperature": (("y", "x"), np.arange(12, dtype=np.float32).reshape(3, 4))},
        attrs={"institution": "WX-1 local fixture"},
    )
    source["temperature"].attrs.update({"units": "K", "standard_name": "air_temperature"})
    path = tmp_path / "fixture.nc"
    source.to_netcdf(path, engine="scipy")

    with netcdf.open_dataset(path, engine="scipy") as actual:
        np.testing.assert_array_equal(actual["temperature"].values, source["temperature"].values)
        assert actual.attrs["institution"] == "WX-1 local fixture"
        assert actual["temperature"].attrs["units"] == "K"


def test_zarr_adapter_reports_missing_optional_dependency(tmp_path):
    if __import__("importlib").util.find_spec("zarr"):
        pytest.skip("Zarr is installed; live local Zarr fixture test is required instead")
    with pytest.raises(ImportError, match="requires the 'zarr' extra"):
        zarr.open_dataset(tmp_path / "missing.zarr")


def test_grib2_adapter_reports_missing_optional_dependency(tmp_path):
    if __import__("importlib").util.find_spec("cfgrib"):
        pytest.skip("cfgrib is installed; live local GRIB2 fixture test is required instead")
    with pytest.raises(ImportError, match="requires the 'grib2' extra"):
        grib2.open_dataset(tmp_path / "missing.grib")


def test_netcdf4_local_fixture_when_engine_is_installed(tmp_path):
    pytest.importorskip("netCDF4")
    source = xr.Dataset(
        {"pressure": (("y", "x"), np.full((2, 3), 100000.0))},
        coords={"y": np.array([51.0, 52.0]), "x": np.array([4.0, 5.0, 6.0])},
        attrs={"institution": "WX-1 NetCDF4 local fixture"},
    )
    source["pressure"].attrs.update({"units": "Pa", "standard_name": "air_pressure"})
    path = tmp_path / "fixture-netcdf4.nc"
    source.to_netcdf(path, engine="netcdf4")
    with netcdf.open_dataset(path, engine="netcdf4") as actual:
        np.testing.assert_array_equal(actual["pressure"].values, source["pressure"].values)
        np.testing.assert_array_equal(actual["y"].values, source["y"].values)
        np.testing.assert_array_equal(actual["x"].values, source["x"].values)
        assert actual.attrs["institution"] == "WX-1 NetCDF4 local fixture"
        assert actual["pressure"].attrs["units"] == "Pa"
        assert actual["pressure"].attrs["standard_name"] == "air_pressure"


def test_zarr_local_roundtrip_when_dependency_is_installed(tmp_path):
    pytest.importorskip("zarr")
    source = xr.Dataset(
        {"wind": (("y", "x"), np.arange(6, dtype=np.float32).reshape(2, 3))},
        coords={"y": np.array([51.0, 52.0]), "x": np.array([4.0, 5.0, 6.0])},
        attrs={"institution": "WX-1 Zarr local fixture"},
    )
    source["wind"].attrs.update({"units": "m s-1", "standard_name": "wind_speed"})
    path = tmp_path / "fixture.zarr"
    try:
        source.to_zarr(path, mode="w", consolidated=False, zarr_format=2)
    except TypeError:  # xarray versions before the zarr_format keyword
        source.to_zarr(path, mode="w", consolidated=False, zarr_version=2)
    with zarr.open_dataset(path, chunks=None, consolidated=False) as actual:
        np.testing.assert_array_equal(actual["wind"].values, source["wind"].values)
        np.testing.assert_array_equal(actual["y"].values, source["y"].values)
        np.testing.assert_array_equal(actual["x"].values, source["x"].values)
        assert actual.attrs["institution"] == "WX-1 Zarr local fixture"
        assert actual["wind"].attrs["units"] == "m s-1"
        assert actual["wind"].attrs["standard_name"] == "wind_speed"


def test_grib2_local_fixture_when_path_is_supplied(monkeypatch):
    import os

    fixture = os.environ.get("WX1_GRIB2_FIXTURE")
    if not fixture:
        pytest.skip("WX1_GRIB2_FIXTURE is not set; no local GRIB2 fixture supplied")
    pytest.importorskip("cfgrib")
    # Multi-level operational files need filter_by_keys or errors='ignore'
    try:
        with grib2.open_dataset(fixture) as actual:
            assert len(actual.data_vars) > 0
    except Exception:
        with grib2.open_dataset(fixture, filter_by_keys={"typeOfLevel": "heightAboveGround"}) as actual:
            assert len(actual.data_vars) > 0
