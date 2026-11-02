"""Optional external scientific-format readers and bidirectional adapters (ADR 0030)."""

from __future__ import annotations

from . import bufr_crex, grib2, hdf_geotiff, netcdf, ukmo_radar, zarr
from .bufr_crex import from_bufr, from_crex, to_bufr, to_crex
from .grib2 import from_grib, to_grib
from .hdf_geotiff import (
    from_geotiff,
    from_hdf4,
    from_hdf5,
    from_odim_h5,
    to_geotiff,
    to_hdf4,
    to_hdf5,
    to_odim_h5,
)
from .netcdf import from_netcdf, to_netcdf
from .ukmo_radar import (
    from_dorade,
    from_iwrf,
    from_nimrod,
    from_pp,
    to_dorade,
    to_iwrf,
    to_nimrod,
    to_pp,
)
from .zarr import from_zarr, to_zarr

__all__ = [
    "grib2",
    "netcdf",
    "zarr",
    "bufr_crex",
    "hdf_geotiff",
    "ukmo_radar",
    "from_grib",
    "to_grib",
    "from_netcdf",
    "to_netcdf",
    "from_zarr",
    "to_zarr",
    "from_bufr",
    "to_bufr",
    "from_crex",
    "to_crex",
    "from_hdf5",
    "to_hdf5",
    "from_odim_h5",
    "to_odim_h5",
    "from_hdf4",
    "to_hdf4",
    "from_geotiff",
    "to_geotiff",
    "from_pp",
    "to_pp",
    "from_nimrod",
    "to_nimrod",
    "from_dorade",
    "to_dorade",
    "from_iwrf",
    "to_iwrf",
]
