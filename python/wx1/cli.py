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

"""Command-line interface for WX-1 scientific container and format conversions (ADR 0030)."""

from __future__ import annotations

import argparse
import sys
import tempfile
from pathlib import Path
from typing import Sequence


def detect_format(path: str | Path) -> str:
    p = Path(path)
    name_lower = p.name.lower()
    suffix = p.suffix.lower()
    if name_lower.endswith(".odim.h5") or name_lower.endswith(".odim.hdf5"):
        return "odim_h5"
    if suffix in (".wx1",):
        return "wx1"
    if suffix in (".grib", ".grib2", ".grb", ".grb2"):
        return "grib2"
    if suffix in (".nc", ".nc4", ".netcdf"):
        return "netcdf4"
    if suffix in (".cdf",):
        return "cdf"
    if suffix in (".zarr",) or (p.is_dir() and ((p / ".zgroup").exists() or (p / "zarr.json").exists())):
        return "zarr"
    if suffix in (".bufr", ".bfr"):
        return "bufr"
    if suffix in (".crex",):
        return "crex"
    if suffix in (".h5", ".hdf5"):
        return "hdf5"
    if suffix in (".hdf",):
        return "hdf4"
    if suffix in (".tif", ".tiff", ".geotiff"):
        return "geotiff"
    if suffix in (".pp",):
        return "pp"
    if suffix in (".nimrod", ".nim"):
        return "nimrod"
    if suffix in (".dorade", ".swp"):
        return "dorade"
    if suffix in (".iwrf",):
        return "iwrf"
    raise ValueError(f"Unable to auto-detect format for path: {path} (suffix: {suffix})")


def convert(
    input_path: str | Path,
    output_path: str | Path,
    *,
    input_format: str | None = None,
    output_format: str | None = None,
    variable: str | None = None,
    codec: str = "zstd_shuffle",
    workers: int = 1,
    profile: str = "restart",
    lossy_error: float | None = None,
    durability: str = "durable",
):
    """Convert scientific datasets between meteorological/scientific formats and WX-1 (ADR 0030)."""
    from .adapters import bufr_crex, grib2, hdf_geotiff, netcdf, ukmo_radar, zarr

    src_fmt = input_format or detect_format(input_path)
    dst_fmt = output_format or detect_format(output_path)

    in_p = Path(input_path)
    out_p = Path(output_path)

    # 1. Foreign -> WX-1
    if dst_fmt == "wx1":
        if src_fmt == "grib2":
            return grib2.from_grib(
                in_p, out_p, variable_name=variable, codec=codec, workers=workers,
                profile=profile, lossy_error_bound=lossy_error, durability=durability,
            )
        elif src_fmt in ("netcdf4", "cdf"):
            return netcdf.from_netcdf(
                in_p, out_p, variable_name=variable, codec=codec, workers=workers,
                profile=profile, lossy_error_bound=lossy_error, durability=durability,
            )
        elif src_fmt == "zarr":
            return zarr.from_zarr(
                in_p, out_p, variable_name=variable, codec=codec, workers=workers,
                profile=profile, lossy_error_bound=lossy_error, durability=durability,
            )
        elif src_fmt == "bufr":
            return bufr_crex.from_bufr(in_p, out_p, codec=codec, workers=workers, profile=profile, durability=durability)
        elif src_fmt == "crex":
            return bufr_crex.from_crex(in_p, out_p, codec=codec, workers=workers, profile=profile, durability=durability)
        elif src_fmt == "hdf5":
            return hdf_geotiff.from_hdf5(
                in_p, out_p, dataset_name=variable, codec=codec, workers=workers, profile=profile, durability=durability,
            )
        elif src_fmt == "odim_h5":
            return hdf_geotiff.from_odim_h5(in_p, out_p, codec=codec, workers=workers, profile=profile, durability=durability)
        elif src_fmt == "hdf4":
            return hdf_geotiff.from_hdf4(
                in_p, out_p, dataset_name=variable, codec=codec, workers=workers, profile=profile, durability=durability,
            )
        elif src_fmt == "geotiff":
            return hdf_geotiff.from_geotiff(in_p, out_p, codec=codec, workers=workers, profile=profile, durability=durability)
        elif src_fmt == "pp":
            return ukmo_radar.from_pp(in_p, out_p, codec=codec, workers=workers, profile=profile, durability=durability)
        elif src_fmt == "nimrod":
            return ukmo_radar.from_nimrod(in_p, out_p, codec=codec, workers=workers, profile=profile, durability=durability)
        elif src_fmt == "dorade":
            return ukmo_radar.from_dorade(in_p, out_p, codec=codec, workers=workers, profile=profile, durability=durability)
        elif src_fmt == "iwrf":
            return ukmo_radar.from_iwrf(in_p, out_p, codec=codec, workers=workers, profile=profile, durability=durability)
        elif src_fmt == "wx1":
            import shutil
            shutil.copyfile(in_p, out_p)
            import wx1
            return wx1.open_file(out_p)

    # 2. WX-1 -> Foreign
    if src_fmt == "wx1":
        var_path = f"/{variable.lstrip('/')}" if variable else None
        if dst_fmt == "grib2":
            return grib2.to_grib(in_p, out_p, variable_path=var_path)
        elif dst_fmt == "netcdf4":
            return netcdf.to_netcdf(in_p, out_p, variable_path=var_path, format="NETCDF4")
        elif dst_fmt == "cdf":
            return netcdf.to_netcdf(in_p, out_p, variable_path=var_path, format="NETCDF3_64BIT_OFFSET", zlib=False)
        elif dst_fmt == "zarr":
            return zarr.to_zarr(in_p, out_p, variable_path=var_path)
        elif dst_fmt == "bufr":
            return bufr_crex.to_bufr(in_p, out_p, variable_path=var_path)
        elif dst_fmt == "crex":
            return bufr_crex.to_crex(in_p, out_p, variable_path=var_path)
        elif dst_fmt == "hdf5":
            return hdf_geotiff.to_hdf5(in_p, out_p, variable_path=var_path)
        elif dst_fmt == "odim_h5":
            return hdf_geotiff.to_odim_h5(in_p, out_p, variable_path=var_path)
        elif dst_fmt == "hdf4":
            return hdf_geotiff.to_hdf4(in_p, out_p, variable_path=var_path)
        elif dst_fmt == "geotiff":
            return hdf_geotiff.to_geotiff(in_p, out_p, variable_path=var_path)
        elif dst_fmt == "pp":
            return ukmo_radar.to_pp(in_p, out_p, variable_path=var_path)
        elif dst_fmt == "nimrod":
            return ukmo_radar.to_nimrod(in_p, out_p, variable_path=var_path)
        elif dst_fmt == "dorade":
            return ukmo_radar.to_dorade(in_p, out_p, variable_path=var_path)
        elif dst_fmt == "iwrf":
            return ukmo_radar.to_iwrf(in_p, out_p, variable_path=var_path)

    # 3. Foreign -> Foreign (via temporary WX-1 container)
    with tempfile.TemporaryDirectory() as td:
        tmp_wx1 = Path(td) / "intermediate.wx1"
        convert(
            in_p,
            tmp_wx1,
            input_format=src_fmt,
            output_format="wx1",
            variable=variable,
            codec=codec,
            workers=workers,
            profile=profile,
            lossy_error=lossy_error,
            durability=durability,
        )
        return convert(
            tmp_wx1,
            out_p,
            input_format="wx1",
            output_format=dst_fmt,
            variable=variable,
        )


def cmd_info(args: argparse.Namespace) -> int:
    import wx1

    path = Path(args.file)
    fmt = detect_format(path)
    print(f"File: {path}")
    print(f"Detected format: {fmt}")

    if fmt == "wx1":
        with wx1.open_file(path) as f:
            print(f"Dataset ID:  {f.dataset_id}")
            print(f"Generation:  {f.generation}")
            print(f"Dimensions:  {f.dimensions}")
            print(f"Variables:   {len(f.variables)}")
            for v in f.variables:
                print(f"  - {v['path']}: shape={v['shape']}, dtype={v['dtype']}, chunks={v['chunk_shape']}")
            print(f"Attributes:  ({len(f.attributes)} total)")
            for k, val in sorted(f.attributes.items()):
                print(f"    {k}: {val}")
    else:
        print(f"Use format-specific inspection tools or convert to .wx1 to inspect metadata.")
    return 0


def cmd_convert(args: argparse.Namespace) -> int:
    try:
        print(f"Converting '{args.input}' -> '{args.output}'...")
        res = convert(
            args.input,
            args.output,
            input_format=args.from_format,
            output_format=args.to_format,
            variable=args.variable,
            codec=args.codec,
            workers=args.workers,
            profile=args.profile,
            lossy_error=args.lossy_error,
            durability=args.durability,
        )
        print(f"Successfully converted to {args.output}")
        return 0
    except Exception as exc:
        print(f"Error during conversion: {exc}", file=sys.stderr)
        return 1


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="wx1", description="WX-1 Scientific Container CLI")
    subparsers = parser.add_subparsers(dest="command", required=True)

    ALL_FORMATS = [
        "grib2", "netcdf4", "cdf", "zarr", "wx1",
        "bufr", "crex", "hdf5", "odim_h5", "hdf4",
        "geotiff", "pp", "nimrod", "dorade", "iwrf",
    ]
    # convert
    p_conv = subparsers.add_parser("convert", help="Convert datasets between meteorological/scientific formats and WX-1")
    p_conv.add_argument("input", help="Source file path")
    p_conv.add_argument("output", help="Destination file path")
    p_conv.add_argument("--from-format", choices=ALL_FORMATS, help="Explicit source format")
    p_conv.add_argument("--to-format", choices=ALL_FORMATS, help="Explicit target format")
    p_conv.add_argument("--variable", "-v", help="Variable name to extract/convert")
    p_conv.add_argument("--codec", default="zstd_shuffle", help="Chunk codec for WX-1 (zstd_shuffle, zstd, lz4, raw)")
    p_conv.add_argument("--workers", "-w", type=int, default=1, help="Parallel worker threads")
    p_conv.add_argument("--profile", default="restart", choices=["restart", "output"], help="Operational profile")
    p_conv.add_argument("--lossy-error", type=float, default=None, help="Lossy error bound")
    p_conv.add_argument("--durability", default="durable", choices=["durable", "buffered"], help="Durability mode")
    p_conv.set_defaults(func=cmd_convert)

    # info
    p_info = subparsers.add_parser("info", help="Inspect WX-1 container metadata and variables")
    p_info.add_argument("file", help="Path to scientific file")
    p_info.set_defaults(func=cmd_info)

    parsed = parser.parse_args(argv)
    return parsed.func(parsed)


if __name__ == "__main__":
    sys.exit(main())
