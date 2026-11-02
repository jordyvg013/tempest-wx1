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

"""Bidirectional Zarr (v2 / v3) adapter for WX-1 using zarr-python (ADR 0030).

Provides lossless import from Zarr stores into WX-1 scientific containers
with chunk and metadata preservation, and export from WX-1 containers back to Zarr.
"""

from __future__ import annotations

import datetime
from pathlib import Path
from typing import Any, Mapping, Sequence


# ==============================================================================
# BLOCK 1: ZARR DEPENDENCY & ROLE INFERENCE
# Purpose: Check zarr-python library availability and map array dimensions to
#          canonical WX-1 dimension roles (time, vertical, lat, lon).
# ==============================================================================

def _require_zarr():
    try:
        import zarr
        return zarr
    except ImportError as exc:
        raise ImportError(
            "Zarr adapter requires the 'zarr' extra (zarr Python package)"
        ) from exc


def open_dataset(path, *, chunks="auto", consolidated=None, zarr_version=None, zarr_format=None, **kwargs):
    """Open Zarr dataset with xarray."""
    try:
        import xarray as xr
    except ImportError as exc:
        raise ImportError("Zarr adapter requires the 'xarray' extra") from exc
    _require_zarr()
    options = dict(kwargs)
    if consolidated is not None:
        options["consolidated"] = consolidated
    if zarr_version is not None and zarr_format is not None:
        raise ValueError("Pass zarr_version or zarr_format, not both")
    if zarr_format is not None:
        options["zarr_format"] = zarr_format
    elif zarr_version is not None:
        options["zarr_version"] = zarr_version
    return xr.open_zarr(path, chunks=chunks, **options)


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
# BLOCK 2: ZARR TO WX-1 SCIENTIFIC CONTAINER IMPORT (from_zarr)
# Purpose: Import Zarr array or hierarchy into WX-1 with preserved chunks and attributes.
# ==============================================================================

def from_zarr(
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
    """Import a Zarr array or group into a WX-1 scientific container (ADR 0030)."""
    import numpy as np
    import wx1

    zarr = _require_zarr()
    in_file = Path(input_path)
    if not in_file.exists():
        raise FileNotFoundError(f"Zarr store not found: {in_file}")

    target_array = None
    group_attrs: dict[str, Any] = {}
    selected_var_name = variable_name or "data"

    try:
        root = zarr.open_group(str(in_file), mode="r")
        group_attrs = dict(getattr(root, "attrs", {}))
        # Look for arrays in group
        keys = list(root.keys())
        if not keys:
            raise ValueError(f"No arrays found in Zarr group: {in_file}")
        if variable_name is not None:
            if variable_name not in root:
                raise KeyError(f"Array '{variable_name}' not found in Zarr group. Available: {keys}")
            selected_var_name = variable_name
            target_array = root[variable_name]
        else:
            selected_var_name = keys[0]
            target_array = root[selected_var_name]
    except Exception:
        # Fall back to opening as direct array
        try:
            target_array = zarr.open_array(str(in_file), mode="r")
            selected_var_name = variable_name or in_file.stem
        except Exception as exc:
            raise ValueError(f"Cannot open Zarr store at: {in_file}") from exc

    arr_attrs = dict(getattr(target_array, "attrs", {}))
    ndim = target_array.ndim

    # Infer dimension names
    if "_ARRAY_DIMENSIONS" in arr_attrs:
        dims = list(arr_attrs["_ARRAY_DIMENSIONS"])
    else:
        dims = [f"dim_{i}" for i in range(ndim)]

    roles = [_infer_dim_role(d) for d in dims]

    # Target chunk shape
    resolved_chunk_shape = list(chunk_shape or getattr(target_array, "chunks", None) or target_array.shape)
    max_chunk_bytes = 32 * 1024 * 1024
    elem_size = max(1, target_array.dtype.itemsize)
    for idx in range(len(resolved_chunk_shape)):
        while resolved_chunk_shape[idx] > 1 and int(np.prod(resolved_chunk_shape)) * elem_size > max_chunk_bytes:
            resolved_chunk_shape[idx] = max(1, resolved_chunk_shape[idx] // 2)

    attrs: dict[str, str] = {
        "wx1:source_format": "zarr",
        "wx1:source_file": in_file.name,
        "wx1:converted_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }

    for k, v in group_attrs.items():
        attrs[f"zarr:group:{k}"] = str(v)

    for k, v in arr_attrs.items():
        if k == "units":
            attrs["units"] = str(v)
        elif k in ("standard_name", "cf:standard_name"):
            attrs["cf:standard_name"] = str(v)
            attrs["standard_name"] = str(v)
        elif k in ("long_name", "cf:long_name"):
            attrs["cf:long_name"] = str(v)
            attrs["long_name"] = str(v)
        elif k != "_ARRAY_DIMENSIONS":
            attrs[f"zarr:{k}"] = str(v)

    if hasattr(target_array, "chunks"):
        attrs["zarr:chunks"] = ",".join(str(c) for c in target_array.chunks)

    data = np.asarray(target_array[:])

    ds_id = dataset_id or f"{in_file.stem}_{selected_var_name}"
    var_path = f"/{selected_var_name}"

    wx1.write_array(
        output_wx1_path,
        data,
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
# BLOCK 3: WX-1 SCIENTIFIC CONTAINER EXPORT TO ZARR (to_zarr)
# Purpose: Export WX-1 containers back to valid Zarr v2/v3 stores.
# ==============================================================================

def to_zarr(
    input_wx1_path: str | Path,
    output_zarr_path: str | Path,
    *,
    variable_path: str | None = None,
    chunks: Sequence[int] | None = None,
) -> Path:
    """Export a WX-1 scientific container back to a Zarr store (ADR 0030)."""
    import numpy as np
    import wx1

    zarr = _require_zarr()
    in_file = Path(input_wx1_path)
    out_dir = Path(output_zarr_path)

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
        stored_chunks = var_info["chunk_shape"]

    out_dir.parent.mkdir(parents=True, exist_ok=True)
    root = zarr.open_group(str(out_dir), mode="w")

    # Copy group-level attributes
    for k, v in attrs.items():
        if k.startswith("zarr:group:"):
            root.attrs[k[len("zarr:group:"):]] = v

    # Resolve chunks
    resolved_chunks = tuple(chunks) if chunks is not None else tuple(stored_chunks)
    var_name = var_info["path"].strip("/").split("/")[-1]

    fill_val = None
    if "_FillValue" in attrs:
        try:
            fill_val = np.dtype(data.dtype).type(attrs["_FillValue"])
        except Exception:
            pass

    create_kwargs = {}
    if fill_val is not None:
        create_kwargs["fill_value"] = fill_val

    # Create array compatible with Zarr 2 and 3
    if hasattr(root, "create_array"):
        z_arr = root.create_array(
            var_name,
            shape=data.shape,
            chunks=resolved_chunks,
            dtype=data.dtype,
            **create_kwargs,
        )
    elif hasattr(root, "create_dataset"):
        z_arr = root.create_dataset(
            var_name,
            shape=data.shape,
            chunks=resolved_chunks,
            dtype=data.dtype,
            **create_kwargs,
        )
    else:
        raise RuntimeError("Unsupported Zarr group API")

    # Set array dimensions convention for Xarray/CF compatibility
    z_arr.attrs["_ARRAY_DIMENSIONS"] = list(dims)

    # Restore attributes
    for k, v in attrs.items():
        if k.startswith("zarr:group:") or k == "zarr:chunks":
            continue
        if k in ("units", "standard_name", "long_name"):
            z_arr.attrs[k] = v
        elif k == "cf:standard_name":
            z_arr.attrs["standard_name"] = v
        elif k == "cf:long_name":
            z_arr.attrs["long_name"] = v
        elif k.startswith("zarr:"):
            z_arr.attrs[k[len("zarr:"):]] = v
        elif not k.startswith("wx1:"):
            z_arr.attrs[k] = v

    # Write data
    z_arr[:] = data
    return out_dir
