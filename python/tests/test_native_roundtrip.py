from __future__ import annotations

from pathlib import Path
import sys

import numpy as np
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import wx1


pytestmark = pytest.mark.skipif(wx1._core is None, reason="WX-1 PyO3 extension has not been built")


def test_native_numpy_roundtrip_chunked_f32_is_bit_exact(tmp_path):
    source = (np.arange(24, dtype="<f4").reshape(4, 6) / np.float32(7.0)).astype("<f4")
    path = tmp_path / "numpy-roundtrip.wx1"

    wx1.write_array(
        path,
        source,
        dataset_id="numpy-roundtrip",
        variable_path="/temperature",
        dimensions=["y", "x"],
        chunk_shape=[2, 3],
        dimension_roles=["latitude", "longitude"],
        attributes={"units": "K", "standard_name": "air_temperature"},
    )

    with wx1.open_file(path) as file:
        assert file.dataset_id == "numpy-roundtrip"
        restored = file.read_array("/temperature")
        np.testing.assert_array_equal(restored.view("u4"), source.view("u4"))
        assert restored.dtype == np.dtype("<f4")
        assert file.variables[0]["attributes"]["units"] == "K"

        selection = file.read_array_slice("/temperature", [1, 2], [2, 3])
        np.testing.assert_array_equal(selection, source[1:3, 2:5])


def test_native_numpy_roundtrip_preserves_big_endian_payload(tmp_path):
    source = np.array([[1.25, -2.5], [3.75, 4.5]], dtype=">f4")
    path = tmp_path / "big-endian.wx1"
    wx1.write_array(
        path,
        source,
        dataset_id="big-endian",
        variable_path="/field",
        dimensions=["y", "x"],
        chunk_shape=[1, 2],
    )
    with wx1.open_file(path) as file:
        restored = file.read_array("/field")
        assert restored.dtype == np.dtype(">f4")
        np.testing.assert_array_equal(restored.view(">u4"), source.view(">u4"))


def test_native_xarray_dask_chunked_read_and_reduction(tmp_path):
    da = pytest.importorskip("dask.array")
    source = np.arange(96, dtype="<f4").reshape(4, 6, 4) / np.float32(11.0)
    path = tmp_path / "native-dask.wx1"
    wx1.write_array(
        path,
        source,
        dataset_id="native-dask",
        variable_path="/temperature",
        dimensions=["time", "y", "x"],
        chunk_shape=[1, 3, 2],
        dimension_roles=["time", "latitude", "longitude"],
    )

    dataset = wx1.open_dataset(path, chunks={"time": 1, "y": 3, "x": 2})
    try:
        assert isinstance(dataset["/temperature"].data, da.Array)
        actual = dataset["/temperature"].mean().compute().item()
        assert actual == pytest.approx(float(source.mean()))
    finally:
        dataset.close()


@pytest.mark.parametrize(
    "source",
    [
        np.array([[0, 1, 65535], [12, 42, 1024]], dtype="<u2"),
        np.array([[1.0, -2.0], [3.5, 4.25]], dtype="<f8"),
        np.array([[True, False], [False, True]], dtype=np.bool_),
    ],
    ids=["uint16", "float64", "bool8"],
)
def test_native_numpy_roundtrip_preserves_supported_dtype_and_values(tmp_path, source):
    path = tmp_path / f"dtype-{source.dtype.name}.wx1"
    wx1.write_array(
        path,
        source,
        dataset_id=f"dtype-{source.dtype.name}",
        variable_path="/field",
        dimensions=["y", "x"],
        chunk_shape=[1, 2],
    )
    with wx1.open_file(path) as file:
        restored = file.read_array("/field")
        assert restored.dtype == source.dtype
        np.testing.assert_array_equal(restored, source)
        if source.dtype.kind in ("u", "i", "f"):
            unsigned = f"u{source.dtype.itemsize}"
            np.testing.assert_array_equal(restored.view(unsigned), source.view(unsigned))


def test_lazy_open_and_explicit_validate(tmp_path):
    source = np.arange(24, dtype="<f4").reshape(4, 6)
    path = tmp_path / "lazy-open.wx1"
    wx1.write_array(
        path,
        source,
        dataset_id="lazy-open-test",
        variable_path="/temperature",
        dimensions=["y", "x"],
        chunk_shape=[2, 3],
    )

    with wx1.open_file(path) as file:
        assert file.dataset_id == "lazy-open-test"
        # Explicit full validation passes on intact file
        file.validate()
        restored = file.read_array("/temperature")
        np.testing.assert_array_equal(restored, source)


def test_adr0024_restart_profile_forbids_lossy_and_preserves_bit_exact_with_byteshuffle(tmp_path):
    source = np.random.RandomState(42).normal(280.0, 10.0, (32, 32)).astype("<f4")
    bad_path = tmp_path / "bad-restart.wx1"
    with pytest.raises(ValueError, match="lossy quantization is strictly forbidden in restart profile"):
        wx1.write_array(
            bad_path,
            source,
            dataset_id="bad-restart",
            variable_path="/t2m",
            dimensions=["y", "x"],
            profile="restart",
            lossy_error_bound=0.01,
        )

    good_path = tmp_path / "good-restart-shuffle.wx1"
    wx1.write_array(
        good_path,
        source,
        dataset_id="good-restart",
        variable_path="/t2m",
        dimensions=["y", "x"],
        chunk_shape=[16, 16],
        profile="restart",
        codec="zstd_shuffle",
    )
    with wx1.open_file(good_path) as file:
        file.validate()
        restored = file.read_array("/t2m")
        np.testing.assert_array_equal(restored.view("u4"), source.view("u4"))
        attrs = file.variables[0]["attributes"]
        assert attrs["wx1:lossy"] == "false"
        assert attrs["wx1:profile"] == "restart"


def test_adr0024_output_profile_lossy_quantization_and_special_floats(tmp_path):
    rng = np.random.RandomState(123)
    source = rng.uniform(250.0, 310.0, (32, 32)).astype("<f4")
    # Inject special IEEE 754 values
    source[0, 0] = np.nan
    source[0, 1] = np.inf
    source[0, 2] = -np.inf
    source[0, 3] = np.float32(-0.0)
    source[0, 4] = np.float32(0.0)

    lossy_path = tmp_path / "lossy-output.wx1"
    eps = 0.01
    wx1.write_array(
        lossy_path,
        source,
        dataset_id="lossy-output",
        variable_path="/t2m",
        dimensions=["y", "x"],
        chunk_shape=[16, 16],
        profile="output",
        lossy_error_bound=eps,
        lossy_metric="max_abs_error",
        codec="zstd_shuffle",
    )

    with wx1.open_file(lossy_path) as file:
        file.validate()
        restored = file.read_array("/t2m")
        attrs = file.variables[0]["attributes"]
        assert attrs["wx1:lossy"] == "true"
        assert attrs["wx1:profile"] == "output"
        assert attrs["wx1:quantization_type"] == "linear_absolute"
        assert float(attrs["wx1:error_bound"]) == pytest.approx(eps)
        assert float(attrs["wx1:max_measured_abs_error"]) <= eps

        # Verify special floats
        assert np.isnan(restored[0, 0])
        assert np.isposinf(restored[0, 1])
        assert np.isneginf(restored[0, 2])
        assert restored[0, 3].view("u4") == np.float32(-0.0).view("u4")
        assert restored[0, 4].view("u4") == np.float32(0.0).view("u4")

        # Verify finite elements strictly obey declared error bound
        finite_mask = np.isfinite(source)
        diffs = np.abs(restored[finite_mask] - source[finite_mask])
        assert np.max(diffs) <= eps


def test_adr0025_parallel_workers_and_async_writer(tmp_path):
    rng = np.random.RandomState(2025)
    source = rng.normal(285.0, 5.0, (4, 32, 32)).astype("<f4")
    p1 = tmp_path / "w1.wx1"
    p4 = tmp_path / "w4.wx1"

    wx1.write_array(
        p1,
        source,
        dataset_id="parallel-test",
        variable_path="/t2m",
        dimensions=["z", "y", "x"],
        chunk_shape=[1, 16, 16],
        codec="zstd_shuffle",
        workers=1,
    )
    wx1.write_array(
        p4,
        source,
        dataset_id="parallel-test",
        variable_path="/t2m",
        dimensions=["z", "y", "x"],
        chunk_shape=[1, 16, 16],
        codec="zstd_shuffle",
        workers=4,
    )
    assert p1.read_bytes() == p4.read_bytes()

    async_path = tmp_path / "async_nwp.wx1"
    with wx1.AsyncWriter(
        async_path,
        dataset_id="nwp-async",
        variable_path="/t2m",
        spatial_dimensions=["y", "x"],
        spatial_shape=[32, 32],
        spatial_chunk_shape=[16, 16],
        codec="zstd_shuffle",
        workers=4,
        max_pending_generations=2,
        durability="buffered",
    ) as writer:
        for step in range(4):
            writer.submit_step(source[step])
        tel = writer.flush()
        assert tel["submitted_generations"] == 4
        assert tel["committed_generations"] == 4

    with wx1.open_file(async_path) as f:
        assert f.generation == 4
        f.validate()
        restored = f.read_array("/t2m")
        np.testing.assert_array_equal(restored.view("u4"), source.view("u4"))


def test_adr0026_python_cubed_sphere_halos_and_tile_read(tmp_path):
    # C16 cubed-sphere with halo_width=2 -> allocated shape (6, 20, 20)
    rng = np.random.RandomState(2026)
    source = rng.normal(290.0, 4.0, (6, 20, 20)).astype("<f4")
    cs_path = tmp_path / "cs_c16.wx1"

    wx1.write_array(
        cs_path,
        source,
        dataset_id="cs-c16",
        variable_path="/t",
        dimensions=["tile", "y", "x"],
        chunk_shape=[1, 20, 20],
        codec="zstd_shuffle",
        workers=4,
        grid_family="cubed_sphere",
        c_resolution=16,
        halo_width=2,
        stagger_horizontal="center",
        tile_dimension="tile",
    )

    with wx1.open_file(cs_path) as f:
        f.validate()
        topos = f.grid_topologies
        assert "primary_grid" in topos
        assert topos["primary_grid"]["family"] == "cubed_sphere"
        assert topos["primary_grid"]["c_resolution"] == 16
        assert topos["primary_grid"]["num_tiles"] == 6

        plan2 = f.plan_tile_slice("/t", 2, include_halos=False)
        assert plan2["chunk_ids"] == (3,)
        assert plan2["shape"] == (1, 16, 16)

        tile2_int = f.read_tile_slice("/t", 2, include_halos=False)
        assert tile2_int.shape == (1, 16, 16)
        np.testing.assert_array_equal(tile2_int[0], source[2, 2:18, 2:18])

        tile2_full = f.read_tile_slice("/t", 2, include_halos=True)
        assert tile2_full.shape == (1, 20, 20)
        np.testing.assert_array_equal(tile2_full[0], source[2])



