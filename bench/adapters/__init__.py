"""Benchmark adapters."""

from bench.adapters.base import BenchmarkAdapter
from bench.adapters.wx1 import WX1Adapter
from bench.adapters.grib2 import GRIB2Adapter
from bench.adapters.netcdf4 import NetCDF4Adapter
from bench.adapters.zarr3 import Zarr3Adapter

__all__ = ["BenchmarkAdapter", "WX1Adapter", "GRIB2Adapter", "NetCDF4Adapter", "Zarr3Adapter"]
