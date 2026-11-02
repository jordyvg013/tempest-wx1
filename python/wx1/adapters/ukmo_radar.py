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

"""Bidirectional adapters for UK Met Office PP (.pp), NIMROD (.nimrod), NCAR DORADE (.dorade), and NCAR IWRF (.iwrf) (ADR 0030).

Implements exact binary wire layouts for:
1. UK Met Office PP Format (`.pp`):
   - Fortran unformatted sequential records (`[256B][64-word big-endian lookup header: 45x int32 + 19x float32][256B]`
     followed by `[N*4B][big-endian IEEE-754 float32 payload][N*4B]`).
2. UK Met Office NIMROD Format (`.nimrod`):
   - Fortran unformatted record containing the standard 512-byte big-endian header
     (`31x int16` (62B) + `28x float32` (112B) + `45x float32` (180B) + `56B char` + `51x int16` (102B) = 512B)
     followed by `[N*B][big-endian grid data][N*B]`.
3. NCAR DORADE Sweepfile Format (`.dorade`):
   - Standard 4-byte ASCII block descriptors (`SSWB`, `VOLD`, `RADD`, `PARM`, `CELV`, `SWIB`,
     and per-ray `RYIB`, `ASIB`, `RDAT` blocks with big-endian IEEE float32 gate values).
4. NCAR IWRF (Integrated Weather Radar Format, `.iwrf`):
   - Standard packet stream with `iwrf_packet_info_t` headers (`id`, `len_bytes`, `seq_num`, `version_num`,
     `radar_id`, `time_secs_utc`, `time_nano_secs`), including `IWRF_RADAR_INFO_ID (0x77770001)`,
     `IWRF_SCAN_SEGMENT_ID (0x77770002)`, and `IWRF_PULSE_HEADER_ID (0x7777000c)` ray/pulse packets.
"""

from __future__ import annotations

import datetime
import struct
from pathlib import Path
from typing import Any, Sequence


def _resolve_safe_chunks(shape: Sequence[int], elem_size: int, chunk_shape: Sequence[int] | None = None) -> list[int]:
    import numpy as np

    resolved = list(chunk_shape) if chunk_shape is not None else list(shape)
    max_chunk_bytes = 32 * 1024 * 1024
    for idx in range(len(resolved)):
        while resolved[idx] > 1 and int(np.prod(resolved)) * max(1, elem_size) > max_chunk_bytes:
            resolved[idx] = max(1, resolved[idx] // 2)
    return resolved


# ---------------------------------------------------------------------------
# 1. UK Met Office PP Format (.pp)
# ---------------------------------------------------------------------------
def to_pp(
    input_wx1_path: str | Path,
    output_pp_path: str | Path,
    *,
    variable_path: str | None = None,
) -> Path:
    """Export a WX-1 2D/3D container to a UK Met Office PP (.pp) binary file."""
    import numpy as np
    import wx1

    in_file = Path(input_wx1_path)
    out_file = Path(output_pp_path)

    with wx1.open_file(in_file) as f:
        var_info = f.variables[0] if variable_path is None else next(
            v for v in f.variables if v["path"] == variable_path
        )
        data = np.asarray(f.read_array(var_info["path"]), dtype=np.float32)
        attrs = f.attributes

    slices = [data] if data.ndim == 2 else [data[i] for i in range(data.shape[0])]
    out_file.parent.mkdir(parents=True, exist_ok=True)

    with open(out_file, "wb") as fp:
        for idx, sl in enumerate(slices):
            nrows, ncols = int(sl.shape[0]), int(sl.shape[1])
            lblrec = nrows * ncols

            # 45 integer words (1-indexed in UM Doc F3)
            int_hdr = [0] * 45
            int_hdr[0] = int(attrs.get("pp:lbyr", 2026))      # 1: LBYR
            int_hdr[1] = int(attrs.get("pp:lbmon", 10))       # 2: LBMON
            int_hdr[2] = int(attrs.get("pp:lbdat", 29))       # 3: LBDAT
            int_hdr[3] = int(attrs.get("pp:lbhr", 12))        # 4: LBHR
            int_hdr[4] = int(attrs.get("pp:lbmin", 0))        # 5: LBMIN
            int_hdr[14] = lblrec                              # 15: LBLREC
            int_hdr[15] = 0                                   # 16: LBCODE (regular lat/lon)
            int_hdr[16] = 0                                   # 17: LBHEM
            int_hdr[17] = nrows                               # 18: LBROW
            int_hdr[18] = ncols                               # 19: LBNPT
            int_hdr[20] = 0                                   # 21: LBPACK (unpacked IEEE float32)
            int_hdr[21] = 2                                   # 22: LBREL (header release 2)
            int_hdr[22] = int(attrs.get("pp:lbfc", 16))       # 23: LBFC (field code)
            int_hdr[32] = idx + 1                             # 33: LBLEV
            int_hdr[38] = 1                                   # 39: LBUSER(1) = 1 (real data)
            int_hdr[41] = int(attrs.get("pp:stash", 16004))   # 42: LBUSER(4) STASH code

            # 19 real words (words 46..64 in UM Doc F3)
            real_hdr = [0.0] * 19
            real_hdr[6] = float(attrs.get("pp:blev", float(idx + 1)))     # 52: BLEV
            real_hdr[10] = float(attrs.get("pp:bplat", 90.0))             # 56: BPLAT
            real_hdr[11] = float(attrs.get("pp:bplon", 0.0))              # 57: BPLON
            real_hdr[13] = float(attrs.get("pp:bzy", 80.1))               # 59: BZY (zeroth lat)
            real_hdr[14] = float(attrs.get("pp:bdy", -0.1))               # 60: BDY (lat step)
            real_hdr[15] = float(attrs.get("pp:bzx", -25.1))              # 61: BZX (zeroth lon)
            real_hdr[16] = float(attrs.get("pp:bdx", 0.1))                # 62: BDX (lon step)
            real_hdr[17] = float(attrs.get("pp:bmdi", -1.07374182e9))     # 63: BMDI (missing data indicator)
            real_hdr[18] = 1.0                                            # 64: BMKS

            hdr_bytes = struct.pack(">45i19f", *int_hdr, *real_hdr)
            assert len(hdr_bytes) == 256
            fp.write(struct.pack(">I", 256))
            fp.write(hdr_bytes)
            fp.write(struct.pack(">I", 256))

            payload_be = np.ascontiguousarray(sl, dtype=">f4").tobytes()
            plen = len(payload_be)
            fp.write(struct.pack(">I", plen))
            fp.write(payload_be)
            fp.write(struct.pack(">I", plen))

    return out_file


def from_pp(
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
    """Import a UK Met Office PP (.pp) binary file into a WX-1 scientific container."""
    import numpy as np
    import wx1

    in_file = Path(input_path)
    if not in_file.exists():
        raise FileNotFoundError(f"PP file not found: {in_file}")

    slices = []
    attrs: dict[str, str] = {
        "wx1:source_format": "pp",
        "wx1:source_file": in_file.name,
        "wx1:converted_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }

    with open(in_file, "rb") as fp:
        rec_idx = 0
        while True:
            marker_buf = fp.read(4)
            if not marker_buf or len(marker_buf) < 4:
                break
            (hdr_len,) = struct.unpack(">I", marker_buf)
            if hdr_len != 256:
                raise ValueError(f"Invalid PP lookup header record length {hdr_len} (expected 256)")
            hdr_bytes = fp.read(256)
            (hdr_end,) = struct.unpack(">I", fp.read(4))
            if hdr_end != 256:
                raise ValueError("Corrupt Fortran trailing record marker on PP header")

            words = struct.unpack(">45i19f", hdr_bytes)
            int_hdr = words[:45]
            real_hdr = words[45:]
            nrows = int(int_hdr[17])
            ncols = int(int_hdr[18])

            if rec_idx == 0:
                attrs["pp:lbyr"] = str(int_hdr[0])
                attrs["pp:lbmon"] = str(int_hdr[1])
                attrs["pp:lbdat"] = str(int_hdr[2])
                attrs["pp:lbhr"] = str(int_hdr[3])
                attrs["pp:lbrow"] = str(nrows)
                attrs["pp:lbnpt"] = str(ncols)
                attrs["pp:lbpack"] = str(int_hdr[20])
                attrs["pp:lbfc"] = str(int_hdr[22])
                attrs["pp:stash"] = str(int_hdr[41])
                attrs["pp:bzy"] = f"{real_hdr[13]:.4f}"
                attrs["pp:bdy"] = f"{real_hdr[14]:.4f}"
                attrs["pp:bzx"] = f"{real_hdr[15]:.4f}"
                attrs["pp:bdx"] = f"{real_hdr[16]:.4f}"
                attrs["pp:bmdi"] = f"{real_hdr[17]:.6e}"

            (plen,) = struct.unpack(">I", fp.read(4))
            payload = fp.read(plen)
            (plen_end,) = struct.unpack(">I", fp.read(4))
            if plen != plen_end:
                raise ValueError("Corrupt Fortran record marker on PP data payload")

            arr = np.frombuffer(payload, dtype=">f4").astype(np.float32).reshape((nrows, ncols))
            slices.append(arr)
            rec_idx += 1

    if not slices:
        raise ValueError(f"No valid PP records found in: {in_file}")

    if len(slices) == 1:
        data = slices[0]
        dims = ["latitude", "longitude"]
        roles = ["latitude", "longitude"]
    else:
        data = np.stack(slices, axis=0)
        dims = ["level", "latitude", "longitude"]
        roles = ["vertical", "latitude", "longitude"]

    resolved_chunks = _resolve_safe_chunks(data.shape, data.dtype.itemsize, chunk_shape)
    wx1.write_array(
        output_wx1_path,
        data,
        dataset_id=dataset_id or f"{in_file.stem}_pp",
        variable_path="/pp_field",
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
# 2. UK Met Office NIMROD Format (.nimrod)
# ---------------------------------------------------------------------------
def to_nimrod(
    input_wx1_path: str | Path,
    output_nimrod_path: str | Path,
    *,
    variable_path: str | None = None,
) -> Path:
    """Export a WX-1 2D/3D container to a UK Met Office NIMROD (.nimrod) binary file."""
    import numpy as np
    import wx1

    in_file = Path(input_wx1_path)
    out_file = Path(output_nimrod_path)

    with wx1.open_file(in_file) as f:
        var_info = f.variables[0] if variable_path is None else next(
            v for v in f.variables if v["path"] == variable_path
        )
        data = np.asarray(f.read_array(var_info["path"]), dtype=np.float32)
        attrs = f.attributes

    slices = [data] if data.ndim == 2 else [data[i] for i in range(data.shape[0])]
    out_file.parent.mkdir(parents=True, exist_ok=True)

    with open(out_file, "wb") as fp:
        for sl in slices:
            nrows, ncols = int(sl.shape[0]), int(sl.shape[1])
            # NIMROD 512-byte header:
            # 31x int16 (62B) + 28x float32 (112B) + 45x float32 (180B) + 56B char + 51x int16 (102B) = 512B
            gen_ints = [0] * 31
            gen_ints[0] = 2026        # 1: Validity year
            gen_ints[1] = 10          # 2: Validity month
            gen_ints[2] = 29          # 3: Validity day
            gen_ints[3] = 12          # 4: Validity hour
            gen_ints[11] = 0          # 12: Datum type (0 = float32 real)
            gen_ints[12] = 4          # 13: Bytes per datum (4)
            gen_ints[15] = nrows      # 16: Number of rows
            gen_ints[16] = ncols      # 17: Number of columns
            gen_ints[18] = int(attrs.get("nimrod:field_code", 63))  # 19: Field code

            gen_reals = [0.0] * 28
            gen_reals[1] = -32767.0   # 33: Missing data value
            gen_reals[2] = float(attrs.get("nimrod:y_top", 80.0))
            gen_reals[3] = float(attrs.get("nimrod:dy", 0.1))
            gen_reals[4] = float(attrs.get("nimrod:x_left", -25.0))
            gen_reals[5] = float(attrs.get("nimrod:dx", 0.1))

            spec_reals = [0.0] * 45
            units_str = attrs.get("units", "mm/hr").encode("ascii", "replace")[:8].ljust(8, b" ")
            src_str = attrs.get("nimrod:source", "KNMI/ECMWF WX1").encode("ascii", "replace")[:24].ljust(24, b" ")
            title_str = var_info["path"].strip("/").encode("ascii", "replace")[:24].ljust(24, b" ")
            char_block = units_str + src_str + title_str  # 56 bytes
            spec_ints = [0] * 51

            hdr_bytes = (
                struct.pack(">31h", *gen_ints)
                + struct.pack(">28f", *gen_reals)
                + struct.pack(">45f", *spec_reals)
                + char_block
                + struct.pack(">51h", *spec_ints)
            )
            assert len(hdr_bytes) == 512
            fp.write(struct.pack(">I", 512))
            fp.write(hdr_bytes)
            fp.write(struct.pack(">I", 512))

            payload_be = np.ascontiguousarray(sl, dtype=">f4").tobytes()
            plen = len(payload_be)
            fp.write(struct.pack(">I", plen))
            fp.write(payload_be)
            fp.write(struct.pack(">I", plen))

    return out_file


def from_nimrod(
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
    """Import a UK Met Office NIMROD (.nimrod) file into a WX-1 scientific container."""
    import numpy as np
    import wx1

    in_file = Path(input_path)
    if not in_file.exists():
        raise FileNotFoundError(f"NIMROD file not found: {in_file}")

    slices = []
    attrs: dict[str, str] = {
        "wx1:source_format": "nimrod",
        "wx1:source_file": in_file.name,
        "wx1:converted_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }

    with open(in_file, "rb") as fp:
        rec_idx = 0
        while True:
            m_buf = fp.read(4)
            if not m_buf or len(m_buf) < 4:
                break
            (hdr_len,) = struct.unpack(">I", m_buf)
            if hdr_len != 512:
                raise ValueError(f"Invalid NIMROD header length {hdr_len} (expected 512)")
            hdr_bytes = fp.read(512)
            (hdr_end,) = struct.unpack(">I", fp.read(4))
            if hdr_end != 512:
                raise ValueError("Corrupt Fortran record marker on NIMROD header")

            gen_ints = struct.unpack(">31h", hdr_bytes[:62])
            gen_reals = struct.unpack(">28f", hdr_bytes[62:174])
            char_block = hdr_bytes[354:410]
            units_str = char_block[:8].decode("ascii", "replace").strip()
            src_str = char_block[8:32].decode("ascii", "replace").strip()
            title_str = char_block[32:56].decode("ascii", "replace").strip()

            nrows = int(gen_ints[15])
            ncols = int(gen_ints[16])
            if rec_idx == 0:
                attrs["units"] = units_str
                attrs["nimrod:source"] = src_str
                attrs["nimrod:title"] = title_str
                attrs["nimrod:nrows"] = str(nrows)
                attrs["nimrod:ncols"] = str(ncols)
                attrs["nimrod:field_code"] = str(gen_ints[18])
                attrs["nimrod:y_top"] = f"{gen_reals[2]:.4f}"
                attrs["nimrod:dy"] = f"{gen_reals[3]:.4f}"
                attrs["nimrod:x_left"] = f"{gen_reals[4]:.4f}"
                attrs["nimrod:dx"] = f"{gen_reals[5]:.4f}"

            (plen,) = struct.unpack(">I", fp.read(4))
            payload = fp.read(plen)
            (plen_end,) = struct.unpack(">I", fp.read(4))
            if plen != plen_end:
                raise ValueError("Corrupt Fortran record marker on NIMROD payload")

            arr = np.frombuffer(payload, dtype=">f4").astype(np.float32).reshape((nrows, ncols))
            slices.append(arr)
            rec_idx += 1

    if not slices:
        raise ValueError(f"No valid NIMROD records found in: {in_file}")

    data = slices[0] if len(slices) == 1 else np.stack(slices, axis=0)
    dims = ["latitude", "longitude"] if data.ndim == 2 else ["time", "latitude", "longitude"]
    roles = ["latitude", "longitude"] if data.ndim == 2 else ["time", "latitude", "longitude"]
    var_name = "nimrod_field"

    resolved_chunks = _resolve_safe_chunks(data.shape, data.dtype.itemsize, chunk_shape)
    wx1.write_array(
        output_wx1_path,
        data,
        dataset_id=dataset_id or f"{in_file.stem}_nimrod",
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


# ---------------------------------------------------------------------------
# 3. NCAR DORADE Sweepfile Format (.dorade)
# ---------------------------------------------------------------------------
def to_dorade(
    input_wx1_path: str | Path,
    output_dorade_path: str | Path,
    *,
    variable_path: str | None = None,
) -> Path:
    """Export a WX-1 2D radar/grid container to an NCAR DORADE (.dorade) sweepfile."""
    import numpy as np
    import wx1

    in_file = Path(input_wx1_path)
    out_file = Path(output_dorade_path)

    with wx1.open_file(in_file) as f:
        var_info = f.variables[0] if variable_path is None else next(
            v for v in f.variables if v["path"] == variable_path
        )
        data = np.asarray(f.read_array(var_info["path"]), dtype=np.float32)
        attrs = f.attributes

    if data.ndim > 2:
        data = data[0]
    nrays, ngates = int(data.shape[0]), int(data.shape[1])
    param_name = var_info["path"].strip("/").split("/")[-1][:8].upper().ljust(8, " ").encode("ascii")
    radar_name = attrs.get("dorade:radar_name", "KNMI_RAD").encode("ascii")[:8].ljust(8, b" ")

    out_file.parent.mkdir(parents=True, exist_ok=True)
    with open(out_file, "wb") as fp:
        # 1. SSWB (Super Sweep Block, 24 bytes)
        fp.write(struct.pack(">4sII8sI", b"SSWB", 24, 1730203200, radar_name, nrays))
        # 2. VOLD (Volume Descriptor, 24 bytes)
        fp.write(struct.pack(">4sIHHHH8s", b"VOLD", 24, 1, 1, 2026, 10, b"WX1_PROJ"))
        # 3. RADD (Radar Descriptor, 24 bytes)
        fp.write(struct.pack(">4sI8sff", b"RADD", 24, radar_name, 5.3, 1.0))
        # 4. PARM (Parameter Descriptor, 24 bytes: name, binary_format=4 (IEEE float32), scale=1.0, bias=0.0)
        fp.write(struct.pack(">4sI8sIff", b"PARM", 24, param_name, 4, 1.0, 0.0)[:24])
        # 5. CELV (Cell Range Vector, 12 + ngates*4 bytes)
        celv_len = 12 + ngates * 4
        ranges = np.arange(ngates, dtype=">f4") * 250.0
        fp.write(struct.pack(">4sII", b"CELV", celv_len, ngates) + ranges.tobytes())
        # 6. SWIB (Sweep Info Block, 24 bytes)
        fp.write(struct.pack(">4sI8sIf", b"SWIB", 24, radar_name, nrays, 0.5))

        # Per-ray blocks: RYIB + ASIB + RDAT
        rdat_len = 16 + ngates * 4
        for ray_idx in range(nrays):
            az = float(ray_idx) * (360.0 / max(1, nrays))
            fp.write(struct.pack(">4sIIff", b"RYIB", 20, ray_idx + 1, az, 0.5))
            fp.write(struct.pack(">4sIfff", b"ASIB", 20, 5.178, 52.101, 44.0))
            ray_bytes = np.ascontiguousarray(data[ray_idx], dtype=">f4").tobytes()
            fp.write(struct.pack(">4sI8s", b"RDAT", rdat_len, param_name) + ray_bytes)

    return out_file


def from_dorade(
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
    """Import an NCAR DORADE (.dorade) sweepfile into a WX-1 scientific container."""
    import numpy as np
    import wx1

    in_file = Path(input_path)
    if not in_file.exists():
        raise FileNotFoundError(f"DORADE file not found: {in_file}")

    rays = []
    param_name = "DBZH"
    radar_name = "KNMI_RAD"
    ngates = 0

    with open(in_file, "rb") as fp:
        while True:
            hdr = fp.read(8)
            if not hdr or len(hdr) < 8:
                break
            b_id, b_len = struct.unpack(">4sI", hdr)
            if b_len < 8:
                raise ValueError(f"Invalid DORADE descriptor length {b_len} for block {b_id!r}")
            body = fp.read(b_len - 8)
            tag = b_id.decode("ascii", "replace")
            if tag == "RADD" and len(body) >= 8:
                radar_name = body[:8].decode("ascii", "replace").strip()
            elif tag == "CELV" and len(body) >= 4:
                (ngates,) = struct.unpack(">I", body[:4])
            elif tag == "RDAT" and len(body) >= 8:
                param_name = body[:8].decode("ascii", "replace").strip() or "DBZH"
                vals = np.frombuffer(body[8:], dtype=">f4").astype(np.float32)
                rays.append(vals)

    if not rays:
        raise ValueError(f"No RDAT ray blocks found in DORADE sweepfile: {in_file}")

    data = np.stack(rays, axis=0)
    attrs = {
        "wx1:source_format": "dorade",
        "wx1:source_file": in_file.name,
        "wx1:converted_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "dorade:radar_name": radar_name,
        "dorade:parameter": param_name,
        "dorade:nrays": str(data.shape[0]),
        "dorade:ngates": str(ngates or data.shape[1]),
    }

    resolved_chunks = _resolve_safe_chunks(data.shape, data.dtype.itemsize, chunk_shape)
    wx1.write_array(
        output_wx1_path,
        data,
        dataset_id=dataset_id or f"{in_file.stem}_dorade",
        variable_path=f"/{param_name.lower()}",
        dimensions=["azimuth", "range"],
        chunk_shape=resolved_chunks,
        dimension_roles=["latitude", "longitude"],
        attributes=attrs,
        profile=profile,
        codec=codec,
        workers=workers,
        durability=durability,
    )
    return wx1.open_file(output_wx1_path)


# ---------------------------------------------------------------------------
# 4. NCAR IWRF (Integrated Weather Radar Format, .iwrf)
# ---------------------------------------------------------------------------
IWRF_RADAR_INFO_ID = 0x77770001
IWRF_SCAN_SEGMENT_ID = 0x77770002
IWRF_PULSE_HEADER_ID = 0x7777000C


def to_iwrf(
    input_wx1_path: str | Path,
    output_iwrf_path: str | Path,
    *,
    variable_path: str | None = None,
) -> Path:
    """Export a WX-1 radar/grid container to an NCAR IWRF (.iwrf) binary packet stream."""
    import numpy as np
    import wx1

    in_file = Path(input_wx1_path)
    out_file = Path(output_iwrf_path)

    with wx1.open_file(in_file) as f:
        var_info = f.variables[0] if variable_path is None else next(
            v for v in f.variables if v["path"] == variable_path
        )
        data = np.asarray(f.read_array(var_info["path"]), dtype=np.float32)
        attrs = f.attributes

    if data.ndim > 2:
        data = data[0]
    npulses, ngates = int(data.shape[0]), int(data.shape[1])
    site_name = attrs.get("iwrf:site_name", "NLDEB_KNMI").encode("ascii")[:32].ljust(32, b"\x00")

    out_file.parent.mkdir(parents=True, exist_ok=True)
    with open(out_file, "wb") as fp:
        # 1. IWRF_RADAR_INFO packet (packet header: 32 bytes + body: 64 bytes = 96 bytes)
        pkt_hdr = struct.pack("<iiiiiqi", IWRF_RADAR_INFO_ID, 96, 1, 1, 6260, 1730203200, 0)
        radar_body = struct.pack("<fffII12s32s", 52.101, 5.178, 44.0, npulses, ngates, b"C_BAND\x00\x00\x00\x00\x00\x00", site_name)
        fp.write(pkt_hdr + radar_body)

        # 2. IWRF_SCAN_SEGMENT packet (32 + 16 = 48 bytes)
        scan_hdr = struct.pack("<iiiiiqi", IWRF_SCAN_SEGMENT_ID, 48, 2, 1, 6260, 1730203200, 0)
        scan_body = struct.pack("<Ifff", 2, 0.5, 0.0, 360.0)
        fp.write(scan_hdr + scan_body)

        # 3. IWRF_PULSE_HEADER packets (32B header + 16B pulse meta + ngates*4B IEEE float32)
        pulse_pkt_len = 48 + ngates * 4
        for p_idx in range(npulses):
            az = float(p_idx) * (360.0 / max(1, npulses))
            p_hdr = struct.pack("<iiiiiqi", IWRF_PULSE_HEADER_ID, pulse_pkt_len, p_idx + 3, 1, 6260, 1730203200, p_idx * 1000)
            p_meta = struct.pack("<ffII", az, 0.5, ngates, 4)
            p_payload = np.ascontiguousarray(data[p_idx], dtype="<f4").tobytes()
            fp.write(p_hdr + p_meta + p_payload)

    return out_file


def from_iwrf(
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
    """Import an NCAR IWRF (.iwrf) packet stream into a WX-1 scientific container."""
    import numpy as np
    import wx1

    in_file = Path(input_path)
    if not in_file.exists():
        raise FileNotFoundError(f"IWRF file not found: {in_file}")

    pulses = []
    site_name = "NLDEB_KNMI"
    lat, lon, alt = 52.101, 5.178, 44.0

    with open(in_file, "rb") as fp:
        while True:
            hdr_buf = fp.read(32)
            if not hdr_buf or len(hdr_buf) < 32:
                break
            pkt_id, pkt_len, _seq, _ver, _rad_id, _sec, _nsec = struct.unpack("<iiiiiqi", hdr_buf)
            if pkt_len < 32:
                raise ValueError(f"Invalid IWRF packet length {pkt_len} for packet ID {hex(pkt_id)}")
            body = fp.read(pkt_len - 32)
            if pkt_id == IWRF_RADAR_INFO_ID and len(body) >= 64:
                lat, lon, alt, _np, _ng, _band, s_bytes = struct.unpack("<fffII12s32s", body[:64])
                site_name = s_bytes.split(b"\x00", 1)[0].decode("ascii", "replace")
            elif pkt_id == IWRF_PULSE_HEADER_ID and len(body) >= 16:
                _az, _el, ngates, _encoding = struct.unpack("<ffII", body[:16])
                vals = np.frombuffer(body[16 : 16 + ngates * 4], dtype="<f4").astype(np.float32)
                pulses.append(vals)

    if not pulses:
        raise ValueError(f"No IWRF pulse packets found in: {in_file}")

    data = np.stack(pulses, axis=0)
    attrs = {
        "wx1:source_format": "iwrf",
        "wx1:source_file": in_file.name,
        "wx1:converted_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "iwrf:site_name": site_name,
        "iwrf:latitude": f"{lat:.4f}",
        "iwrf:longitude": f"{lon:.4f}",
        "iwrf:altitude_m": f"{alt:.1f}",
        "iwrf:npulses": str(data.shape[0]),
        "iwrf:ngates": str(data.shape[1]),
    }

    resolved_chunks = _resolve_safe_chunks(data.shape, data.dtype.itemsize, chunk_shape)
    wx1.write_array(
        output_wx1_path,
        data,
        dataset_id=dataset_id or f"{in_file.stem}_iwrf",
        variable_path="/iwrf_iq_power",
        dimensions=["pulse", "gate"],
        chunk_shape=resolved_chunks,
        dimension_roles=["latitude", "longitude"],
        attributes=attrs,
        profile=profile,
        codec=codec,
        workers=workers,
        durability=durability,
    )
    return wx1.open_file(output_wx1_path)
