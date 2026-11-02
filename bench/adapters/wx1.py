"""WX-1 benchmark adapter."""

from __future__ import annotations

from pathlib import Path
from typing import Any, Mapping
import numpy as np
import xarray as xr

import wx1
from bench.adapters.base import BenchmarkAdapter


class WX1Adapter(BenchmarkAdapter):
    def __init__(self):
        super().__init__("wx1", "WX-1", "lossless")

    def write(self, dataset: xr.Dataset, destination: Path, settings: Mapping[str, Any] | None = None) -> Path:
        dest_path = Path(destination)
        if dest_path.exists():
            dest_path.unlink()

        temp_var = dataset["temperature"]
        values = temp_var.values.astype("<f4")
        chunk_shape = (16, 16)
        if settings and "chunk_shape" in settings:
            chunk_shape = tuple(settings["chunk_shape"])

        wx1.write_array(
            dest_path,
            values,
            dataset_id="wx1-bench-dataset",
            variable_path="/temperature",
            dimensions=["latitude", "longitude"],
            chunk_shape=chunk_shape,
            dimension_roles=["latitude", "longitude"],
            attributes={"units": "K", "standard_name": "air_temperature"},
        )
        return dest_path

    def open(self, path: Path) -> Any:
        self.opened_handle = wx1.open_file(path)
        return self.opened_handle

    def read_field(self, var_name: str) -> np.ndarray:
        if self.opened_handle is None:
            raise RuntimeError("Adapter is not opened")
        # Map var_name to internal variable path if needed
        var_path = f"/{var_name}" if not var_name.startswith("/") else var_name
        return self.opened_handle.read_array(var_path)

    def close(self) -> None:
        if self.opened_handle is not None:
            self.opened_handle.close()
            self.opened_handle = None
