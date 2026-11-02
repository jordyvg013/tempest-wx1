from __future__ import annotations

import sys
from pathlib import Path
from types import SimpleNamespace

import numpy as np
import pytest
import xarray as xr

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import wx1
import wx1.xarray_backend as backend


class FakeNativeFile:
    dataset_id = "xarray-fixture"
    generation = 1

    def __init__(self):
        self.data = np.arange(16, dtype="<f4").reshape(4, 4)
        self.calls = []

    def attributes(self):
        return [("institution", "pytest")]

    def dimensions(self):
        return [("y", 4, "latitude"), ("x", 4, "longitude")]

    def coordinate_variables(self):
        return []

    def variables(self):
        return [{
            "path": "/temperature", "name": "temperature", "group": "/",
            "dtype": "float32", "byte_order": "little", "dimensions": ["y", "x"],
            "shape": [4, 4], "chunk_shape": [2, 2], "attributes": {"units": "K"},
        }]

    def plan_array_slice(self, path, start, count):
        return ([1], int(np.prod(count)) * 4)

    def read_array_slice(self, path, start, count):
        self.calls.append((tuple(start), tuple(count)))
        data = np.ascontiguousarray(
            self.data[tuple(slice(s, s + c) for s, c in zip(start, count))], dtype="<f4"
        )
        return data.tobytes(), "float32", "little", list(data.shape)

    def close(self):
        pass


def test_xarray_materialization_is_lazy_and_reads_only_selection(monkeypatch, tmp_path):
    native = FakeNativeFile()
    monkeypatch.setattr(backend, "open_file", lambda path: wx1.Wx1File(native))
    engine = backend.Wx1BackendEntrypoint()

    dataset = engine.open_dataset(tmp_path / "fixture.wx1")
    try:
        assert native.calls == []
        assert dataset.attrs["wx1_dataset_id"] == "xarray-fixture"
        assert dataset["/temperature"].attrs["units"] == "K"

        result = dataset["/temperature"].isel(y=slice(1, 3), x=slice(0, 2)).values

        np.testing.assert_array_equal(result, native.data[1:3, 0:2])
        assert native.calls == [((1, 0), (2, 2))]
    finally:
        dataset.close()


def test_xarray_backend_registered_class_can_identify_wx1_path():
    engine = backend.Wx1BackendEntrypoint()
    assert engine.guess_can_open("sample.wx1") is True
    assert engine.guess_can_open("sample.nc") is False



def test_xarray_backend_signature_is_explicit_and_engine_class_works(monkeypatch, tmp_path):
    import inspect

    native = FakeNativeFile()
    monkeypatch.setattr(backend, "open_file", lambda path: wx1.Wx1File(native))
    signature = inspect.signature(backend.Wx1BackendEntrypoint.open_dataset)
    assert all(
        parameter.kind not in (inspect.Parameter.VAR_POSITIONAL, inspect.Parameter.VAR_KEYWORD)
        for parameter in signature.parameters.values()
    )

    dataset = xr.open_dataset(
        tmp_path / "fixture.wx1",
        engine=backend.Wx1BackendEntrypoint,
    )
    try:
        assert dataset.attrs["wx1_dataset_id"] == "xarray-fixture"
        assert native.calls == []
    finally:
        dataset.close()

def test_dask_chunked_execution_when_optional_dependency_is_installed(monkeypatch, tmp_path):
    pytest.importorskip("dask.array")
    native = FakeNativeFile()
    monkeypatch.setattr(backend, "open_file", lambda path: wx1.Wx1File(native))
    dataset = xr.open_dataset(
        tmp_path / "fixture.wx1",
        engine=backend.Wx1BackendEntrypoint,
        chunks={"y": 2, "x": 2},
    )
    try:
        assert hasattr(dataset["/temperature"].data, "compute")
        result = dataset["/temperature"].mean().compute().item()
        assert result == pytest.approx(float(native.data.mean()))
    finally:
        dataset.close()


def test_xarray_strided_and_reverse_slices_read_only_bounding_region(monkeypatch, tmp_path):
    native = FakeNativeFile()
    monkeypatch.setattr(backend, "open_file", lambda path: wx1.Wx1File(native))
    dataset = backend.Wx1BackendEntrypoint().open_dataset(tmp_path / "fixture.wx1")
    try:
        actual = dataset["/temperature"].isel(y=slice(None, None, -1), x=slice(1, None, 2)).values
        expected = native.data[::-1, 1::2]
        np.testing.assert_array_equal(actual, expected)
        # The backend reads one bounding rectangle and applies strides in memory.
        assert native.calls == [((0, 1), (4, 3))]
    finally:
        dataset.close()


def test_xarray_negative_integer_index_is_supported(monkeypatch, tmp_path):
    native = FakeNativeFile()
    monkeypatch.setattr(backend, "open_file", lambda path: wx1.Wx1File(native))
    dataset = backend.Wx1BackendEntrypoint().open_dataset(tmp_path / "fixture.wx1")
    try:
        actual = dataset["/temperature"].isel(y=-1).values
        np.testing.assert_array_equal(actual, native.data[-1, :])
    finally:
        dataset.close()
