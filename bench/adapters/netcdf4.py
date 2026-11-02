"""NetCDF4 benchmark adapter using xarray and netcdf4."""

from __future__ import annotations

from pathlib import Path
from typing import Any, Mapping
import numpy as np
import xarray as xr

from bench.adapters.base import BenchmarkAdapter


class NetCDF4Adapter(BenchmarkAdapter):
    def __init__(self):
        super().__init__("netcdf4", "NetCDF4", "lossless")

    def write(self, dataset: xr.Dataset, destination: Path, settings: Mapping[str, Any] | None = None) -> Path:
        dest_path = Path(destination)
        if dest_path.exists():
            dest_path.unlink()
        dataset.to_netcdf(dest_path, engine="netcdf4")
        return dest_path

    def open(self, path: Path) -> Any:
        self.opened_handle = xr.open_dataset(path, engine="netcdf4")
        return self.opened_handle

    def read_field(self, var_name: str) -> np.ndarray:
        if self.opened_handle is None:
            raise RuntimeError("Adapter is not opened")
        return self.opened_handle[var_name].values

    def close(self) -> None:
        if self.opened_handle is not None:
            self.opened_handle.close()
            self.opened_handle = None
