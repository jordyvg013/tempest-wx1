"""Bidirectional adapters for WMO BUFR (.bufr) and WMO CREX (.crex) station/point data (ADR 0030).

Supports:
- WMO FM 94 BUFR Edition 4 binary station/point observation encoding and decoding via ecCodes.
- WMO FM 95 CREX Edition 2 character form (`++CREX++ ... 7777`) encoding and decoding.
"""

from __future__ import annotations

import datetime
from pathlib import Path
from typing import Any, Sequence


BUFR_DEFAULT_DESCRIPTORS = [
    1001,   # 001001 WMO block number
    1002,   # 001002 WMO station number
    5001,   # 005001 Latitude (high accuracy)
    6001,   # 006001 Longitude (high accuracy)
    10051,  # 010051 Pressure reduced to mean sea level
    12101,  # 012101 Temperature/air temperature (2m)
    11001,  # 011001 Wind direction
    11002,  # 011002 Wind speed
]

BUFR_DEFAULT_KEYS = [
    "blockNumber",
    "stationNumber",
    "latitude",
    "longitude",
    "pressureReducedToMeanSeaLevel",
    "airTemperature",
    "windDirection",
    "windSpeed",
]


def _require_eccodes():
    try:
        import eccodes
        return eccodes
    except ImportError as exc:
        raise ImportError("BUFR adapter requires the 'eccodes' Python package") from exc


def from_bufr(
    input_path: str | Path,
    output_wx1_path: str | Path,
    *,
    dataset_id: str | None = None,
    codec: str = "zstd_shuffle",
    workers: int = 1,
    profile: str = "restart",
    durability: str = "durable",
):
    """Import a WMO FM 94 BUFR file into a WX-1 scientific container."""
    import numpy as np
    import wx1

    eccodes = _require_eccodes()
    in_file = Path(input_path)
    if not in_file.exists():
        raise FileNotFoundError(f"BUFR file not found: {in_file}")

    rows = []
    meta_attrs: dict[str, str] = {
        "wx1:source_format": "bufr",
        "wx1:source_file": in_file.name,
        "wx1:converted_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }

    with open(in_file, "rb") as f:
        msg_idx = 0
        while True:
            bid = eccodes.codes_bufr_new_from_file(f)
            if bid is None:
                break
            try:
                eccodes.codes_set(bid, "unpack", 1)
                if msg_idx == 0:
                    for mkey in (
                        "edition",
                        "centre",
                        "masterTablesVersionNumber",
                        "localTablesVersionNumber",
                        "dataCategory",
                        "typicalDate",
                        "typicalTime",
                        "numberOfSubsets",
                    ):
                        try:
                            meta_attrs[f"bufr:{mkey}"] = str(eccodes.codes_get(bid, mkey))
                        except Exception:
                            pass

                n_sub = int(eccodes.codes_get(bid, "numberOfSubsets"))
                cols = []
                active_keys = []
                for key in BUFR_DEFAULT_KEYS:
                    try:
                        vals = eccodes.codes_get_array(bid, key)
                        if len(vals) == n_sub:
                            cols.append(np.asarray(vals, dtype=np.float64))
                            active_keys.append(key)
                        elif len(vals) == 1:
                            cols.append(np.full(n_sub, float(vals[0]), dtype=np.float64))
                            active_keys.append(key)
                    except Exception:
                        pass
                if cols:
                    msg_matrix = np.column_stack(cols)
                    rows.append(msg_matrix)
                    meta_attrs["bufr:columns"] = ",".join(active_keys)
                msg_idx += 1
            finally:
                eccodes.codes_release(bid)

    if not rows:
        raise ValueError(f"No valid station subsets decoded from BUFR file: {in_file}")

    data = np.vstack(rows).astype(np.float64)
    meta_attrs["bufr:total_subsets"] = str(data.shape[0])

    ds_id = dataset_id or f"{in_file.stem}_bufr"
    wx1.write_array(
        output_wx1_path,
        data,
        dataset_id=ds_id,
        variable_path="/observations",
        dimensions=["station", "parameter"],
        chunk_shape=[min(data.shape[0], 4096), data.shape[1]],
        dimension_roles=["other", "other"],
        attributes=meta_attrs,
        profile=profile,
        codec=codec,
        workers=workers,
        durability=durability,
    )
    return wx1.open_file(output_wx1_path)


def to_bufr(
    input_wx1_path: str | Path,
    output_bufr_path: str | Path,
    *,
    variable_path: str | None = None,
) -> Path:
    """Export a WX-1 station observation container to a WMO FM 94 BUFR Edition 4 file."""
    import numpy as np
    import wx1

    eccodes = _require_eccodes()
    in_file = Path(input_wx1_path)
    out_file = Path(output_bufr_path)

    with wx1.open_file(in_file) as f:
        var_info = f.variables[0] if variable_path is None else next(
            v for v in f.variables if v["path"] == variable_path
        )
        data = np.asarray(f.read_array(var_info["path"]), dtype=np.float64)
        attrs = f.attributes

    if data.ndim == 1:
        data = data.reshape(-1, 1)
    n_subsets, n_cols = data.shape

    out_file.parent.mkdir(parents=True, exist_ok=True)
    bid = eccodes.codes_bufr_new_from_samples("BUFR4")
    try:
        eccodes.codes_set(bid, "numberOfSubsets", int(n_subsets))
        eccodes.codes_set(bid, "compressedData", 1)
        for mkey in ("masterTablesVersionNumber", "localTablesVersionNumber", "dataCategory", "typicalDate", "typicalTime"):
            if f"bufr:{mkey}" in attrs:
                try:
                    eccodes.codes_set(bid, mkey, int(attrs[f"bufr:{mkey}"]))
                except Exception:
                    pass

        eccodes.codes_set_array(bid, "unexpandedDescriptors", BUFR_DEFAULT_DESCRIPTORS[:n_cols])
        for col_idx, key in enumerate(BUFR_DEFAULT_KEYS[:n_cols]):
            col_vals = data[:, col_idx]
            if key in ("blockNumber", "stationNumber", "windDirection"):
                eccodes.codes_set_array(bid, key, np.round(col_vals).astype(np.int64).tolist())
            else:
                eccodes.codes_set_array(bid, key, col_vals.astype(np.float64).tolist())

        eccodes.codes_set(bid, "pack", 1)
        with open(out_file, "wb") as out_f:
            eccodes.codes_write(bid, out_f)
    finally:
        eccodes.codes_release(bid)

    return out_file


def to_crex(
    input_wx1_path: str | Path,
    output_crex_path: str | Path,
    *,
    variable_path: str | None = None,
) -> Path:
    """Export a WX-1 station observation container to a WMO FM 95 CREX Edition 2 character file."""
    import numpy as np
    import wx1

    in_file = Path(input_wx1_path)
    out_file = Path(output_crex_path)

    with wx1.open_file(in_file) as f:
        var_info = f.variables[0] if variable_path is None else next(
            v for v in f.variables if v["path"] == variable_path
        )
        data = np.asarray(f.read_array(var_info["path"]), dtype=np.float64)
        attrs = f.attributes

    if data.ndim == 1:
        data = data.reshape(-1, 1)

    typical_date = attrs.get("bufr:typicalDate", "20261029")
    typical_time = attrs.get("bufr:typicalTime", "1200")[:4]
    col_names = attrs.get("bufr:columns", ",".join(BUFR_DEFAULT_KEYS[: data.shape[1]]))

    lines = [
        "++CREX++",
        f"T000201 A000 P00098 U00 S001 Y{typical_date} H{typical_time}",
        "B01001 B01002 B05001 B06001 B10051 B12101 B11001 B11002 E",
        f"(COLUMNS: {col_names})=",
    ]
    for row_idx in range(data.shape[0]):
        r = data[row_idx]
        tokens = []
        if data.shape[1] > 0: tokens.append(f"{int(round(r[0])):02d}")
        if data.shape[1] > 1: tokens.append(f"{int(round(r[1])):03d}")
        if data.shape[1] > 2: tokens.append(f"{r[2]:+09.5f}")
        if data.shape[1] > 3: tokens.append(f"{r[3]:+010.5f}")
        if data.shape[1] > 4: tokens.append(f"{r[4]:08.1f}")
        if data.shape[1] > 5: tokens.append(f"{r[5]:07.2f}")
        if data.shape[1] > 6: tokens.append(f"{int(round(r[6])):03d}")
        if data.shape[1] > 7: tokens.append(f"{r[7]:06.2f}")
        lines.append(" ".join(tokens) + "+")

    lines[-1] = lines[-1][:-1] + "="
    lines.append("7777")
    out_file.parent.mkdir(parents=True, exist_ok=True)
    out_file.write_text("\n".join(lines) + "\n", encoding="ascii")
    return out_file


def from_crex(
    input_path: str | Path,
    output_wx1_path: str | Path,
    *,
    dataset_id: str | None = None,
    codec: str = "zstd_shuffle",
    workers: int = 1,
    profile: str = "restart",
    durability: str = "durable",
):
    """Import a WMO FM 95 CREX character file into a WX-1 scientific container."""
    import numpy as np
    import wx1

    in_file = Path(input_path)
    if not in_file.exists():
        raise FileNotFoundError(f"CREX file not found: {in_file}")

    raw_text = in_file.read_text(encoding="ascii", errors="replace")
    if "++CREX++" not in raw_text or "7777" not in raw_text:
        raise ValueError(f"Invalid WMO CREX message structure in: {in_file}")

    lines = [ln.strip() for ln in raw_text.splitlines() if ln.strip()]
    rows = []
    sec1_header = ""
    descriptors_line = ""

    for ln in lines:
        if ln.startswith(("++CREX++", "7777", "(COLUMNS:")):
            continue
        if ln.startswith("T00"):
            sec1_header = ln
            continue
        if ln.startswith("B01"):
            descriptors_line = ln
            continue
        clean = ln.rstrip("+=").strip()
        if not clean:
            continue
        vals = [float(tok) for tok in clean.split()]
        rows.append(vals)

    if not rows:
        raise ValueError(f"No observation subsets found in CREX file: {in_file}")

    data = np.asarray(rows, dtype=np.float64)
    attrs = {
        "wx1:source_format": "crex",
        "wx1:source_file": in_file.name,
        "wx1:converted_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "crex:section1": sec1_header,
        "crex:descriptors": descriptors_line,
        "bufr:columns": ",".join(BUFR_DEFAULT_KEYS[: data.shape[1]]),
    }

    ds_id = dataset_id or f"{in_file.stem}_crex"
    wx1.write_array(
        output_wx1_path,
        data,
        dataset_id=ds_id,
        variable_path="/observations",
        dimensions=["station", "parameter"],
        chunk_shape=[min(data.shape[0], 4096), data.shape[1]],
        dimension_roles=["other", "other"],
        attributes=attrs,
        profile=profile,
        codec=codec,
        workers=workers,
        durability=durability,
    )
    return wx1.open_file(output_wx1_path)
