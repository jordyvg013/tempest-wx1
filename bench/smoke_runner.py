"""Smoke runner for Phase-2 step-1 cross-format benchmark adapters."""

from __future__ import annotations

import argparse
import csv
import datetime
import hashlib
import json
import os
from pathlib import Path
import time
import numpy as np
import xarray as xr

from bench.datasets.synthetic import make_synthetic_dataset, generate_fixture
from bench.adapters.wx1 import WX1Adapter
from bench.adapters.grib2 import GRIB2Adapter
from bench.adapters.netcdf4 import NetCDF4Adapter
from bench.adapters.zarr3 import Zarr3Adapter


def get_path_size(path: Path) -> int:
    if path.is_file():
        return path.stat().st_size
    elif path.is_dir():
        return sum(f.stat().st_size for f in path.rglob("*") if f.is_file())
    return 0


def run_smoke(results_dir: Path, allow_skips: bool = False) -> int:
    results_dir.mkdir(parents=True, exist_ok=True)
    fixture_dir = Path(os.environ.get("WX1_FIXTURE_DIR", "/tmp/wx1-cross-format-fixtures"))
    if not (fixture_dir / "SHA256SUMS.txt").exists():
        generate_fixture(fixture_dir)

    ds_source = make_synthetic_dataset()
    expected_data = ds_source["temperature"].values.astype("<f4")

    adapters = [
        ("wx1", WX1Adapter(), "bench_artifact.wx1"),
        ("netcdf4", NetCDF4Adapter(), "bench_artifact.nc"),
        ("zarr3", Zarr3Adapter(), "bench_artifact.zarr"),
        ("grib2", GRIB2Adapter(), "bench_artifact.grib2"),
    ]

    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_records = []

    print(f"=== WX-1 Phase 2 Step 1 Smoke Runner ({timestamp}) ===")
    print(f"Fixture: regular lat/lon 31x16 F32 ({ds_source['temperature'].size} elements)")

    scratch_dir = Path(os.environ.get("WX1_SCRATCH", "/tmp")) / f"smoke_run_{timestamp}"
    scratch_dir.mkdir(parents=True, exist_ok=True)

    any_failed = False

    for fmt_id, adapter, filename in adapters:
        art_path = scratch_dir / filename
        record = {
            "timestamp": timestamp,
            "format_id": fmt_id,
            "label": adapter.label,
            "comparison_class": adapter.comparison_class,
            "status": "pending",
            "write_ms": None,
            "open_ms": None,
            "read_ms": None,
            "artifact_bytes": 0,
            "max_abs_error": 0.0,
            "error_note": "",
        }
        try:
            # 1. Write
            t0 = time.perf_counter()
            adapter.write(ds_source, art_path)
            t_write = (time.perf_counter() - t0) * 1000.0
            record["write_ms"] = round(t_write, 3)
            record["artifact_bytes"] = get_path_size(art_path)

            # 2. Open
            t0 = time.perf_counter()
            adapter.open(art_path)
            t_open = (time.perf_counter() - t0) * 1000.0
            record["open_ms"] = round(t_open, 3)

            # 3. Read
            t0 = time.perf_counter()
            restored = adapter.read_field("temperature")
            t_read = (time.perf_counter() - t0) * 1000.0
            record["read_ms"] = round(t_read, 3)

            adapter.close()

            # 4. Correctness validation
            diff = np.abs(restored.flatten() - expected_data.flatten())
            max_err = float(np.max(diff))
            record["max_abs_error"] = max_err

            if adapter.comparison_class == "lossless":
                if max_err != 0.0:
                    record["status"] = "failed"
                    record["error_note"] = f"Lossless format had non-zero error: {max_err}"
                    any_failed = True
                else:
                    record["status"] = "passed"
            else:  # bounded_error
                if max_err > 0.001:
                    record["status"] = "failed"
                    record["error_note"] = f"Error {max_err} exceeded bound 0.001 K"
                    any_failed = True
                else:
                    record["status"] = "passed"

            print(f"[{record['status'].upper()}] {adapter.label:15s} | write: {record['write_ms']:7.3f} ms | open: {record['open_ms']:6.3f} ms | read: {record['read_ms']:6.3f} ms | bytes: {record['artifact_bytes']:7d} | max_err: {max_err:.6f}")

        except Exception as exc:
            record["status"] = "failed"
            record["error_note"] = str(exc)
            any_failed = True
            print(f"[FAILED] {adapter.label:15s} | Exception: {exc}")

        run_records.append(record)

    # Clean up artifacts
    try:
        import shutil
        shutil.rmtree(scratch_dir)
    except Exception:
        pass

    # Save CSV and JSON
    csv_file = results_dir / f"smoke_run_{timestamp}.csv"
    with open(csv_file, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=list(run_records[0].keys()))
        writer.writeheader()
        writer.writerows(run_records)

    json_file = results_dir / f"smoke_run_{timestamp}.json"
    with open(json_file, "w") as f:
        json.dump(run_records, f, indent=2)

    # Generate Markdown report
    md_file = results_dir / "report.md"
    md_lines = [
        f"# WX-1 Phase 2 Step 1 Smoke Benchmark Report",
        f"",
        f"- **Run timestamp:** `{timestamp}`",
        f"- **Dataset:** `wx1-phase2-synthetic-31x16` (F32 regular lat/lon, 496 points)",
        f"- **Fixtures directory:** `{fixture_dir}`",
        f"",
        f"| Format | Klasse | Status | Write (ms) | Open (ms) | Read (ms) | Grootte (bytes) | Max Abs Fout |",
        f"|---|---|---|---:|---:|---:|---:|---:|",
    ]
    for r in run_records:
        md_lines.append(
            f"| **{r['label']}** | {r['comparison_class']} | `{r['status']}` | {r['write_ms']} | {r['open_ms']} | {r['read_ms']} | {r['artifact_bytes']} | {r['max_abs_error']:.6f} |"
        )
    md_lines.append("")
    md_lines.append("### Conclusie:")
    if not any_failed:
        md_lines.append("- Alle vier adapters (WX-1, NetCDF4, Zarr 3, GRIB2) zijn geslaagd voor de functionaliteits- en roundtriptests.")
        md_lines.append("- WX-1, NetCDF4 en Zarr 3 zijn bit-exact (fout = 0.000000).")
        md_lines.append("- GRIB2/ecCodes valt binnen de vereiste foutgrens (< 0.001 K).")
    else:
        md_lines.append("- Één of meer adapters zijn gefaald.")
    md_lines.append("")

    md_file.write_text("\n".join(md_lines))
    print(f"\nReport written to: {md_file}")
    print(f"Data saved to: {csv_file}")

    if any_failed and not allow_skips:
        return 1
    return 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", default="bench/results")
    parser.add_argument("--allow-skips", action="store_true")
    args = parser.parse_args()
    exit_code = run_smoke(Path(args.results_dir), allow_skips=args.allow_skips)
    exit(exit_code)
