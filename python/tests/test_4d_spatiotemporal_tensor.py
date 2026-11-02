"""Test suite for 4D spatio-temporal atmospheric tensor storage, slicing, and chunking."""

from __future__ import annotations

import sys
from pathlib import Path
import numpy as np
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import wx1


def test_4d_tensor_write_read_and_slicing(tmp_path):
    """Verify 4D [time, level, lat, lon] tensor creation and multi-axis slicing."""
    out_file = tmp_path / "tensor_4d.wx1"

    # Shape: 4 times, 3 levels, 8 lats, 16 lons
    data = np.arange(4 * 3 * 8 * 16, dtype=np.float32).reshape(4, 3, 8, 16)
    levels = [850, 500, 250]

    wx1.write_array(
        out_file,
        data,
        dataset_id="test_4d",
        variable_path="/t",
        dimensions=["time", "level", "latitude", "longitude"],
        dimension_roles=["time", "vertical", "latitude", "longitude"],
        chunk_shape=[1, 1, 8, 16],
        codec="zstd_shuffle",
        profile="restart",
        attributes={"units": "K", "grib:levels": "850,500,250"},
    )

    with wx1.open_file(out_file) as f:
        assert len(f.variables) == 1
        v = f.variables[0]
        assert v["path"] == "/t"
        assert v["shape"] == [4, 3, 8, 16]
        assert v["dimensions"] == ["time", "level", "latitude", "longitude"]
        assert v["chunk_shape"] == [1, 1, 8, 16]
        assert f.attributes["grib:levels"] == "850,500,250"

        # 1. Full read
        full_read = f.read_array("/t")
        np.testing.assert_array_equal(full_read, data)

        # 2. Read single 2D slice at time 2, level 1 (500 hPa)
        sl_2d = f.read_array_slice("/t", start=[2, 1, 0, 0], count=[1, 1, 8, 16])
        assert sl_2d.shape == (1, 1, 8, 16)
        np.testing.assert_array_equal(sl_2d[0, 0], data[2, 1])

        # 3. Read full 3D column (all 3 levels) at time 0
        col_3d = f.read_array_slice("/t", start=[0, 0, 0, 0], count=[1, 3, 8, 16])
        assert col_3d.shape == (1, 3, 8, 16)
        np.testing.assert_array_equal(col_3d[0], data[0])

        # 4. Read single-point sounding profile across all levels at time 1
        pt_sounding = f.read_array_slice("/t", start=[1, 0, 4, 7], count=[1, 3, 1, 1])
        assert pt_sounding.shape == (1, 3, 1, 1)
        expected_profile = data[1, :, 4, 7]
        np.testing.assert_array_equal(pt_sounding[0, :, 0, 0], expected_profile)


def test_4d_spatially_tiled_chunking(tmp_path):
    """Verify spatially tiled chunking [T, Y, X] for fast station time-series."""
    out_file = tmp_path / "tensor_tiled.wx1"

    # Shape: 10 times, 16 lats, 16 lons
    data = np.linspace(250.0, 310.0, 10 * 16 * 16, dtype=np.float32).reshape(10, 16, 16)

    # Spatially tiled chunk shape: [10, 4, 4]
    wx1.write_array(
        out_file,
        data,
        dataset_id="test_tiled",
        variable_path="/t2m",
        dimensions=["time", "latitude", "longitude"],
        chunk_shape=[10, 4, 4],
        codec="zstd_shuffle",
        profile="restart",
    )

    with wx1.open_file(out_file) as f:
        # Station time-series at point (lat=5, lon=7) across all 10 timesteps
        ts = f.read_array_slice("/t2m", start=[0, 5, 7], count=[10, 1, 1])
        assert ts.shape == (10, 1, 1)
        np.testing.assert_array_equal(ts[:, 0, 0], data[:, 5, 7])


def test_4d_error_bounded_quantization(tmp_path):
    """Verify declared error-bounded quantization on 4D output tensors."""
    out_file = tmp_path / "tensor_lossy.wx1"

    rng = np.random.default_rng(42)
    data = rng.uniform(-25.0, 25.0, size=(2, 2, 8, 8)).astype(np.float32)
    bound = 0.05

    wx1.write_array(
        out_file,
        data,
        dataset_id="test_lossy_4d",
        variable_path="/wind_u",
        dimensions=["time", "level", "lat", "lon"],
        chunk_shape=[1, 1, 8, 8],
        codec="zstd_shuffle",
        profile="output",
        lossy_error_bound=bound,
    )

    with wx1.open_file(out_file) as f:
        read_back = f.read_array("/wind_u")
        max_diff = float(np.max(np.abs(data - read_back)))
        assert max_diff <= bound, f"Max diff {max_diff} exceeded bound {bound}"
