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

"""Bidirectional NetCDF-4 adapter for WX-1 using netCDF4 / CF conventions (ADR 0030).

Provides lossless import from NetCDF-4 / CF datasets into WX-1 scientific containers
with metadata preservation, and export from WX-1 containers back to NetCDF-4 files.
"""

from __future__ import annotations

import datetime
from pathlib import Path
from typing import Any, Mapping, Sequence


# ==============================================================================
# BLOCK 1: NETCDF4 DEPENDENCY & CF ROLE INFERENCE
# Purpose: Check netCDF4 library availability and map NetCDF dimension names
#          (lat, lon, lev, time, member) to WX-1 canonical dimension roles.
# ==============================================================================

def _require_netcdf4():
    try:
        import netCDF4
        return netCDF4
    except ImportError as exc:
        raise ImportError(
            "NetCDF adapter requires the 'netcdf4' extra (netCDF4 Python package)"
        ) from exc


def open_dataset(path, *, engine=None, chunks=None, **kwargs):
    """Open NetCDF dataset with xarray."""
    try:
        import xarray as xr
    except ImportError as exc:
        raise ImportError("NetCDF adapter requires the 'xarray' extra") from exc
    return xr.open_dataset(path, engine=engine, chunks=chunks, **kwargs)


def _infer_dim_role(dim_name: str) -> str:
    name = dim_name.lower()
    if any(h in name for h in ("lat", "latitude", "y", "j")):
        return "latitude"
    if any(h in name for h in ("lon", "longitude", "x", "i")):
        return "longitude"
    if any(s in name for s in ("soil", "layer", "depth")):
        return "soil_layer"
    if any(v in name for v in ("lev", "pres", "height", "z", "sig", "hybrid")):
        return "vertical"
    if any(t in name for t in ("time", "step", "date", "valid_time")):
        return "time"
    if any(e in name for e in ("ens", "member", "realization")):
        return "ensemble"
    return "other"


# ==============================================================================
# BLOCK 2: NETCDF-4 TO WX-1 SCIENTIFIC CONTAINER IMPORT (from_netcdf)
# Purpose: Import NetCDF-4 variables into WX-1, preserving attributes, NaN masks,
#          and dimension roles while re-chunking under 32 MiB bounds.
# ==============================================================================

def from_netcdf(
    input_path: str | Path,
    output_wx1_path: str | Path,
    *,
    variable_name: str | None = None,
    dataset_id: str | None = None,
    chunk_shape: Sequence[int] | None = None,
    codec: str = "zstd_shuffle",
    workers: int = 1,
    profile: str = "restart",
    lossy_error_bound: float | None = None,
    lossy_metric: str = "max_abs_error",
    durability: str = "durable",
):
    """Import a NetCDF-4 / CF dataset into a WX-1 scientific container (ADR 0030)."""
    import numpy as np
    import wx1

    netCDF4 = _require_netcdf4()
    in_file = Path(input_path)
    if not in_file.exists():
        raise FileNotFoundError(f"NetCDF file not found: {in_file}")

    with netCDF4.Dataset(in_file, "r") as ds:
        var_names = list(ds.variables.keys())
        if not var_names:
            raise ValueError(f"No variables found in NetCDF file: {in_file}")

        # Choose target variable
        if variable_name is not None:
            if variable_name not in ds.variables:
                raise KeyError(
                    f"Variable '{variable_name}' not found. Available in NetCDF: {var_names}"
                )
            selected_var_name = variable_name
        else:
            # Prefer multidimensional variables over 1D coordinate variables
            multi_dim_vars = [k for k in var_names if ds.variables[k].ndim >= 2]
            selected_var_name = multi_dim_vars[0] if multi_dim_vars else var_names[0]

        nc_var = ds.variables[selected_var_name]
        dims = list(nc_var.dimensions)
        roles = [_infer_dim_role(d) for d in dims]

        attrs: dict[str, str] = {
            "wx1:source_format": "netcdf4",
            "wx1:source_file": in_file.name,
            "wx1:converted_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        }

        # Global dataset attributes
        for g_attr in ds.ncattrs():
            val = getattr(ds, g_attr)
            attrs[f"netcdf:global:{g_attr}"] = str(val)

        # Variable attributes
        for attr in nc_var.ncattrs():
            val = getattr(nc_var, attr)
            if attr == "standard_name":
                attrs["cf:standard_name"] = str(val)
                attrs["standard_name"] = str(val)
            elif attr == "long_name":
                attrs["cf:long_name"] = str(val)
                attrs["long_name"] = str(val)
            elif attr == "units":
                attrs["units"] = str(val)
            elif attr == "_FillValue":
                attrs["_FillValue"] = str(val)
            else:
                attrs[f"netcdf:{attr}"] = str(val)

        # Read array data
        try:
            nc_var.set_auto_maskandscale(False)
        except Exception:
            pass
        raw_data = nc_var[:]
        if hasattr(raw_data, "filled"):
            fill_val = getattr(nc_var, "_FillValue", None)
            if fill_val is not None:
                array_data = raw_data.filled(fill_val)
            else:
                array_data = raw_data.filled(np.nan)
        else:
            array_data = np.asarray(raw_data)

        ds_id = dataset_id or f"{in_file.stem}_{selected_var_name}"
        var_path = f"/{selected_var_name}"

        # Ensure chunk size stays within safe raw limit (<= 32 MiB)
        if chunk_shape is not None:
            resolved_chunk_shape = list(chunk_shape)
        else:
            resolved_chunk_shape = list(array_data.shape)
            max_chunk_bytes = 32 * 1024 * 1024
            elem_size = max(1, array_data.dtype.itemsize)
            for idx in range(len(resolved_chunk_shape)):
                while resolved_chunk_shape[idx] > 1 and int(np.prod(resolved_chunk_shape)) * elem_size > max_chunk_bytes:
                    resolved_chunk_shape[idx] = max(1, resolved_chunk_shape[idx] // 2)

        wx1.write_array(
            output_wx1_path,
            array_data,
            dataset_id=ds_id,
            variable_path=var_path,
            dimensions=dims,
            chunk_shape=resolved_chunk_shape,
            dimension_roles=roles,
            attributes=attrs,
            profile=profile,
            lossy_error_bound=lossy_error_bound,
            lossy_metric=lossy_metric,
            codec=codec,
            workers=workers,
            durability=durability,
        )

        return wx1.open_file(output_wx1_path)


# ==============================================================================
# BLOCK 3: WX-1 TO NETCDF-4 EXPORT (to_netcdf)
# Purpose: Export WX-1 containers back to compliant CF NetCDF-4/Classic files
#          with optional zlib compression and fill values.
# ==============================================================================

def to_netcdf(
    input_wx1_path: str | Path,
    output_nc_path: str | Path,
    *,
    variable_path: str | None = None,
    format: str = "NETCDF4",
    zlib: bool | None = None,
    complevel: int = 4,
    shuffle: bool = True,
) -> Path:
    """Export a WX-1 scientific container back to a NetCDF-4 or Classic NetCDF-3 file (ADR 0030)."""
    import numpy as np
    import wx1

    netCDF4 = _require_netcdf4()
    in_file = Path(input_wx1_path)
    out_file = Path(output_nc_path)

    with wx1.open_file(in_file) as f:
        vars_list = f.variables
        if not vars_list:
            raise ValueError(f"No variables found in WX-1 container: {in_file}")

        if variable_path is not None:
            var_info = next((v for v in vars_list if v["path"] == variable_path), None)
            if var_info is None:
                raise KeyError(f"Variable '{variable_path}' not found in WX-1 container")
        else:
            var_info = vars_list[0]

        data = f.read_array(var_info["path"])
        attrs = f.attributes
        dims = var_info["dimensions"]

    out_file.parent.mkdir(parents=True, exist_ok=True)
    is_netcdf4 = format.upper().startswith("NETCDF4")
    use_zlib = (zlib if zlib is not None else is_netcdf4) and is_netcdf4

    with netCDF4.Dataset(out_file, "w", format=format) as ds:
        # Create dimensions
        for d_name, size in zip(dims, data.shape):
            ds.createDimension(d_name, size)

        # Parse _FillValue if present
        fill_val = None
        if "_FillValue" in attrs:
            try:
                fill_val = np.dtype(data.dtype).type(attrs["_FillValue"])
            except Exception:
                pass

        # Create variable
        nc_dtype = data.dtype
        var_name = var_info["path"].strip("/").split("/")[-1]
        create_kwargs: dict[str, Any] = {"fill_value": fill_val}
        if use_zlib:
            create_kwargs["zlib"] = True
            create_kwargs["complevel"] = complevel
            create_kwargs["shuffle"] = shuffle
            if var_info.get("chunk_shape"):
                create_kwargs["chunksizes"] = tuple(
                    min(int(c), int(s)) for c, s in zip(var_info["chunk_shape"], data.shape)
                )

        nc_var = ds.createVariable(
            var_name,
            nc_dtype,
            tuple(dims),
            **create_kwargs,
        )
        try:
            nc_var.set_auto_maskandscale(False)
        except Exception:
            pass

        def _parse_typed_attr(k: str, v_str: str) -> Any:
            if k in ("scale_factor", "add_offset"):
                try:
                    return np.float64(v_str)
                except Exception:
                    return v_str
            return v_str

        # Restore attributes
        for key, val in attrs.items():
            if key.startswith("netcdf:global:"):
                g_key = key[len("netcdf:global:"):]
                try:
                    setattr(ds, g_key, val)
                except Exception:
                    pass
            elif key in ("_FillValue", "missing_value"):
                continue  # Handled in createVariable
            elif key in ("units", "standard_name", "long_name"):
                try:
                    setattr(nc_var, key, str(val))
                except Exception:
                    pass
            elif key == "cf:standard_name":
                try:
                    setattr(nc_var, "standard_name", str(val))
                except Exception:
                    pass
            elif key == "cf:long_name":
                try:
                    setattr(nc_var, "long_name", str(val))
                except Exception:
                    pass
            elif key.startswith("netcdf:"):
                attr_name = key[len("netcdf:"):]
                try:
                    setattr(nc_var, attr_name, _parse_typed_attr(attr_name, val))
                except Exception:
                    pass
            elif not key.startswith("wx1:"):
                try:
                    setattr(nc_var, key, _parse_typed_attr(key, val))
                except Exception:
                    pass

        # Write array
        nc_var[:] = data

    return out_file
