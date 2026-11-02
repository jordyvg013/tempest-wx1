"""Generate deterministic offline synthetic regular lat/lon F32 fixture (31 x 16)."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import numpy as np
import xarray as xr


def make_synthetic_dataset() -> xr.Dataset:
    lats = np.linspace(90.0, -90.0, 31, dtype=np.float32)
    lons = np.linspace(0.0, 337.5, 16, dtype=np.float32)
    flat_indices = np.arange(31 * 16, dtype=np.float32)
    temp_values = (273.15 + flat_indices * 0.125).reshape(31, 16).astype("<f4")

    ds = xr.Dataset(
        data_vars={
            "temperature": (["latitude", "longitude"], temp_values, {"units": "K", "standard_name": "air_temperature"})
        },
        coords={
            "latitude": ("latitude", lats, {"units": "degrees_north", "standard_name": "latitude"}),
            "longitude": ("longitude", lons, {"units": "degrees_east", "standard_name": "longitude"}),
        },
        attrs={
            "title": "WX-1 Phase-2 Synthetic Step-1 Fixture",
            "institution": "WX-1 Development Suite",
            "conventions": "CF-1.8",
        },
    )
    return ds


def generate_fixture(output_dir: str | Path | None = None) -> Path:
    if output_dir is None:
        output_dir = os.environ.get("WX1_FIXTURE_DIR", "/tmp/wx1-cross-format-fixtures")
    out_path = Path(output_dir)
    out_path.mkdir(parents=True, exist_ok=True)

    ds = make_synthetic_dataset()

    lat_raw = ds["latitude"].values.tobytes()
    lon_raw = ds["longitude"].values.tobytes()
    temp_raw = ds["temperature"].values.tobytes()

    (out_path / "latitude.raw").write_bytes(lat_raw)
    (out_path / "longitude.raw").write_bytes(lon_raw)
    (out_path / "temperature.raw").write_bytes(temp_raw)

    manifest = {
        "dataset_id": "wx1-phase2-synthetic-31x16",
        "shape": [31, 16],
        "dimensions": ["latitude", "longitude"],
        "dtype": "<f4",
        "variable": "temperature",
        "units": "K",
        "sha256": {
            "latitude.raw": hashlib.sha256(lat_raw).hexdigest(),
            "longitude.raw": hashlib.sha256(lon_raw).hexdigest(),
            "temperature.raw": hashlib.sha256(temp_raw).hexdigest(),
        },
    }
    (out_path / "manifest.json").write_text(json.dumps(manifest, indent=2))

    sha_lines = [
        f"{manifest['sha256']['latitude.raw']}  latitude.raw",
        f"{manifest['sha256']['longitude.raw']}  longitude.raw",
        f"{manifest['sha256']['temperature.raw']}  temperature.raw",
    ]
    (out_path / "SHA256SUMS.txt").write_text("\n".join(sha_lines) + "\n")
    print(f"Synthetic fixture written to: {out_path}")
    return out_path


if __name__ == "__main__":
    generate_fixture()
