# WX-1 Formal Baseline Cross-Format Benchmark Report

- **Datum / Tijd:** `20261101T232524Z`
- **acceptance.yaml SHA256:** `faa17ac1919a333ae873140272a93161af96508d914475cf600725270793fdb7`
- **Herhalingen:** 30 warm repetitions per read workload

## Resultatentabel

| Dataset | Formaat | Grootte (bytes) | R1 Open (ms) | R2 Vol Veld (ms) | R5 Regio Box (ms) | Max Abs Fout (K) | Bit-exact |
|---|---|---:|---:|---:|---:|---:|:---:|
| **harmonie_nl** | ZARR3 | 252,694 | 6.101 | 6.906 | 8.171 | 0.000000 | Ja |
| **harmonie_nl** | NETCDF4 | 619,712 | 2.560 | 0.254 | 0.327 | 0.000000 | Ja |
| **harmonie_nl** | GRIB2 | 305,107 | 3.865 | 2.194 | 2.345 | 0.000000 | Ja |
| **harmonie_nl** | WX1 | 268,710 | 0.194 | 1.780 | 0.459 | 0.000000 | Ja |
| **ecmwf_oper_europe** | ZARR3 | 102,993 | 5.979 | 6.532 | 8.317 | 0.000000 | Ja |
| **ecmwf_oper_europe** | NETCDF4 | 701,964 | 2.465 | 0.270 | 0.334 | 0.000000 | Ja |
| **ecmwf_oper_europe** | GRIB2 | 75,269 | 3.988 | 3.757 | 3.903 | 0.000000 | Ja |
| **ecmwf_oper_europe** | WX1 | 104,890 | 0.198 | 1.543 | 0.254 | 0.000000 | Ja |
