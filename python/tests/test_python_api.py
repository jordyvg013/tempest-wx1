# Copyright 2026 Jordy van Gils
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

from __future__ import annotations

import sys
from pathlib import Path
from types import SimpleNamespace

import numpy as np
import pytest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import wx1


class FakeNativeFile:
    dataset_id = "python-fixture"
    generation = 1

    def __init__(self, array):
        self.array = np.asarray(array)
        self.calls = []

    def attributes(self):
        return [("source", "pytest-fixture")]

    def dimensions(self):
        return [("y", self.array.shape[0], "latitude"), ("x", self.array.shape[1], "longitude")]

    def coordinate_variables(self):
        return []

    def variables(self):
        return [{
            "path": "/temperature",
            "name": "temperature",
            "group": "/",
            "dtype": "float32",
            "byte_order": "little",
            "dimensions": ["y", "x"],
            "shape": list(self.array.shape),
            "chunk_shape": [2, 2],
            "attributes": {"units": "K", "standard_name": "air_temperature"},
        }]

    def plan_array_slice(self, path, start, count):
        assert path == "/temperature"
        self.calls.append(("plan", tuple(start), tuple(count)))
        return ([1], int(np.prod(count)) * 4)

    def read_array_slice(self, path, start, count):
        assert path == "/temperature"
        self.calls.append(("read", tuple(start), tuple(count)))
        selection = tuple(slice(s, s + c) for s, c in zip(start, count))
        data = np.ascontiguousarray(self.array[selection], dtype="<f4")
        return data.tobytes(), "float32", "little", list(data.shape)

    def close(self):
        pass


def test_numpy_read_slice_preserves_shape_values_and_dtype():
    source = np.arange(16, dtype="<f4").reshape(4, 4)
    native = FakeNativeFile(source)
    file = wx1.Wx1File(native)

    result = file.read_array_slice("/temperature", [1, 1], [2, 2])

    assert result.dtype == np.dtype("<f4")
    assert result.shape == (2, 2)
    np.testing.assert_array_equal(result, source[1:3, 1:3])
    assert not result.flags.writeable


def test_numpy_roundtrip_wrapper_sends_exact_c_order_bytes(monkeypatch, tmp_path):
    captured = {}

    def write_array(*args):
        captured["args"] = args

    monkeypatch.setattr(wx1, "_core", SimpleNamespace(write_array=write_array))
    source = np.arange(24, dtype="<f4").reshape(4, 6)
    wx1.write_array(
        tmp_path / "array.wx1",
        source,
        dataset_id="numpy-roundtrip",
        variable_path="/temperature",
        dimensions=["y", "x"],
        chunk_shape=[2, 3],
        dimension_roles=["latitude", "longitude"],
        attributes={"units": "K"},
    )

    args = captured["args"]
    assert args[1] == "numpy-roundtrip"
    assert args[2] == "/temperature"
    assert args[4] == [4, 6]
    assert args[5] == [2, 3]
    assert args[6] == "float32"
    assert args[7] == "little"
    np.testing.assert_array_equal(np.frombuffer(args[8], dtype="<f4").reshape(4, 6), source)
    assert args[9] == ["latitude", "longitude"]
    assert args[10] == {"units": "K"}


def test_write_array_rejects_unsupported_dtype(monkeypatch, tmp_path):
    monkeypatch.setattr(wx1, "_core", SimpleNamespace(write_array=lambda *args: None))
    with pytest.raises(TypeError, match="Unsupported WX-1 NumPy dtype"):
        wx1.write_array(
            tmp_path / "bad.wx1", np.array([1 + 2j]), dataset_id="bad",
            variable_path="/complex", dimensions=["x"],
        )


def test_public_api_reports_missing_native_extension(monkeypatch, tmp_path):
    monkeypatch.setattr(wx1, "_core", None)
    with pytest.raises(ImportError, match="native extension is not built"):
        wx1.open_file(tmp_path / "missing.wx1")


def test_numpy_big_endian_dtype_is_preserved_by_writer_wrapper(monkeypatch, tmp_path):
    captured = {}
    monkeypatch.setattr(wx1, "_core", SimpleNamespace(write_array=lambda *args: captured.setdefault("args", args)))
    source = np.array([1.25, -2.5], dtype=">f4")
    wx1.write_array(
        tmp_path / "big-endian.wx1", source, dataset_id="big-endian",
        variable_path="/field", dimensions=["x"], chunk_shape=[1],
    )
    args = captured["args"]
    assert args[6] == "float32"
    assert args[7] == "big"
    np.testing.assert_array_equal(np.frombuffer(args[8], dtype=">f4"), source)


def test_numpy_non_contiguous_input_is_normalized_to_c_order(monkeypatch, tmp_path):
    captured = {}
    monkeypatch.setattr(
        wx1,
        "_core",
        SimpleNamespace(write_array=lambda *args: captured.setdefault("args", args)),
    )
    source = np.arange(24, dtype="<i4").reshape(4, 6).T
    assert not source.flags.c_contiguous

    wx1.write_array(
        tmp_path / "non-contiguous.wx1",
        source,
        dataset_id="non-contiguous",
        variable_path="/field",
        dimensions=["x", "y"],
        chunk_shape=[2, 2],
    )

    args = captured["args"]
    restored_bytes = np.frombuffer(args[8], dtype="<i4").reshape(source.shape)
    np.testing.assert_array_equal(restored_bytes, source)


def test_adr0027_units_and_cf_registry_in_python_api():
    assert wx1.unit_dimension("degC") == "temperature"
    assert wx1.unit_dimension("K") == "temperature"
    assert wx1.unit_dimension("hPa") == "pressure"
    assert wx1.unit_dimension("m s-1") == "velocity"

    scale, offset = wx1.unit_conversion_factor("degC", "K")
    assert abs(scale - 1.0) < 1e-9
    assert abs(offset - 273.15) < 1e-9
    assert abs((25.0 * scale + offset) - 298.15) < 1e-9

    with pytest.raises(ValueError):
        wx1.unit_conversion_factor("hPa", "K")

    cf = wx1.resolve_cf_standard_name("air_temperature")
    assert cf is not None
    assert cf["canonical_id"] == "air_temperature"
    assert cf["canonical_units"] == "K"
    assert cf["physical_dimension"] == "temperature"

