"""xarray backend with lazy per-selection reads from a WX-1 snapshot."""

from __future__ import annotations

from collections.abc import Iterable
from pathlib import Path

import numpy as np
import xarray as xr
from xarray.backends import BackendEntrypoint
from xarray.backends.common import BackendArray
from xarray.core.indexing import (
    IndexingSupport,
    LazilyIndexedArray,
    explicit_indexing_adapter,
)

from . import open_file


# ==============================================================================
# BLOCK 1: LAZY SUB-VOLUME SLICING ADAPTER (Wx1BackendArray)
# Purpose: Intercept xarray / Dask index keys (slices, steps, integers) and
#          map them to bounded WX-1 read_array_slice calls without loading
#          unselected chunks into memory.
# ==============================================================================

class Wx1BackendArray(BackendArray):
    """BackendArray that requests only the chunks intersecting each selection."""

    def __init__(self, file, variable):
        self._file = file
        self._path = variable["path"]
        self.shape = tuple(int(value) for value in variable["shape"])
        self.dtype = np.dtype(variable["numpy_dtype"])

    def __getitem__(self, key):
        return explicit_indexing_adapter(
            key,
            self.shape,
            IndexingSupport.BASIC,
            self._raw_indexing_method,
        )

    def _raw_indexing_method(self, key):
        starts = []
        counts = []
        post_index = []
        for axis, (selector, length) in enumerate(zip(key, self.shape)):
            if isinstance(selector, (int, np.integer)):
                index = int(selector)
                if index < 0:
                    index += length
                if index < 0 or index >= length:
                    raise IndexError(f"index {selector} is out of bounds for axis {axis} with size {length}")
                starts.append(index)
                counts.append(1)
                post_index.append(0)
                continue
            if not isinstance(selector, slice):
                raise IndexError("WX-1 backend supports basic integer and slice indexing")
            begin, end, step = selector.indices(length)
            if step > 0:
                low, high = begin, end
                local_start = 0
                local_stop = max(0, high - low)
                local_step = step
            else:
                if begin < end:
                    low, high = 0, 0
                    local_start, local_stop, local_step = 0, 0, 1
                else:
                    low, high = end + 1, begin + 1
                    local_start = max(0, high - low - 1)
                    local_stop = None
                    local_step = step
            starts.append(low)
            counts.append(max(0, high - low))
            post_index.append(slice(local_start, local_stop, local_step))

        array = self._file.read_array_slice(self._path, starts, counts)
        return array[tuple(post_index)]


# ==============================================================================
# BLOCK 2: XARRAY ENGINE ENTRYPOINT (engine="wx1")
# Purpose: Integrate WX-1 natively into xarray.open_dataset, populating dataset
#          coordinates, attributes, and variables with lazy Dask-compatible backend arrays.
# ==============================================================================

class Wx1BackendEntrypoint(BackendEntrypoint):
    """xarray entrypoint registered as ``engine='wx1'``."""

    description = "Experimental WX-1 scientific container backend"
    def open_dataset(
        self,
        filename_or_obj: str | Path,
        *,
        drop_variables: str | Iterable[str] | None = None,
    ) -> xr.Dataset:
        if isinstance(drop_variables, str):
            drop_variables = {drop_variables}
        path = Path(filename_or_obj)
        file = open_file(path)
        variables = {}
        for item in file.variables:
            variable_path = item["path"]
            if drop_variables and variable_path in drop_variables:
                continue
            dtype = np.dtype(
                {
                    "uint8": "u1", "int8": "i1", "uint16": "u2", "int16": "i2",
                    "uint32": "u4", "int32": "i4", "uint64": "u8", "int64": "i8",
                    "float32": "f4", "float64": "f8", "bool": "b1",
                }[item["dtype"]]
            )
            if dtype.itemsize > 1:
                dtype = dtype.newbyteorder("<" if item["byte_order"] == "little" else ">")
            item_for_backend = dict(item)
            item_for_backend["numpy_dtype"] = dtype.str
            backend_array = Wx1BackendArray(file, item_for_backend)
            lazy_data = LazilyIndexedArray(backend_array)
            variables[variable_path] = xr.Variable(
                tuple(item["dimensions"]),
                lazy_data,
                attrs=dict(item["attributes"]),
            )

        dataset = xr.Dataset(
            data_vars=variables,
            attrs={"wx1_dataset_id": file.dataset_id, "wx1_generation": file.generation, **file.attributes},
        )
        coordinate_paths = [path for path in file.coordinate_variables if path in dataset.variables]
        if coordinate_paths:
            dataset = dataset.set_coords(coordinate_paths)
        dataset.set_close(file.close)
        return dataset

    def guess_can_open(self, filename_or_obj):
        return str(filename_or_obj).lower().endswith(".wx1")
