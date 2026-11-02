# WX-1 Formal Baseline Cross-Format Benchmark Report

- **Datum / Tijd:** `20261101T222740Z`
- **acceptance.yaml SHA256:** `faa17ac1919a333ae873140272a93161af96508d914475cf600725270793fdb7`
- **Herhalingen:** 30 warm repetitions per read workload

## Resultatentabel

| Dataset | Formaat | Grootte (bytes) | R1 Open (ms) | R2 Vol Veld (ms) | R5 Regio Box (ms) | Max Abs Fout (K) | Bit-exact |
|---|---|---:|---:|---:|---:|---:|:---:|
| **harmonie_nl** | ZARR3 | 252,694 | 7.476 | 6.687 | 8.077 | 0.000000 | Ja |
| **harmonie_nl** | NETCDF4 | 619,712 | 2.897 | 0.262 | 0.310 | 0.000000 | Ja |
| **harmonie_nl** | GRIB2 | 305,107 | 4.263 | 2.177 | 2.321 | 0.000000 | Ja |
| **harmonie_nl** | WX1 | 268,710 | 1.408 | 1.823 | 0.481 | 0.000000 | Ja |
| **ecmwf_oper_europe** | ZARR3 | 102,993 | 5.467 | 6.570 | 7.905 | 0.000000 | Ja |
| **ecmwf_oper_europe** | NETCDF4 | 701,964 | 2.378 | 0.278 | 0.319 | 0.000000 | Ja |
| **ecmwf_oper_europe** | GRIB2 | 75,269 | 3.863 | 3.775 | 3.593 | 0.000000 | Ja |
| **ecmwf_oper_europe** | WX1 | 104,890 | 0.758 | 1.540 | 0.254 | 0.000000 | Ja |
