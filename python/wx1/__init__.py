"""Experimental Python interface to WX-1 scientific containers."""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any, Mapping, Sequence

try:
    from . import _core
except ImportError:
    try:
        import _core
    except ImportError as exc:  # source tree without a built maturin extension
        _core = None
        _CORE_IMPORT_ERROR = exc
    else:
        _CORE_IMPORT_ERROR = None
else:
    _CORE_IMPORT_ERROR = None

__all__ = [
    "Wx1File",
    "AsyncWriter",
    "open_file",
    "open_dataset",
    "write_array",
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
    "convert",
    "write_restart_checkpoint",
    "read_restart_checkpoint",
]


def _require_core():
    if _core is None:
        raise ImportError(
            "The WX-1 native extension is not built. Install the package with "
            "maturin (for example: python -m pip install .)."
        ) from _CORE_IMPORT_ERROR
    return _core


def _numpy():
    try:
        import numpy as np
    except ImportError as exc:
        raise ImportError("NumPy integration requires the 'numpy' extra") from exc
    return np


def _dtype_for_numpy(dtype_name: str, byte_order: str):
    np = _numpy()
    aliases = {
        "uint8": "u1", "int8": "i1", "uint16": "u2", "int16": "i2",
        "uint32": "u4", "int32": "i4", "uint64": "u8", "int64": "i8",
        "float32": "f4", "float64": "f8", "bool": "b1",
    }
    try:
        base = aliases[dtype_name]
    except KeyError as exc:
        raise ValueError(f"Unsupported WX-1 dtype returned by native core: {dtype_name}") from exc
    if base in ("u1", "i1", "b1"):
        return np.dtype(base)
    return np.dtype(("<" if byte_order == "little" else ">") + base)


def _to_host_numpy(data):
    """Transfers arbitrary device arrays (JAX, DLPack, PyTorch, CuPy) or NumPy arrays to contiguous host NumPy (ADR 0033)."""
    np = _numpy()
    if isinstance(data, np.ndarray):
        return data
    # 1. JAX array detection
    if "jax" in sys.modules:
        try:
            import jax
            if isinstance(data, jax.Array):
                return np.asarray(data)
        except Exception:
            pass
    # 2. DLPack protocol
    if hasattr(data, "__dlpack__"):
        try:
            return np.from_dlpack(data)
        except Exception:
            pass
    # 3. PyTorch / CuPy / general device tensor with .cpu()
    if hasattr(data, "cpu") and callable(data.cpu):
        try:
            return np.asarray(data.cpu())
        except Exception:
            pass
    return np.asarray(data)


def _place_on_device(arr, to_device: str | None):
    """Transfers host NumPy array to requested device (e.g. 'jax', 'cpu') (ADR 0033)."""
    if to_device is None or to_device == "cpu" or to_device == "numpy":
        return arr
    if to_device == "jax":
        try:
            import jax
            return jax.numpy.asarray(arr)
        except ImportError as exc:
            raise ImportError("to_device='jax' requires the 'jax' package") from exc
    if to_device == "torch":
        try:
            import torch
            return torch.as_tensor(arr)
        except ImportError as exc:
            raise ImportError("to_device='torch' requires the 'torch' package") from exc
    raise ValueError(f"Unsupported to_device '{to_device}'; expected 'cpu', 'jax', or 'torch'")


class Wx1File:
    """Python convenience wrapper around a fully validated native WX-1 snapshot."""

    def __init__(self, native_file):
        self._native = native_file

    @property
    def dataset_id(self) -> str:
        return self._native.dataset_id

    @property
    def generation(self) -> int:
        return self._native.generation

    @property
    def attributes(self) -> dict[str, str]:
        attrs = dict(self._native.attributes())
        if not attrs:
            for v in self.variables:
                v_attrs = v.get("attributes", {})
                for k, val in v_attrs.items():
                    attrs[k] = str(val)
        return attrs

    @property
    def dimensions(self) -> dict[str, tuple[int, str]]:
        return {name: (int(length), role) for name, length, role in self._native.dimensions()}

    @property
    def variables(self) -> list[dict[str, Any]]:
        return list(self._native.variables())

    @property
    def coordinate_variables(self) -> tuple[str, ...]:
        return tuple(self._native.coordinate_variables())

    @property
    def grid_topologies(self) -> dict[str, dict[str, Any]]:
        if hasattr(self._native, "grid_topologies"):
            return dict(self._native.grid_topologies())
        return {}

    @property
    def provenance(self) -> list[dict[str, Any]]:
        if hasattr(self._native, "provenance"):
            return list(self._native.provenance())
        return []

    def plan_array_slice(self, variable_path: str, start: Sequence[int], count: Sequence[int]):
        chunk_ids, output_bytes = self._native.plan_array_slice(
            variable_path, [int(value) for value in start], [int(value) for value in count]
        )
        return {"chunk_ids": tuple(chunk_ids), "output_bytes": int(output_bytes)}

    def plan_tile_slice(self, variable_path: str, tile_index: int, *, include_halos: bool = False):
        chunk_ids, output_bytes, start, shape = self._native.plan_tile_slice(
            variable_path, int(tile_index), bool(include_halos)
        )
        return {
            "chunk_ids": tuple(chunk_ids),
            "output_bytes": int(output_bytes),
            "start": tuple(start),
            "shape": tuple(shape),
        }

    def read_tile_slice(
        self,
        variable_path: str,
        tile_index: int,
        *,
        include_halos: bool = False,
        copy: bool = False,
    ):
        np = _numpy()
        raw, dtype_name, byte_order, shape = self._native.read_tile_slice(
            variable_path, int(tile_index), bool(include_halos)
        )
        result = np.frombuffer(raw, dtype=_dtype_for_numpy(dtype_name, byte_order)).reshape(tuple(shape))
        return result.copy() if copy else result

    def read_raw_slice(self, variable_path: str, start: Sequence[int], count: Sequence[int]):
        raw, dtype_name, byte_order, shape = self._native.read_array_slice(
            variable_path, [int(value) for value in start], [int(value) for value in count]
        )
        return raw, dtype_name, byte_order, tuple(shape)

    def read_array_slice(
        self,
        variable_path: str,
        start: Sequence[int],
        count: Sequence[int],
        *,
        copy: bool = False,
        to_device: str | None = None,
    ):
        np = _numpy()
        raw, dtype_name, byte_order, shape = self.read_raw_slice(variable_path, start, count)
        result = np.frombuffer(raw, dtype=_dtype_for_numpy(dtype_name, byte_order)).reshape(shape)
        arr = result.copy() if copy else result
        return _place_on_device(arr, to_device)

    def read_points(
        self,
        variable_path: str,
        points: Sequence[Sequence[int]],
        *,
        copy: bool = False,
    ):
        """Read discrete multidimensional point coordinates with chunk-grouped batching (R8/R4)."""
        np = _numpy()
        pts = [[int(coord) for coord in pt] for pt in points]
        raw, dtype_name, byte_order, shape = self._native.read_point_coordinates(variable_path, pts)
        result = np.frombuffer(raw, dtype=_dtype_for_numpy(dtype_name, byte_order)).reshape(tuple(shape))
        return result.copy() if copy else result

    def read_array(self, variable_path: str, *, copy: bool = False, to_device: str | None = None):
        clean = variable_path.lstrip("/")
        variable = next((item for item in self.variables if item["path"] == variable_path or item["path"].lstrip("/") == clean), None)
        if variable is None:
            raise KeyError(variable_path)
        actual_path = variable["path"]
        shape = list(variable["shape"])
        np = _numpy()
        dtype = _dtype_for_numpy(variable["dtype"], variable["byte_order"])
        total_bytes = int(np.prod(shape)) * dtype.itemsize if shape else dtype.itemsize
        max_slice_bytes = 64 * 1024 * 1024
        if total_bytes <= max_slice_bytes or len(shape) == 0:
            return self.read_array_slice(
                actual_path,
                [0] * len(shape),
                shape,
                copy=copy,
                to_device=to_device,
            )
        # Bounded batched read along axis 0 when total volume > 64 MiB
        slice_bytes = int(np.prod(shape[1:])) * dtype.itemsize
        step = max(1, max_slice_bytes // max(1, slice_bytes))
        out = np.empty(shape, dtype=dtype)
        for start0 in range(0, shape[0], step):
            cnt0 = min(step, shape[0] - start0)
            out[start0:start0 + cnt0] = self.read_array_slice(
                actual_path,
                [start0] + [0] * (len(shape) - 1),
                [cnt0] + shape[1:],
                copy=False,
            )
        return _place_on_device(out, to_device)

    def close(self) -> None:
        # The native snapshot retains metadata/descriptors only; every read opens its own file.
        self._native.close()

    def validate(self) -> None:
        """Perform exhaustive full verification of all chunk checksums and generation hashes."""
        self._native.validate()

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, traceback):
        self.close()
        return False


def open_file(path: str | Path) -> Wx1File:
    """Open a scientific WX-1 file, fully validating its current generation."""
    native = _require_core().open_file(str(Path(path)))
    return Wx1File(native)


def write_array(
    path: str | Path,
    data,
    *,
    dataset_id: str,
    variable_path: str,
    dimensions: Sequence[str],
    chunk_shape: Sequence[int] | None = None,
    dimension_roles: Sequence[str] | None = None,
    attributes: Mapping[str, str] | None = None,
    profile: str = "restart",
    lossy_error_bound: float | None = None,
    lossy_metric: str = "max_abs_error",
    codec: str = "zstd",
    workers: int = 1,
    durability: str = "durable",
    grid_family: str | None = None,
    c_resolution: int | None = None,
    halo_width: int | None = None,
    stagger_horizontal: str | None = None,
    stagger_vertical: str | None = None,
    tile_dimension: str | None = None,
) -> None:
    """Write one root-level variable to a new WX-1 file.

    Parameters
    ----------
    profile : str, default "restart"
        Operational profile: "restart" (strictly lossless and bit-exact) or
        "output" (error-bounded lossy quantization permitted when declared).
    lossy_error_bound : float or None, default None
        Maximum declared error bound (e.g. 0.01 for +/- 0.01 K).
        Strictly forbidden in "restart" profile.
    lossy_metric : str, default "max_abs_error"
        Error metric: "max_abs_error" (absolute bound) or "relative_error" (relative bound).
    codec : str, default "zstd"
        Chunk compression codec: "zstd", "zstd_shuffle" (ByteShuffle + Zstd), "lz4", or "raw".
    workers : int, default 1
        Number of parallel chunk-staging threads (ADR 0025).
    durability : str, default "durable"
        Write durability mode: "durable" (explicit fsync before rename/return)
        or "buffered" (OS page cache buffered commit).
    """
    if profile == "restart" and lossy_error_bound is not None:
        raise ValueError("lossy quantization is strictly forbidden in restart profile")
    np = _numpy()
    array = _to_host_numpy(data)
    dtype_names = {
        ("u", 1): "uint8", ("i", 1): "int8",
        ("u", 2): "uint16", ("i", 2): "int16",
        ("u", 4): "uint32", ("i", 4): "int32",
        ("u", 8): "uint64", ("i", 8): "int64",
        ("f", 4): "float32", ("f", 8): "float64",
        ("b", 1): "bool",
    }
    try:
        native_dtype_name = dtype_names[(array.dtype.kind, array.dtype.itemsize)]
    except KeyError as exc:
        raise TypeError(f"Unsupported WX-1 NumPy dtype: {array.dtype}") from exc
    array = np.ascontiguousarray(array)
    byte_order = array.dtype.byteorder
    if byte_order == "=" or byte_order == "|":
        byte_order_name = sys.byteorder if array.dtype.itemsize > 1 else "little"
    else:
        byte_order_name = "little" if byte_order == "<" else "big"
    chunk = tuple(int(value) for value in (chunk_shape or array.shape))
    roles = list(dimension_roles) if dimension_roles is not None else ["other"] * array.ndim
    attrs = {str(key): str(value) for key, value in (attributes or {}).items()}
    _require_core().write_array(
        str(Path(path)),
        str(dataset_id),
        str(variable_path),
        [str(value) for value in dimensions],
        [int(value) for value in array.shape],
        list(chunk),
        native_dtype_name,
        byte_order_name,
        array.tobytes(order="C"),
        roles,
        attrs,
        str(profile),
        lossy_error_bound,
        str(lossy_metric),
        str(codec),
        int(workers),
        str(durability),
        str(grid_family) if grid_family is not None else None,
        int(c_resolution) if c_resolution is not None else None,
        int(halo_width) if halo_width is not None else None,
        str(stagger_horizontal) if stagger_horizontal is not None else None,
        str(stagger_vertical) if stagger_vertical is not None else None,
        str(tile_dimension) if tile_dimension is not None else None,
    )


class AsyncWriter:
    """Bounded asynchronous WX-1 writer for NWP timestep loops (ADR 0025)."""

    def __init__(
        self,
        path: str | Path,
        *,
        dataset_id: str,
        variable_path: str,
        spatial_dimensions: Sequence[str],
        spatial_shape: Sequence[int],
        spatial_chunk_shape: Sequence[int] | None = None,
        dtype: str = "float32",
        byte_order: str = "little",
        codec: str = "zstd_shuffle",
        workers: int = 4,
        max_pending_generations: int = 2,
        max_queued_bytes: int = 268435456,
        durability: str = "durable",
    ):
        cshape = list(spatial_chunk_shape) if spatial_chunk_shape is not None else list(spatial_shape)
        self._native = _require_core().AsyncWriter(
            str(Path(path)),
            str(dataset_id),
            str(variable_path),
            [str(d) for d in spatial_dimensions],
            [int(s) for s in spatial_shape],
            [int(c) for c in cshape],
            str(dtype),
            str(byte_order),
            str(codec),
            int(workers),
            int(max_pending_generations),
            int(max_queued_bytes),
            str(durability),
        )

    def submit_step(self, data) -> None:
        """Stage and enqueue one NWP timestep array into the bounded background writer (ADR 0025, ADR 0033)."""
        np = _numpy()
        host_arr = _to_host_numpy(data)
        arr = np.ascontiguousarray(host_arr)
        self._native.submit_step(arr.tobytes(order="C"))

    def flush(self) -> dict[str, int]:
        return dict(self._native.flush())

    def telemetry(self) -> dict[str, int]:
        return dict(self._native.telemetry())

    def close(self) -> dict[str, int]:
        return dict(self._native.close())

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, traceback):
        try:
            self.close()
        except Exception:
            pass
        return False


def write_restart_checkpoint(
    path: str | Path,
    state: Any,
    *,
    dataset_id: str = "restart_checkpoint",
    dimensions: Sequence[str] | Mapping[str, Sequence[str]] | None = None,
    iteration: int | None = None,
    sim_time_seconds: float | None = None,
    prng_state: Any | None = None,
    shard_info: Mapping[str, Any] | None = None,
    codec: str = "zstd_shuffle",
    durability: str = "durable",
    workers: int = 4,
) -> None:
    """Writes a 100% bit-exact NWP model restart checkpoint (ADR 0033).

    Guarantees:
    - Zero lossy quantization (profile is strictly 'restart').
    - Auxiliary restart metadata: simulation iteration, timestamp, PRNG seed state.
    - Shard partitioning attributes for multi-device/multi-rank runs.
    """
    path = Path(path)
    meta_attrs = {
        "wx1:profile": "restart",
        "wx1:dataset_type": "nwp_checkpoint",
    }
    if iteration is not None:
        meta_attrs["restart:iteration"] = str(int(iteration))
    if sim_time_seconds is not None:
        meta_attrs["restart:sim_time_seconds"] = str(float(sim_time_seconds))
    if prng_state is not None:
        meta_attrs["restart:prng_state"] = str(prng_state)
    if shard_info is not None:
        for k, v in shard_info.items():
            meta_attrs[f"shard:{k}"] = str(v)

    if isinstance(state, Mapping):
        if len(state) == 1:
            var_name, var_arr = next(iter(state.items()))
            host_arr = _to_host_numpy(var_arr)
            dims = dimensions if isinstance(dimensions, Sequence) and not isinstance(dimensions, (str, bytes)) else [f"dim_{i}" for i in range(host_arr.ndim)]
            write_array(
                path,
                dataset_id=dataset_id,
                variable_path=var_name,
                dimensions=dims,
                array=host_arr,
                profile="restart",
                codec=codec,
                durability=durability,
                workers=workers,
                attributes=meta_attrs,
            )
        else:
            path.mkdir(parents=True, exist_ok=True)
            import json
            manifest = {
                "dataset_id": dataset_id,
                "iteration": iteration,
                "sim_time_seconds": sim_time_seconds,
                "prng_state": prng_state,
                "shard_info": dict(shard_info) if shard_info else None,
                "variables": list(state.keys()),
            }
            with open(path / "checkpoint_manifest.json", "w", encoding="utf-8") as f:
                json.dump(manifest, f, indent=2)

            for var_name, var_arr in state.items():
                var_path = path / f"{var_name}.wx1"
                host_arr = _to_host_numpy(var_arr)
                var_dims = None
                if isinstance(dimensions, Mapping):
                    var_dims = dimensions.get(var_name)
                elif isinstance(dimensions, Sequence):
                    var_dims = dimensions
                if var_dims is None:
                    var_dims = [f"dim_{i}" for i in range(getattr(host_arr, "ndim", 2))]
                write_array(
                    var_path,
                    dataset_id=f"{dataset_id}_{var_name}",
                    variable_path=var_name,
                    dimensions=var_dims,
                    data=host_arr,
                    profile="restart",
                    codec=codec,
                    durability=durability,
                    workers=workers,
                    attributes=meta_attrs,
                )
    else:
        host_arr = _to_host_numpy(state)
        dims = dimensions if isinstance(dimensions, Sequence) and not isinstance(dimensions, (str, bytes)) else [f"dim_{i}" for i in range(getattr(host_arr, "ndim", 2))]
        write_array(
            path,
            dataset_id=dataset_id,
            variable_path="state",
            dimensions=dims,
            data=host_arr,
            profile="restart",
            codec=codec,
            durability=durability,
            workers=workers,
            attributes=meta_attrs,
        )


def read_restart_checkpoint(
    path: str | Path,
    *,
    to_device: str | None = None,
) -> dict[str, Any]:
    """Reads a bit-exact NWP model restart checkpoint and returns restored variables and metadata (ADR 0033)."""
    path = Path(path)
    if path.is_file():
        f = open_file(path)
        vars_dict = {}
        for v in f.variables:
            vars_dict[v["name"]] = f.read_array(v["path"], to_device=to_device)
        attrs = f.attributes
        iteration = int(attrs["restart:iteration"]) if "restart:iteration" in attrs else None
        sim_time = float(attrs["restart:sim_time_seconds"]) if "restart:sim_time_seconds" in attrs else None
        prng_state = attrs.get("restart:prng_state")
        shard_info = {k[6:]: v for k, v in attrs.items() if k.startswith("shard:")}
        return {
            "variables": vars_dict,
            "iteration": iteration,
            "sim_time_seconds": sim_time,
            "prng_state": prng_state,
            "shard_info": shard_info if shard_info else None,
        }
    elif path.is_dir():
        import json
        manifest_path = path / "checkpoint_manifest.json"
        manifest = {}
        if manifest_path.exists():
            with open(manifest_path, "r", encoding="utf-8") as mf:
                manifest = json.load(mf)
        vars_dict = {}
        for wx1_file in sorted(path.glob("*.wx1")):
            f = open_file(wx1_file)
            for v in f.variables:
                vars_dict[v["name"]] = f.read_array(v["path"], to_device=to_device)
        return {
            "variables": vars_dict,
            "iteration": manifest.get("iteration"),
            "sim_time_seconds": manifest.get("sim_time_seconds"),
            "prng_state": manifest.get("prng_state"),
            "shard_info": manifest.get("shard_info"),
        }
    else:
        raise FileNotFoundError(f"Checkpoint path '{path}' does not exist")


def open_dataset(path: str | Path, **kwargs):
    """Open with the xarray WX-1 backend. Requires the ``xarray`` extra."""
    try:
        import xarray as xr
        from .xarray_backend import Wx1BackendEntrypoint
    except ImportError as exc:
        raise ImportError("xarray integration requires the 'xarray' extra") from exc
    engine = kwargs.pop("engine", None)
    if engine is None:
        engine = Wx1BackendEntrypoint
    return xr.open_dataset(path, engine=engine, **kwargs)


def unit_dimension(unit: str) -> str | None:
    """Returns the physical dimension name for a known unit (e.g. 'temperature', 'pressure')."""
    return _require_core().unit_dimension(str(unit))


def unit_conversion_factor(from_unit: str, to_unit: str) -> tuple[float, float]:
    """Returns (scale, offset) for linear conversion: target = (source * scale) + offset."""
    return tuple(_require_core().unit_conversion_factor(str(from_unit), str(to_unit)))


def resolve_cf_standard_name(name: str) -> dict[str, str] | None:
    """Resolves a CF standard_name to its canonical parameter identity and physical units."""
    res = _require_core().resolve_cf_standard_name(str(name))
    return dict(res) if res is not None else None


def from_grib(*args, **kwargs):
    """Import a WMO GRIB2 file into a WX-1 container (ADR 0030)."""
    from .adapters.grib2 import from_grib as _impl
    return _impl(*args, **kwargs)


def to_grib(*args, **kwargs):
    """Export a WX-1 container back to a WMO GRIB2 file (ADR 0030)."""
    from .adapters.grib2 import to_grib as _impl
    return _impl(*args, **kwargs)


def from_netcdf(*args, **kwargs):
    """Import a NetCDF-4 dataset into a WX-1 container (ADR 0030)."""
    from .adapters.netcdf import from_netcdf as _impl
    return _impl(*args, **kwargs)


def to_netcdf(*args, **kwargs):
    """Export a WX-1 container back to a NetCDF-4 file (ADR 0030)."""
    from .adapters.netcdf import to_netcdf as _impl
    return _impl(*args, **kwargs)


def from_zarr(*args, **kwargs):
    """Import a Zarr array or group into a WX-1 container (ADR 0030)."""
    from .adapters.zarr import from_zarr as _impl
    return _impl(*args, **kwargs)


def to_zarr(*args, **kwargs):
    """Export a WX-1 container back to a Zarr store (ADR 0030)."""
    from .adapters.zarr import to_zarr as _impl
    return _impl(*args, **kwargs)


def from_bufr(*args, **kwargs):
    """Import a WMO FM 94 BUFR file into a WX-1 container (ADR 0030)."""
    from .adapters.bufr_crex import from_bufr as _impl
    return _impl(*args, **kwargs)


def to_bufr(*args, **kwargs):
    """Export a WX-1 container to a WMO FM 94 BUFR file (ADR 0030)."""
    from .adapters.bufr_crex import to_bufr as _impl
    return _impl(*args, **kwargs)


def from_crex(*args, **kwargs):
    """Import a WMO FM 95 CREX file into a WX-1 container (ADR 0030)."""
    from .adapters.bufr_crex import from_crex as _impl
    return _impl(*args, **kwargs)


def to_crex(*args, **kwargs):
    """Export a WX-1 container to a WMO FM 95 CREX file (ADR 0030)."""
    from .adapters.bufr_crex import to_crex as _impl
    return _impl(*args, **kwargs)


def from_hdf5(*args, **kwargs):
    """Import an HDF5 (.h5 / .hdf5) file into a WX-1 container (ADR 0030)."""
    from .adapters.hdf_geotiff import from_hdf5 as _impl
    return _impl(*args, **kwargs)


def to_hdf5(*args, **kwargs):
    """Export a WX-1 container to an HDF5 (.h5 / .hdf5) file (ADR 0030)."""
    from .adapters.hdf_geotiff import to_hdf5 as _impl
    return _impl(*args, **kwargs)


def from_odim_h5(*args, **kwargs):
    """Import an EUMETNET OPERA ODIM_H5 radar file into a WX-1 container (ADR 0030)."""
    from .adapters.hdf_geotiff import from_odim_h5 as _impl
    return _impl(*args, **kwargs)


def to_odim_h5(*args, **kwargs):
    """Export a WX-1 container to an EUMETNET OPERA ODIM_H5 radar file (ADR 0030)."""
    from .adapters.hdf_geotiff import to_odim_h5 as _impl
    return _impl(*args, **kwargs)


def from_hdf4(*args, **kwargs):
    """Import an HDF4 (.hdf) file into a WX-1 container (ADR 0030)."""
    from .adapters.hdf_geotiff import from_hdf4 as _impl
    return _impl(*args, **kwargs)


def to_hdf4(*args, **kwargs):
    """Export a WX-1 container to an HDF4 (.hdf) file (ADR 0030)."""
    from .adapters.hdf_geotiff import to_hdf4 as _impl
    return _impl(*args, **kwargs)


def from_geotiff(*args, **kwargs):
    """Import a GeoTIFF (.tif) file into a WX-1 container (ADR 0030)."""
    from .adapters.hdf_geotiff import from_geotiff as _impl
    return _impl(*args, **kwargs)


def to_geotiff(*args, **kwargs):
    """Export a WX-1 container to a GeoTIFF (.tif) file (ADR 0030)."""
    from .adapters.hdf_geotiff import to_geotiff as _impl
    return _impl(*args, **kwargs)


def from_pp(*args, **kwargs):
    """Import a UK Met Office PP (.pp) file into a WX-1 container (ADR 0030)."""
    from .adapters.ukmo_radar import from_pp as _impl
    return _impl(*args, **kwargs)


def to_pp(*args, **kwargs):
    """Export a WX-1 container to a UK Met Office PP (.pp) file (ADR 0030)."""
    from .adapters.ukmo_radar import to_pp as _impl
    return _impl(*args, **kwargs)


def from_nimrod(*args, **kwargs):
    """Import a UK Met Office NIMROD (.nimrod) file into a WX-1 container (ADR 0030)."""
    from .adapters.ukmo_radar import from_nimrod as _impl
    return _impl(*args, **kwargs)


def to_nimrod(*args, **kwargs):
    """Export a WX-1 container to a UK Met Office NIMROD (.nimrod) file (ADR 0030)."""
    from .adapters.ukmo_radar import to_nimrod as _impl
    return _impl(*args, **kwargs)


def from_dorade(*args, **kwargs):
    """Import an NCAR DORADE (.dorade) sweepfile into a WX-1 container (ADR 0030)."""
    from .adapters.ukmo_radar import from_dorade as _impl
    return _impl(*args, **kwargs)


def to_dorade(*args, **kwargs):
    """Export a WX-1 container to an NCAR DORADE (.dorade) sweepfile (ADR 0030)."""
    from .adapters.ukmo_radar import to_dorade as _impl
    return _impl(*args, **kwargs)


def from_iwrf(*args, **kwargs):
    """Import an NCAR IWRF (.iwrf) packet stream into a WX-1 container (ADR 0030)."""
    from .adapters.ukmo_radar import from_iwrf as _impl
    return _impl(*args, **kwargs)


def to_iwrf(*args, **kwargs):
    """Export a WX-1 container to an NCAR IWRF (.iwrf) packet stream (ADR 0030)."""
    from .adapters.ukmo_radar import to_iwrf as _impl
    return _impl(*args, **kwargs)


def convert(*args, **kwargs):
    """Convert scientific datasets between GRIB2, NetCDF, Zarr, and WX-1 (ADR 0030)."""
    from .cli import convert as _impl
    return _impl(*args, **kwargs)


