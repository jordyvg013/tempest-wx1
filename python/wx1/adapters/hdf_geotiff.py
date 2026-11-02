"""Bidirectional adapters for HDF5 (.h5 / .hdf5), EUMETNET OPERA ODIM_H5, HDF4 (.hdf), and GeoTIFF (.tif) (ADR 0030).

Supports:
- Generic HDF5 (`.h5`, `.hdf5`) with gzip+shuffle compression via `h5py`.
- EUMETNET OPERA Weather Radar Information Model (`ODIM_H5/V2_4`) with `/what`, `/where`, `/how`, `/dataset1/data1/data`.
- Classic HDF4 Scientific Data Set (`.hdf`) via `pyhdf.SD` with DEFLATE compression.
- GeoTIFF (`.tif`) with Zstd or Deflate predictor compression via `tifffile`.
"""

from __future__ import annotations

import datetime
from pathlib import Path
from typing import Any, Sequence


def _infer_dim_role(dim_name: str) -> str:
    name = dim_name.lower()
    if any(h in name for h in ("lat", "latitude", "y", "j", "azimuth", "ray")):
        return "latitude"
    if any(h in name for h in ("lon", "longitude", "x", "i", "range", "bin", "gate")):
        return "longitude"
    if any(s in name for s in ("soil", "layer", "depth")):
        return "soil_layer"
    if any(v in name for v in ("lev", "pres", "height", "z", "elev", "sweep", "tilt")):
        return "vertical"
    if any(t in name for t in ("time", "step", "date", "valid_time")):
        return "time"
    if any(e in name for e in ("ens", "member", "realization")):
        return "ensemble"
    return "other"


def _resolve_safe_chunks(shape: Sequence[int], elem_size: int, chunk_shape: Sequence[int] | None = None) -> list[int]:
    import numpy as np

    resolved = list(chunk_shape) if chunk_shape is not None else list(shape)
    max_chunk_bytes = 32 * 1024 * 1024
    for idx in range(len(resolved)):
        while resolved[idx] > 1 and int(np.prod(resolved)) * max(1, elem_size) > max_chunk_bytes:
            resolved[idx] = max(1, resolved[idx] // 2)
    return resolved


# ---------------------------------------------------------------------------
# 1. Generic HDF5 (.h5 / .hdf5)
# ---------------------------------------------------------------------------
def from_hdf5(
    input_path: str | Path,
    output_wx1_path: str | Path,
    *,
    dataset_name: str | None = None,
    dataset_id: str | None = None,
    chunk_shape: Sequence[int] | None = None,
    codec: str = "zstd_shuffle",
    workers: int = 1,
    profile: str = "restart",
    durability: str = "durable",
):
    """Import an HDF5 (.h5 / .hdf5) file into a WX-1 scientific container."""
    import h5py
    import numpy as np
    import wx1

    in_file = Path(input_path)
    if not in_file.exists():
        raise FileNotFoundError(f"HDF5 file not found: {in_file}")

    with h5py.File(in_file, "r") as hf:
        # Check if ODIM_H5
        conv = hf.attrs.get("Conventions", b"")
        if isinstance(conv, bytes):
            conv = conv.decode("utf-8", errors="replace")
        if "ODIM_H5" in str(conv):
            return from_odim_h5(
                input_path,
                output_wx1_path,
                dataset_id=dataset_id,
                chunk_shape=chunk_shape,
                codec=codec,
                workers=workers,
                profile=profile,
                durability=durability,
            )

        found_datasets: list[tuple[str, h5py.Dataset]] = []

        def _visitor(name: str, obj: Any):
            if isinstance(obj, h5py.Dataset) and obj.ndim >= 1:
                found_datasets.append((name, obj))

        hf.visititems(_visitor)
        if not found_datasets:
            raise ValueError(f"No datasets found in HDF5 file: {in_file}")

        if dataset_name is not None:
            clean_target = dataset_name.strip("/")
            selected = next(( pair for pair in found_datasets if pair[0].strip("/") == clean_target ), None)
            if selected is None:
                raise KeyError(f"Dataset '{dataset_name}' not found in HDF5. Available: {[p[0] for p in found_datasets]}")
            ds_path, h5_ds = selected
        else:
            # Prefer highest-dimensional dataset
            found_datasets.sort(key=lambda item: (item[1].ndim, item[1].size), reverse=True)
            ds_path, h5_ds = found_datasets[0]

        data = np.asarray(h5_ds[...])
        attrs: dict[str, str] = {
            "wx1:source_format": "hdf5",
            "wx1:source_file": in_file.name,
            "wx1:converted_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "hdf5:dataset_path": f"/{ds_path.strip('/')}",
        }
        for k, v in hf.attrs.items():
            attrs[f"hdf5:global:{k}"] = v.decode("utf-8", "replace") if isinstance(v, bytes) else str(v)
        for k, v in h5_ds.attrs.items():
            if k in ("DIMENSION_LIST", "REFERENCE_LIST", "CLASS", "NAME"):
                continue
            val_str = v.decode("utf-8", "replace") if isinstance(v, bytes) else str(v)
            if k in ("units", "_FillValue", "standard_name", "long_name"):
                attrs[k] = val_str
            else:
                attrs[f"hdf5:{k}"] = val_str

        dims_str = attrs.get("hdf5:dimensions", "")
        if dims_str:
            dims = [d.strip() for d in dims_str.split(",") if d.strip()]
        elif data.ndim == 2:
            dims = ["latitude", "longitude"]
        elif data.ndim == 3:
            dims = ["level", "latitude", "longitude"]
        else:
            dims = [f"dim_{i}" for i in range(data.ndim)]

        roles = [_infer_dim_role(d) for d in dims]
        var_name = ds_path.strip("/").split("/")[-1]
        resolved_chunks = _resolve_safe_chunks(data.shape, data.dtype.itemsize, chunk_shape)

        wx1.write_array(
            output_wx1_path,
            data,
            dataset_id=dataset_id or f"{in_file.stem}_{var_name}",
            variable_path=f"/{var_name}",
            dimensions=dims,
            chunk_shape=resolved_chunks,
            dimension_roles=roles,
            attributes=attrs,
            profile=profile,
            codec=codec,
            workers=workers,
            durability=durability,
        )
    return wx1.open_file(output_wx1_path)


def to_hdf5(
    input_wx1_path: str | Path,
    output_h5_path: str | Path,
    *,
    variable_path: str | None = None,
    compression: str = "gzip",
    compression_opts: int = 4,
    shuffle: bool = True,
) -> Path:
    """Export a WX-1 scientific container to an HDF5 (.h5 / .hdf5) file with gzip+shuffle."""
    import h5py
    import numpy as np
    import wx1

    in_file = Path(input_wx1_path)
    out_file = Path(output_h5_path)

    with wx1.open_file(in_file) as f:
        var_info = f.variables[0] if variable_path is None else next(
            v for v in f.variables if v["path"] == variable_path
        )
        data = np.asarray(f.read_array(var_info["path"]))
        attrs = f.attributes
        dims = var_info["dimensions"]
        chunks = tuple(min(int(c), int(s)) for c, s in zip(var_info["chunk_shape"], data.shape))

    out_file.parent.mkdir(parents=True, exist_ok=True)
    with h5py.File(out_file, "w") as hf:
        for k, v in attrs.items():
            if k.startswith("hdf5:global:"):
                hf.attrs[k[len("hdf5:global:"):]] = str(v)
        ds_name = attrs.get("hdf5:dataset_path", var_info["path"]).strip("/")
        h5_ds = hf.create_dataset(
            ds_name,
            data=data,
            chunks=chunks,
            compression=compression,
            compression_opts=compression_opts if compression == "gzip" else None,
            shuffle=shuffle,
        )
        h5_ds.attrs["dimensions"] = ",".join(dims)
        for k, v in attrs.items():
            if k.startswith(("wx1:", "hdf5:global:", "hdf5:dataset_path")):
                continue
            bare = k[5:] if k.startswith("hdf5:") else k
            h5_ds.attrs[bare] = str(v)

    return out_file


# ---------------------------------------------------------------------------
# 2. EUMETNET OPERA ODIM_H5 Radar Format (.odim.h5)
# ---------------------------------------------------------------------------
def to_odim_h5(
    input_wx1_path: str | Path,
    output_odim_path: str | Path,
    *,
    variable_path: str | None = None,
    quantity: str = "DBZH",
    gain: float = 0.5,
    offset: float = -32.0,
    nodata: float = 255.0,
    undetect: float = 0.0,
) -> Path:
    """Export a WX-1 container to an EUMETNET OPERA ODIM_H5 v2.4 compliant radar HDF5 file."""
    import h5py
    import numpy as np
    import wx1

    in_file = Path(input_wx1_path)
    out_file = Path(output_odim_path)

    with wx1.open_file(in_file) as f:
        var_info = f.variables[0] if variable_path is None else next(
            v for v in f.variables if v["path"] == variable_path
        )
        data = np.asarray(f.read_array(var_info["path"]))
        attrs = f.attributes
        chunks = tuple(min(int(c), int(s)) for c, s in zip(var_info["chunk_shape"], data.shape))

    out_file.parent.mkdir(parents=True, exist_ok=True)
    with h5py.File(out_file, "w") as hf:
        hf.attrs["Conventions"] = np.bytes_("ODIM_H5/V2_4")

        # Root /what
        g_what = hf.create_group("what")
        g_what.attrs["object"] = np.bytes_(attrs.get("odim:object", "COMP" if data.ndim == 2 else "PVOL"))
        g_what.attrs["version"] = np.bytes_("H5rad 2.4")
        g_what.attrs["date"] = np.bytes_(attrs.get("odim:date", "20261029"))
        g_what.attrs["time"] = np.bytes_(attrs.get("odim:time", "120000"))
        g_what.attrs["source"] = np.bytes_(attrs.get("odim:source", "WMO:06260,PLC:De Bilt,NOD:nldeb"))

        def _clean_val(v, default):
            if v is None: return default
            if isinstance(v, (int, float)): return float(v)
            s = str(v).strip("[]b'\" \t\r\n")
            try: return float(s)
            except Exception: return default

        # Root /where
        g_where = hf.create_group("where")
        g_where.attrs["lon"] = _clean_val(attrs.get("odim:lon"), 5.178)
        g_where.attrs["lat"] = _clean_val(attrs.get("odim:lat"), 52.101)
        g_where.attrs["height"] = _clean_val(attrs.get("odim:height"), 44.0)
        if data.ndim >= 2:
            g_where.attrs["xsize"] = int(data.shape[-1])
            g_where.attrs["ysize"] = int(data.shape[-2])
            g_where.attrs["xscale"] = _clean_val(attrs.get("odim:xscale"), 1000.0)
            g_where.attrs["yscale"] = _clean_val(attrs.get("odim:yscale"), 1000.0)

        # Root /how
        g_how = hf.create_group("how")
        g_how.attrs["system"] = np.bytes_(attrs.get("odim:system", "KNMI/ECMWF-WX1-RADAR"))
        g_how.attrs["software"] = np.bytes_("WX-1 ADR-0030")

        # /dataset1/data1
        ds1 = hf.create_group("dataset1")
        d1 = ds1.create_group("data1")
        d1_what = d1.create_group("what")
        d1_what.attrs["quantity"] = np.bytes_(attrs.get("odim:quantity", quantity))
        d1_what.attrs["gain"] = _clean_val(attrs.get("odim:gain"), gain)
        d1_what.attrs["offset"] = _clean_val(attrs.get("odim:offset"), offset)
        d1_what.attrs["nodata"] = _clean_val(attrs.get("odim:nodata"), nodata)
        d1_what.attrs["undetect"] = _clean_val(attrs.get("odim:undetect"), undetect)

        dset = d1.create_dataset(
            "data",
            data=data,
            chunks=chunks,
            compression="gzip",
            compression_opts=4,
            shuffle=True,
        )
        dset.attrs["CLASS"] = np.bytes_("IMAGE")
        dset.attrs["IMAGE_VERSION"] = np.bytes_("1.2")

        # Preserve any additional attributes under /how/wx1_attrs
        for k, v in attrs.items():
            if not k.startswith("wx1:"):
                g_how.attrs[f"wx1_{k}"] = str(v)

    return out_file


def from_odim_h5(
    input_path: str | Path,
    output_wx1_path: str | Path,
    *,
    dataset_id: str | None = None,
    chunk_shape: Sequence[int] | None = None,
    codec: str = "zstd_shuffle",
    workers: int = 1,
    profile: str = "restart",
    durability: str = "durable",
):
    """Import an EUMETNET OPERA ODIM_H5 v2.4 radar file into a WX-1 scientific container."""
    import h5py
    import numpy as np
    import wx1

    in_file = Path(input_path)
    if not in_file.exists():
        raise FileNotFoundError(f"ODIM_H5 file not found: {in_file}")

    with h5py.File(in_file, "r") as hf:
        if "dataset1/data1/data" not in hf:
            raise ValueError(f"Missing '/dataset1/data1/data' in ODIM_H5 file: {in_file}")
        dset = hf["dataset1/data1/data"]
        data = np.asarray(dset[...])

        attrs: dict[str, str] = {
            "wx1:source_format": "odim_h5",
            "wx1:source_file": in_file.name,
            "wx1:converted_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "odim:Conventions": "ODIM_H5/V2_4",
        }
        for grp_name in ("what", "where", "how", "dataset1/what", "dataset1/where", "dataset1/data1/what"):
            if grp_name in hf:
                for k, v in hf[grp_name].attrs.items():
                    val_str = v.decode("utf-8", "replace") if isinstance(v, bytes) else str(v)
                    if k.startswith("wx1_"):
                        attrs[k[4:]] = val_str
                    else:
                        attrs[f"odim:{k}"] = val_str

        raw_qty = attrs.get("odim:quantity", "DBZH")
        if isinstance(raw_qty, str):
            clean_qty = "".join(ch for ch in raw_qty if ch.isalnum() or ch in ("_", "-")).lower()
        else:
            clean_qty = "dbzh"
        qty = clean_qty or "dbzh"
        dims = ["azimuth", "range"] if data.ndim == 2 else ["elevation", "azimuth", "range"]
        roles = [_infer_dim_role(d) for d in dims]
        resolved_chunks = _resolve_safe_chunks(data.shape, data.dtype.itemsize, chunk_shape)

        wx1.write_array(
            output_wx1_path,
            data,
            dataset_id=dataset_id or f"{in_file.stem}_{qty}",
            variable_path=f"/{qty}",
            dimensions=dims,
            chunk_shape=resolved_chunks,
            dimension_roles=roles,
            attributes=attrs,
            profile=profile,
            codec=codec,
            workers=workers,
            durability=durability,
        )
    return wx1.open_file(output_wx1_path)


# ---------------------------------------------------------------------------
# 3. Classic HDF4 Scientific Data Set (.hdf)
# ---------------------------------------------------------------------------
def to_hdf4(
    input_wx1_path: str | Path,
    output_hdf_path: str | Path,
    *,
    variable_path: str | None = None,
    deflate_level: int = 4,
) -> Path:
    """Export a WX-1 scientific container to an HDF4 (.hdf) Scientific Data Set file."""
    import numpy as np
    from pyhdf.SD import SD, SDC
    import wx1

    in_file = Path(input_wx1_path)
    out_file = Path(output_hdf_path)

    with wx1.open_file(in_file) as f:
        var_info = f.variables[0] if variable_path is None else next(
            v for v in f.variables if v["path"] == variable_path
        )
        data = np.ascontiguousarray(f.read_array(var_info["path"]))
        attrs = f.attributes
        dims = var_info["dimensions"]

    dtype_map = {
        np.dtype("float32"): SDC.FLOAT32,
        np.dtype("float64"): SDC.FLOAT64,
        np.dtype("int16"): SDC.INT16,
        np.dtype("uint16"): SDC.UINT16,
        np.dtype("int32"): SDC.INT32,
        np.dtype("uint8"): SDC.UINT8,
    }
    sdc_type = dtype_map.get(data.dtype, SDC.FLOAT32)
    if data.dtype not in dtype_map:
        data = data.astype(np.float32)

    out_file.parent.mkdir(parents=True, exist_ok=True)
    if out_file.exists():
        out_file.unlink()

    sd = SD(str(out_file), SDC.WRITE | SDC.CREATE)
    try:
        var_name = var_info["path"].strip("/").split("/")[-1]
        sds = sd.create(var_name, sdc_type, tuple(int(s) for s in data.shape))
        try:
            if deflate_level > 0:
                sds.setcompress(SDC.COMP_DEFLATE, int(deflate_level))
            for idx, d_name in enumerate(dims):
                try:
                    sds.dim(idx).setname(str(d_name))
                except Exception:
                    pass
            for k, v in attrs.items():
                if not k.startswith("wx1:"):
                    try:
                        setattr(sds, k.replace(":", "__"), str(v)[:256])
                    except Exception:
                        pass
            sds[:] = data
        finally:
            sds.endaccess()
    finally:
        sd.end()

    return out_file


def from_hdf4(
    input_path: str | Path,
    output_wx1_path: str | Path,
    *,
    dataset_name: str | None = None,
    dataset_id: str | None = None,
    chunk_shape: Sequence[int] | None = None,
    codec: str = "zstd_shuffle",
    workers: int = 1,
    profile: str = "restart",
    durability: str = "durable",
):
    """Import an HDF4 (.hdf) Scientific Data Set file into a WX-1 scientific container."""
    import numpy as np
    from pyhdf.SD import SD, SDC
    import wx1

    in_file = Path(input_path)
    if not in_file.exists():
        raise FileNotFoundError(f"HDF4 file not found: {in_file}")

    sd = SD(str(in_file), SDC.READ)
    try:
        dsets = sd.datasets()
        if not dsets:
            raise ValueError(f"No Scientific Data Sets found in HDF4 file: {in_file}")
        selected_name = dataset_name if dataset_name in dsets else next(iter(dsets.keys()))
        sds = sd.select(selected_name)
        try:
            data = np.asarray(sds[:])
            dims_info = dsets[selected_name][0]
            dims = [str(d) if not str(d).startswith("fakeDim") else f"dim_{i}" for i, d in enumerate(dims_info)]
            if len(dims) == 2 and dims == ["dim_0", "dim_1"]:
                dims = ["latitude", "longitude"]
            roles = [_infer_dim_role(d) for d in dims]

            attrs: dict[str, str] = {
                "wx1:source_format": "hdf4",
                "wx1:source_file": in_file.name,
                "wx1:converted_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            }
            for k, v in sds.attributes().items():
                restored_key = k.replace("__", ":")
                attrs[restored_key] = str(v)
        finally:
            sds.endaccess()
    finally:
        sd.end()

    resolved_chunks = _resolve_safe_chunks(data.shape, data.dtype.itemsize, chunk_shape)
    wx1.write_array(
        output_wx1_path,
        data,
        dataset_id=dataset_id or f"{in_file.stem}_{selected_name}",
        variable_path=f"/{selected_name}",
        dimensions=dims,
        chunk_shape=resolved_chunks,
        dimension_roles=roles,
        attributes=attrs,
        profile=profile,
        codec=codec,
        workers=workers,
        durability=durability,
    )
    return wx1.open_file(output_wx1_path)


# ---------------------------------------------------------------------------
# 4. GeoTIFF (.tif)
# ---------------------------------------------------------------------------
def to_geotiff(
    input_wx1_path: str | Path,
    output_tif_path: str | Path,
    *,
    variable_path: str | None = None,
    compression: str = "zstd",
) -> Path:
    """Export a WX-1 scientific container to a cloud-friendly GeoTIFF (.tif) file."""
    import json
    import numpy as np
    import tifffile
    import wx1

    in_file = Path(input_wx1_path)
    out_file = Path(output_tif_path)

    with wx1.open_file(in_file) as f:
        var_info = f.variables[0] if variable_path is None else next(
            v for v in f.variables if v["path"] == variable_path
        )
        data = np.ascontiguousarray(f.read_array(var_info["path"]))
        attrs = dict(f.attributes)
        attrs["wx1:variable_path"] = var_info["path"]
        attrs["wx1:dimensions"] = ",".join(var_info["dimensions"])

    out_file.parent.mkdir(parents=True, exist_ok=True)
    predictor = 3 if np.issubdtype(data.dtype, np.floating) else 2
    tifffile.imwrite(
        out_file,
        data,
        compression=compression,
        predictor=predictor,
        description=json.dumps(attrs),
    )
    return out_file


def from_geotiff(
    input_path: str | Path,
    output_wx1_path: str | Path,
    *,
    dataset_id: str | None = None,
    chunk_shape: Sequence[int] | None = None,
    codec: str = "zstd_shuffle",
    workers: int = 1,
    profile: str = "restart",
    durability: str = "durable",
):
    """Import a GeoTIFF (.tif) file into a WX-1 scientific container."""
    import json
    import numpy as np
    import tifffile
    import wx1

    in_file = Path(input_path)
    if not in_file.exists():
        raise FileNotFoundError(f"GeoTIFF file not found: {in_file}")

    with tifffile.TiffFile(in_file) as tif:
        data = np.asarray(tif.asarray())
        desc = tif.pages[0].description if tif.pages else ""

    attrs: dict[str, str] = {
        "wx1:source_format": "geotiff",
        "wx1:source_file": in_file.name,
        "wx1:converted_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }
    if desc:
        try:
            parsed = json.loads(desc)
            if isinstance(parsed, dict):
                for k, v in parsed.items():
                    if not k.startswith("wx1:source_"):
                        attrs[str(k)] = str(v)
        except Exception:
            attrs["tiff:ImageDescription"] = str(desc)

    var_path = attrs.pop("wx1:variable_path", f"/{in_file.stem}")
    dims_str = attrs.pop("wx1:dimensions", "")
    if dims_str:
        dims = [d.strip() for d in dims_str.split(",") if d.strip()]
    else:
        dims = ["latitude", "longitude"] if data.ndim == 2 else ["level", "latitude", "longitude"]
    roles = [_infer_dim_role(d) for d in dims]
    resolved_chunks = _resolve_safe_chunks(data.shape, data.dtype.itemsize, chunk_shape)

    wx1.write_array(
        output_wx1_path,
        data,
        dataset_id=dataset_id or in_file.stem,
        variable_path=var_path,
        dimensions=dims,
        chunk_shape=resolved_chunks,
        dimension_roles=roles,
        attributes=attrs,
        profile=profile,
        codec=codec,
        workers=workers,
        durability=durability,
    )
    return wx1.open_file(output_wx1_path)
