"""Unit tests for Phase 2 step 1 benchmark adapters."""

from __future__ import annotations

import numpy as np
import pytest

from bench.datasets.synthetic import make_synthetic_dataset
from bench.adapters.wx1 import WX1Adapter
from bench.adapters.grib2 import GRIB2Adapter
from bench.adapters.netcdf4 import NetCDF4Adapter
from bench.adapters.zarr3 import Zarr3Adapter


@pytest.fixture
def synthetic_ds():
    return make_synthetic_dataset()


def test_wx1_adapter_roundtrip_is_bit_exact(synthetic_ds, tmp_path):
    adapter = WX1Adapter()
    art_path = tmp_path / "test.wx1"
    adapter.write(synthetic_ds, art_path)
    assert art_path.exists()

    adapter.open(art_path)
    restored = adapter.read_field("temperature")
    adapter.close()

    expected = synthetic_ds["temperature"].values
    np.testing.assert_array_equal(restored, expected)


def test_netcdf4_adapter_roundtrip_is_bit_exact(synthetic_ds, tmp_path):
    adapter = NetCDF4Adapter()
    art_path = tmp_path / "test.nc"
    adapter.write(synthetic_ds, art_path)
    assert art_path.exists()

    adapter.open(art_path)
    restored = adapter.read_field("temperature")
    adapter.close()

    expected = synthetic_ds["temperature"].values
    np.testing.assert_array_equal(restored, expected)


def test_zarr3_adapter_roundtrip_is_bit_exact(synthetic_ds, tmp_path):
    adapter = Zarr3Adapter()
    art_path = tmp_path / "test.zarr"
    adapter.write(synthetic_ds, art_path)
    assert art_path.exists()

    adapter.open(art_path)
    restored = adapter.read_field("temperature")
    adapter.close()

    expected = synthetic_ds["temperature"].values
    np.testing.assert_array_equal(restored, expected)


def test_grib2_adapter_roundtrip_within_error_bound(synthetic_ds, tmp_path):
    adapter = GRIB2Adapter()
    art_path = tmp_path / "test.grib2"
    adapter.write(synthetic_ds, art_path)
    assert art_path.exists()

    adapter.open(art_path)
    restored = adapter.read_field("temperature")
    adapter.close()

    expected = synthetic_ds["temperature"].values
    max_err = float(np.max(np.abs(restored - expected)))
    assert max_err <= 0.001
