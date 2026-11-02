"""Bidirectional GRIB2 adapter for WX-1 using ecCodes (ADR 0030).

Provides lossless import from WMO GRIB2 files into WX-1 scientific containers
with complete metadata preservation (namespaced under ``grib:*``, ``cf:*``),
and export from WX-1 containers back to valid WMO GRIB2 files.
"""

from __future__ import annotations

import datetime
from pathlib import Path
from typing import Any, Mapping, Sequence


GRIB_METADATA_KEYS = [
    "edition",
    "centre",
    "subCentre",
    "discipline",
    "parameterCategory",
    "parameterNumber",
    "productDefinitionTemplateNumber",
    "shortName",
    "name",
    "units",
    "typeOfLevel",
    "level",
    "dataDate",
    "dataTime",
    "forecastTime",
    "stepUnits",
    "stepRange",
    "stepType",
    "gridType",
    "shapeOfTheEarth",
    "Ni",
    "Nj",
    "numberOfPoints",
    "latitudeOfFirstGridPointInDegrees",
    "longitudeOfFirstGridPointInDegrees",
    "latitudeOfLastGridPointInDegrees",
    "longitudeOfLastGridPointInDegrees",
    "iDirectionIncrementInDegrees",
    "jDirectionIncrementInDegrees",
    "iScansNegatively",
    "jScansPositively",
    "jPointsAreConsecutive",
    "packingType",
    "bitsPerValue",
    "perturbationNumber",
    "numberOfForecastsInEnsemble",
    "typeOfProcessedData",
]

# Keys that can be safely written to ecCodes handles on export
GRIB_EXPORT_INT_KEYS = {
    "discipline",
    "parameterCategory",
    "parameterNumber",
    "productDefinitionTemplateNumber",
    "dataDate",
    "dataTime",
    "forecastTime",
    "stepUnits",
    "level",
    "Ni",
    "Nj",
    "shapeOfTheEarth",
    "iScansNegatively",
    "jScansPositively",
    "jPointsAreConsecutive",
    "bitsPerValue",
    "perturbationNumber",
    "numberOfForecastsInEnsemble",
    "typeOfProcessedData",
}

GRIB_EXPORT_FLOAT_KEYS = {
    "latitudeOfFirstGridPointInDegrees",
    "longitudeOfFirstGridPointInDegrees",
    "latitudeOfLastGridPointInDegrees",
    "longitudeOfLastGridPointInDegrees",
    "iDirectionIncrementInDegrees",
    "jDirectionIncrementInDegrees",
}

GRIB_EXPORT_STR_KEYS = {
    "centre",
    "shortName",
    "typeOfLevel",
    "stepType",
}


def _require_eccodes():
    try:
        import eccodes
        return eccodes
    except ImportError as exc:
        raise ImportError(
            "GRIB2 adapter requires the 'grib2' extra (eccodes Python package)"
        ) from exc


def open_dataset(path, *, filter_by_keys=None, indexpath="", **kwargs):
    """Open GRIB2 dataset with xarray + cfgrib backend."""
    try:
        import xarray as xr
    except ImportError as exc:
        raise ImportError("GRIB2 adapter requires the 'xarray' extra") from exc
    try:
        import cfgrib  # noqa: F401
    except ImportError as exc:
        raise ImportError("GRIB2 adapter requires cfgrib and ecCodes") from exc
    backend_kwargs = {"indexpath": indexpath}
    if filter_by_keys is not None:
        backend_kwargs["filter_by_keys"] = dict(filter_by_keys)
    return xr.open_dataset(path, engine="cfgrib", backend_kwargs=backend_kwargs, **kwargs)


def from_grib(
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
    """Import a WMO GRIB2 file into a WX-1 scientific container (ADR 0030).

    Extracts numerical payload and preserves all WMO metadata under the ``grib:*``
    namespace, alongside canonical CF/physical attributes.
    """
    import numpy as np
    import wx1

    eccodes = _require_eccodes()
    in_file = Path(input_path)
    if not in_file.exists():
        raise FileNotFoundError(f"GRIB2 file not found: {in_file}")

    messages = []
    with open(in_file, "rb") as f:
        while True:
            gid = eccodes.codes_grib_new_from_file(f)
            if gid is None:
                break
            try:
                sname = eccodes.codes_get(gid, "shortName")
            except Exception:
                sname = "unknown"
            messages.append((gid, sname))

    if not messages:
        raise ValueError(f"No valid GRIB2 messages found in: {in_file}")

    try:
        # Filter by requested variable_name or default to the first variable
        if variable_name is not None:
            selected_messages = [m for m in messages if m[1] == str(variable_name)]
            if not selected_messages:
                available = sorted({m[1] for m in messages})
                raise KeyError(
                    f"Variable '{variable_name}' not found. Available in GRIB: {available}"
                )
        else:
            first_var = messages[0][1]
            selected_messages = [m for m in messages if m[1] == first_var]

        # Extract primary message metadata
        primary_gid, var_short_name = selected_messages[0]
        attrs: dict[str, str] = {
            "wx1:source_format": "grib2",
            "wx1:source_file": in_file.name,
            "wx1:converted_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        }

        for key in GRIB_METADATA_KEYS:
            try:
                val = eccodes.codes_get(primary_gid, key)
                attrs[f"grib:{key}"] = str(val)
            except Exception:
                pass

        # Standard physical and CF conventions
        if "grib:units" in attrs:
            attrs["units"] = attrs["grib:units"]
        if "grib:shortName" in attrs:
            attrs["cf:standard_name"] = attrs["grib:shortName"]
        if "grib:name" in attrs:
            attrs["cf:long_name"] = attrs["grib:name"]

        # Grid dimensions
        ni = int(attrs.get("grib:Ni", 0))
        nj = int(attrs.get("grib:Nj", 0))

        # Check multi-level / multi-member or single level
        num_levels = len(selected_messages)
        all_members = []
        for gid, _ in selected_messages:
            try:
                all_members.append(int(eccodes.codes_get(gid, "perturbationNumber")))
            except Exception:
                pass
        is_ensemble = len(all_members) == num_levels and len(set(all_members)) > 1

        if num_levels > 1:
            if is_ensemble:
                def _msg_sort_key(item):
                    try:
                        return int(eccodes.codes_get(item[0], "perturbationNumber"))
                    except Exception:
                        return 0
            else:
                def _msg_sort_key(item):
                    try:
                        return int(eccodes.codes_get(item[0], "level"))
                    except Exception:
                        return 0
            selected_messages.sort(key=_msg_sort_key)

        all_levels = []
        sorted_members = []
        data_slices = []

        for gid, _ in selected_messages:
            try:
                lvl = eccodes.codes_get(gid, "level")
                all_levels.append(int(lvl))
            except Exception:
                all_levels.append(0)
            if is_ensemble:
                try:
                    sorted_members.append(int(eccodes.codes_get(gid, "perturbationNumber")))
                except Exception:
                    sorted_members.append(0)
            values = eccodes.codes_get_values(gid)
            # If values can be represented bit-exactly as float32, keep float32
            f32_vals = values.astype(np.float32)
            if np.all(f32_vals.astype(np.float64) == values):
                data_slices.append(f32_vals)
            else:
                data_slices.append(values)

        tol = attrs.get("grib:typeOfLevel", "").lower()
        if is_ensemble:
            v_dim = "member"
            v_role = "ensemble"
            attrs["grib:ensemble_members"] = ",".join(str(m) for m in sorted_members)
        elif "soil" in tol:
            v_dim = "soil_layer"
            v_role = "soil_layer"
        else:
            v_dim = "level"
            v_role = "vertical"

        if num_levels > 1:
            attrs["grib:levels"] = ",".join(str(lvl) for lvl in all_levels)
            if ni > 0 and nj > 0:
                stacked = np.stack([s.reshape((nj, ni)) for s in data_slices], axis=0)
                dims = [v_dim, "latitude", "longitude"]
                roles = [v_role, "latitude", "longitude"]
            else:
                stacked = np.stack(data_slices, axis=0)
                dims = [v_dim, "points"]
                roles = [v_role, "other"]
        else:
            if ni > 0 and nj > 0:
                stacked = data_slices[0].reshape((nj, ni))
                dims = ["latitude", "longitude"]
                roles = ["latitude", "longitude"]
            else:
                stacked = data_slices[0]
                dims = ["points"]
                roles = ["other"]

        ds_id = dataset_id or f"{in_file.stem}_{var_short_name}"
        var_path = f"/{var_short_name}"

        # Ensure chunk size stays within safe raw limit (<= 32 MiB)
        if chunk_shape is not None:
            resolved_chunk_shape = list(chunk_shape)
        else:
            resolved_chunk_shape = list(stacked.shape)
            max_chunk_bytes = 32 * 1024 * 1024
            elem_size = max(1, stacked.dtype.itemsize)
            for idx in range(len(resolved_chunk_shape)):
                while resolved_chunk_shape[idx] > 1 and int(np.prod(resolved_chunk_shape)) * elem_size > max_chunk_bytes:
                    resolved_chunk_shape[idx] = max(1, resolved_chunk_shape[idx] // 2)

        wx1.write_array(
            output_wx1_path,
            stacked,
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

    finally:
        for gid, _ in messages:
            try:
                eccodes.codes_release(gid)
            except Exception:
                pass


def to_grib(
    input_wx1_path: str | Path,
    output_grib_path: str | Path,
    *,
    variable_path: str | None = None,
    template_grib: str | Path | None = None,
    packing_type: str | None = None,
    bits_per_value: int | None = None,
) -> Path:
    """Export a WX-1 scientific container back to a WMO GRIB2 file (ADR 0030).

    Uses preserved ``grib:*`` metadata to configure GRIB2 messages and write
    lossless numerical grids with ecCodes.
    """
    import numpy as np
    import wx1

    eccodes = _require_eccodes()
    in_file = Path(input_wx1_path)
    out_file = Path(output_grib_path)

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

    # Determine levels/members and slices
    shape = data.shape
    ndim = data.ndim
    members_str = attrs.get("grib:ensemble_members", "")
    ensemble_members = [int(x.strip()) for x in members_str.split(",") if x.strip()] if members_str else []

    if ndim == 3:
        # (levels/members, lat, lon)
        num_slices = shape[0]
        slices = [data[i] for i in range(num_slices)]
        levels_str = attrs.get("grib:levels", "")
        if levels_str:
            levels = [int(x.strip()) for x in levels_str.split(",") if x.strip()]
        else:
            levels = list(range(1, num_slices + 1))
    elif ndim == 2:
        # (lat, lon)
        num_slices = 1
        slices = [data]
        try:
            levels = [int(attrs.get("grib:level", 1))]
        except Exception:
            levels = [1]
    else:
        # 1D points or 4D (time, level, lat, lon)
        flattened = data.reshape(-1, *shape[-2:]) if ndim > 2 else [data]
        num_slices = len(flattened)
        slices = [flattened[i] for i in range(num_slices)]
        levels = list(range(1, num_slices + 1))

    target_packing = packing_type or attrs.get("grib:packingType", "grid_ccsds")
    target_bpv = bits_per_value
    if target_bpv is None and "grib:bitsPerValue" in attrs:
        try:
            target_bpv = int(attrs["grib:bitsPerValue"])
        except Exception:
            pass

    # Open template if supplied
    template_gid = None
    if template_grib is not None:
        with open(template_grib, "rb") as tf:
            template_gid = eccodes.codes_grib_new_from_file(tf)

    out_file.parent.mkdir(parents=True, exist_ok=True)
    with open(out_file, "wb") as out_f:
        try:
            for idx, slice_arr in enumerate(slices):
                if template_gid is not None:
                    gid = eccodes.codes_clone(template_gid)
                else:
                    # Choose appropriate sample
                    type_of_level = attrs.get("grib:typeOfLevel", "surface")
                    if "isobaric" in type_of_level.lower():
                        sample = "regular_ll_pl_grib2"
                    else:
                        sample = "regular_ll_sfc_grib2"
                    gid = eccodes.codes_grib_new_from_samples(sample)
                    try:
                        eccodes.codes_set(gid, "typeOfLevel", type_of_level)
                    except Exception:
                        pass

                # Set grid dimensions if 2D
                if slice_arr.ndim == 2:
                    eccodes.codes_set(gid, "Nj", int(slice_arr.shape[0]))
                    eccodes.codes_set(gid, "Ni", int(slice_arr.shape[1]))

                # Set productDefinitionTemplateNumber first if ensemble (template 1)
                if "grib:productDefinitionTemplateNumber" in attrs:
                    try:
                        eccodes.codes_set(
                            gid,
                            "productDefinitionTemplateNumber",
                            int(attrs["grib:productDefinitionTemplateNumber"]),
                        )
                    except Exception:
                        pass
                elif ensemble_members:
                    try:
                        eccodes.codes_set(gid, "productDefinitionTemplateNumber", 1)
                    except Exception:
                        pass

                # Apply preserved grib:* attributes
                for key, val in attrs.items():
                    if not key.startswith("grib:"):
                        continue
                    bare_key = key[5:]
                    if bare_key in GRIB_EXPORT_INT_KEYS and bare_key not in (
                        "level",
                        "Ni",
                        "Nj",
                        "productDefinitionTemplateNumber",
                        "perturbationNumber",
                        "bitsPerValue",
                    ):
                        try:
                            eccodes.codes_set(gid, bare_key, int(val))
                        except Exception:
                            pass
                    elif bare_key in GRIB_EXPORT_FLOAT_KEYS:
                        try:
                            eccodes.codes_set(gid, bare_key, float(val))
                        except Exception:
                            pass
                    elif bare_key in GRIB_EXPORT_STR_KEYS:
                        try:
                            eccodes.codes_set(gid, bare_key, str(val))
                        except Exception:
                            pass

                # Set level and ensemble perturbationNumber for this slice
                if idx < len(levels):
                    try:
                        eccodes.codes_set(gid, "level", int(levels[idx]))
                    except Exception:
                        pass
                if idx < len(ensemble_members):
                    try:
                        eccodes.codes_set(gid, "perturbationNumber", int(ensemble_members[idx]))
                    except Exception:
                        pass

                # Configure packingType and bitsPerValue before codes_set_values
                if target_bpv is not None and target_bpv > 0:
                    try:
                        eccodes.codes_set(gid, "bitsPerValue", int(target_bpv))
                    except Exception:
                        pass
                if target_packing:
                    try:
                        eccodes.codes_set(gid, "packingType", str(target_packing))
                    except Exception:
                        pass

                # Set values
                flat_data = np.ascontiguousarray(slice_arr.ravel(), dtype=np.float64)
                try:
                    eccodes.codes_set_values(gid, flat_data)
                except Exception:
                    # Fallback if constant field or CCSDS constraint fails on specific field
                    eccodes.codes_set(gid, "packingType", "grid_simple")
                    eccodes.codes_set_values(gid, flat_data)

                # Write message
                eccodes.codes_write(gid, out_f)
                eccodes.codes_release(gid)

        finally:
            if template_gid is not None:
                eccodes.codes_release(template_gid)

    return out_file
