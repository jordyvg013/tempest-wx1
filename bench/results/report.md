# WX-1 Phase 2 Step 1 Smoke Benchmark Report

- **Run timestamp:** `20261101T215744Z`
- **Dataset:** `wx1-phase2-synthetic-31x16` (F32 regular lat/lon, 496 points)
- **Fixtures directory:** `/tmp/wx1-cross-format-fixtures`

| Format | Klasse | Status | Write (ms) | Open (ms) | Read (ms) | Grootte (bytes) | Max Abs Fout |
|---|---|---|---:|---:|---:|---:|---:|
| **WX-1** | lossless | `passed` | 20.877 | 0.592 | 1.504 | 9676 | 0.000000 |
| **NetCDF4** | lossless | `passed` | 48.172 | 7.114 | 0.226 | 10364 | 0.000000 |
| **Zarr 3** | lossless | `passed` | 149.615 | 11.757 | 6.54 | 7082 | 0.000000 |
| **GRIB2/ecCodes** | bounded_error | `passed` | 97.123 | 250.69 | 1.029 | 1171 | 0.000000 |

### Conclusie:
- Alle vier adapters (WX-1, NetCDF4, Zarr 3, GRIB2) zijn geslaagd voor de functionaliteits- en roundtriptests.
- WX-1, NetCDF4 en Zarr 3 zijn bit-exact (fout = 0.000000).
- GRIB2/ecCodes valt binnen de vereiste foutgrens (< 0.001 K).
