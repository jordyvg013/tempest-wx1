"""GRIB2 benchmark adapter using ecCodes and cfgrib."""

from __future__ import annotations

from pathlib import Path
from typing import Any, Mapping
import numpy as np
import xarray as xr
import eccodes
import cfgrib

from bench.adapters.base import BenchmarkAdapter


class GRIB2Adapter(BenchmarkAdapter):
    def __init__(self):
        super().__init__("grib2", "GRIB2/ecCodes", "bounded_error")

    def write(self, dataset: xr.Dataset, destination: Path, settings: Mapping[str, Any] | None = None) -> Path:
        dest_path = Path(destination)
        if dest_path.exists():
            dest_path.unlink()

        gid = eccodes.codes_grib_new_from_samples("regular_ll_sfc_grib2")
        try:
            eccodes.codes_set(gid, "shortName", "2t")
            eccodes.codes_set(gid, "Ni", int(len(dataset["longitude"])))
            eccodes.codes_set(gid, "Nj", int(len(dataset["latitude"])))
            eccodes.codes_set(gid, "latitudeOfFirstGridPointInDegrees", float(dataset["latitude"].values[0]))
            eccodes.codes_set(gid, "latitudeOfLastGridPointInDegrees", float(dataset["latitude"].values[-1]))
            eccodes.codes_set(gid, "longitudeOfFirstGridPointInDegrees", float(dataset["longitude"].values[0]))
            eccodes.codes_set(gid, "longitudeOfLastGridPointInDegrees", float(dataset["longitude"].values[-1]))
            
            lat_step = abs(float(dataset["latitude"].values[0] - dataset["latitude"].values[1]))
            lon_step = abs(float(dataset["longitude"].values[1] - dataset["longitude"].values[0]))
            eccodes.codes_set(gid, "jDirectionIncrementInDegrees", lat_step)
            eccodes.codes_set(gid, "iDirectionIncrementInDegrees", lon_step)

            bits_per_val = 16
            if settings and "bits_per_value" in settings:
                bits_per_val = int(settings["bits_per_value"])
            eccodes.codes_set(gid, "bitsPerValue", bits_per_val)

            temp_vals = dataset["temperature"].values.astype(np.float64).flatten()
            eccodes.codes_set_values(gid, temp_vals)

            with open(dest_path, "wb") as f:
                eccodes.codes_write(gid, f)
        finally:
            eccodes.codes_release(gid)

        return dest_path

    def open(self, path: Path) -> Any:
        self.opened_handle = cfgrib.open_dataset(str(path))
        return self.opened_handle

    def read_field(self, var_name: str) -> np.ndarray:
        if self.opened_handle is None:
            raise RuntimeError("Adapter is not opened")
        # cfgrib standardizes 2t to t2m
        if var_name == "temperature" and "t2m" in self.opened_handle:
            return self.opened_handle["t2m"].values
        elif var_name in self.opened_handle:
            return self.opened_handle[var_name].values
        first_var = list(self.opened_handle.data_vars.keys())[0]
        return self.opened_handle[first_var].values

    def close(self) -> None:
        if self.opened_handle is not None:
            self.opened_handle.close()
            self.opened_handle = None
