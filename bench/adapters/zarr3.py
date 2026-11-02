"""Zarr 3 benchmark adapter using xarray and zarr."""

from __future__ import annotations

import shutil
from pathlib import Path
from typing import Any, Mapping
import numpy as np
import xarray as xr

from bench.adapters.base import BenchmarkAdapter


class Zarr3Adapter(BenchmarkAdapter):
    def __init__(self):
        super().__init__("zarr3", "Zarr 3", "lossless")

    def write(self, dataset: xr.Dataset, destination: Path, settings: Mapping[str, Any] | None = None) -> Path:
        dest_path = Path(destination)
        if dest_path.exists():
            if dest_path.is_dir():
                shutil.rmtree(dest_path)
            else:
                dest_path.unlink()
        try:
            dataset.to_zarr(dest_path, mode="w", zarr_format=3)
        except TypeError:
            dataset.to_zarr(dest_path, mode="w")
        return dest_path

    def open(self, path: Path) -> Any:
        self.opened_handle = xr.open_zarr(path)
        return self.opened_handle

    def read_field(self, var_name: str) -> np.ndarray:
        if self.opened_handle is None:
            raise RuntimeError("Adapter is not opened")
        return self.opened_handle[var_name].values

    def close(self) -> None:
        if self.opened_handle is not None:
            self.opened_handle.close()
            self.opened_handle = None
