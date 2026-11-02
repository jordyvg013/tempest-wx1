"""Create deterministic local fixtures for optional external-format adapter tests."""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path

import numpy as np
import xarray as xr


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--zarr", action="store_true", help="also create a local Zarr v2 fixture")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    dataset = xr.Dataset(
        {
            "temperature": (("time", "y", "x"), np.arange(24, dtype=np.float32).reshape(2, 3, 4)),
            "wind_speed": (("time", "y", "x"), np.full((2, 3, 4), 8.5, dtype=np.float32)),
        },
        coords={"time": np.array([0, 1], dtype=np.int32), "y": np.arange(3), "x": np.arange(4)},
        attrs={"institution": "WX-1 deterministic fixture", "fixture_version": "1"},
    )
    dataset["temperature"].attrs.update({"units": "K", "standard_name": "air_temperature"})
    dataset["wind_speed"].attrs.update({"units": "m s-1", "standard_name": "wind_speed"})

    netcdf_path = args.output / "wx1-synthetic.nc"
    dataset.to_netcdf(netcdf_path, engine="scipy")
    manifest = [f"{sha256(netcdf_path)}  {netcdf_path.name}"]

    if args.zarr:
        import zarr  # noqa: F401
        zarr_path = args.output / "wx1-synthetic.zarr"
        try:
            dataset.to_zarr(zarr_path, mode="w", consolidated=False, zarr_format=2)
        except TypeError:
            dataset.to_zarr(zarr_path, mode="w", consolidated=False, zarr_version=2)
        manifest.append(f"{sha256(zarr_path / '.zgroup')}  {zarr_path.name}/.zgroup")

    (args.output / "SHA256SUMS.txt").write_text("\n".join(manifest) + "\n", encoding="utf-8")
    print("Created local fixtures:")
    for line in manifest:
        print(line)


if __name__ == "__main__":
    main()
