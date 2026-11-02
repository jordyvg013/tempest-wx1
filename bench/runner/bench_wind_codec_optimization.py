#!/usr/bin/env python3
"""Benchmark Codec and Chunking Optimization for ERA5 Surface Wind Fields (u10/v10).

Evaluates:
- Pure Zstandard vs Zstandard + ByteShuffle vs LZ4 vs Raw
- Chunk topology: [1, 721, 1440], [2, 721, 1440], [4, 721, 1440], [7, 721, 1440]
- Lossless vs Error-Bounded (profile="output", bound=0.05 m/s)
- Metrics: File size (MB), Compression ratio, Write time (s), R2 Single 2D Slice read latency (ms), Full volume read latency (s)
"""

from __future__ import annotations

import time
from pathlib import Path
import netCDF4 as nc
import numpy as np
import wx1

NC_FILE = Path("/mnt/d/Weer/Uitvoer/data/era5/sfc/era5_sfc_instant.nc")
OUTPUT_DIR = Path("/tmp/wind_codec_bench")
OUTPUT_DIR.mkdir(parents=True, exist_ok=True)


def run_benchmark():
    print("=" * 80)
    print("ERA5 SURFACE WIND COMPRESSION & CHUNKING BENCHMARK (u10)")
    print("=" * 80)

    assert NC_FILE.exists(), f"Missing {NC_FILE}"
    with nc.Dataset(NC_FILE, "r") as ds:
        var = ds.variables["u10"]
        u10_data = np.asarray(var[:], dtype=np.float32)
        attrs = {k: str(var.getncattr(k)) for k in var.ncattrs()}

    shape = u10_data.shape
    raw_mb = u10_data.nbytes / 1e6
    print(f"Data shape: {shape} ({u10_data.size:,} float32 elements = {raw_mb:.2f} MB raw)")

    configs = [
        # (name, codec, chunk_shape, profile, lossy_bound)
        ("1. Zstd+Shuffle (chunk [3, 721, 1440]) [Default]", "zstd_shuffle", [3, 721, 1440], "restart", None),
        ("2. Pure Zstd (chunk [3, 721, 1440])", "zstd", [3, 721, 1440], "restart", None),
        ("3. LZ4 Fast (chunk [3, 721, 1440])", "lz4", [3, 721, 1440], "restart", None),
        ("4. Pure Zstd (chunk [1, 721, 1440] - fine 2D)", "zstd", [1, 721, 1440], "restart", None),
        ("5. Pure Zstd (chunk [7, 721, 1440] - large ~29MB)", "zstd", [7, 721, 1440], "restart", None),
        ("6. Zstd+Shuffle (chunk [7, 721, 1440] - large ~29MB)", "zstd_shuffle", [7, 721, 1440], "restart", None),
        ("7. Error-bounded 0.05 m/s + Zstd+Shuffle", "zstd_shuffle", [3, 721, 1440], "output", 0.05),
        ("8. Error-bounded 0.01 m/s + Zstd+Shuffle", "zstd_shuffle", [3, 721, 1440], "output", 0.01),
    ]

    results = []

    for name, codec, chunk, prof, bound in configs:
        out_file = OUTPUT_DIR / f"bench_{len(results)}.wx1"
        if out_file.exists():
            out_file.unlink()

        t0 = time.perf_counter()
        wx1.write_array(
            out_file,
            u10_data,
            dataset_id="u10_bench",
            variable_path="/u10",
            dimensions=["time", "latitude", "longitude"],
            chunk_shape=chunk,
            codec=codec,
            profile=prof,
            lossy_error_bound=bound,
            attributes=attrs,
            durability="buffered",
        )
        t_write = time.perf_counter() - t0
        file_size_mb = out_file.stat().st_size / 1e6
        ratio = raw_mb / file_size_mb

        # Read benchmarks
        f = wx1.open_file(out_file)

        # Single 2D slice read latency (time 60, middle of dataset)
        times_slice = []
        for _ in range(10):
            t0 = time.perf_counter()
            _sl = f.read_array_slice("/u10", start=[60, 0, 0], count=[1, 721, 1440])
            times_slice.append((time.perf_counter() - t0) * 1000)
        median_slice_ms = float(np.median(times_slice))

        # Full volume read latency
        t0 = time.perf_counter()
        read_all = f.read_array("/u10")
        t_full = time.perf_counter() - t0

        # Max diff
        diff = float(np.max(np.abs(u10_data - read_all)))

        print(f"\n[{name}]")
        print(f"  Size: {file_size_mb:.2f} MB (Ratio: {ratio:.2f}x vs raw)")
        print(f"  Write throughput: {raw_mb / t_write:.1f} MB/s ({t_write:.2f} s)")
        print(f"  Single 2D slice read: {median_slice_ms:.2f} ms")
        print(f"  Full volume read: {t_full:.2f} s ({raw_mb / t_full:.1f} MB/s)")
        print(f"  Max absolute error: {diff:.6e} m/s")

        results.append({
            "name": name,
            "codec": codec,
            "chunk": chunk,
            "profile": prof,
            "bound": bound,
            "size_mb": file_size_mb,
            "ratio": ratio,
            "write_s": t_write,
            "slice_ms": median_slice_ms,
            "full_s": t_full,
            "max_error": diff,
        })

        if out_file.exists():
            out_file.unlink()

    print("\n" + "=" * 80)
    print("SUMMARY COMPARISON TABLE")
    print("=" * 80)
    print(f"{'Configuration':<52} | {'Size MB':<8} | {'Ratio':<6} | {'Slice ms':<9} | {'Max Err (m/s)':<12}")
    print("-" * 95)
    for r in results:
        print(f"{r['name']:<52} | {r['size_mb']:8.2f} | {r['ratio']:5.2f}x | {r['slice_ms']:7.2f} ms | {r['max_error']:.2e}")


if __name__ == "__main__":
    run_benchmark()
