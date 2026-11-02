# WX-1 Formal Baseline Cross-Format Benchmark Report

- **Datum / Tijd:** `20261101T230513Z`
- **acceptance.yaml SHA256:** `faa17ac1919a333ae873140272a93161af96508d914475cf600725270793fdb7`
- **Herhalingen:** 30 warm repetitions per read workload

## Resultatentabel

| Dataset | Formaat | Grootte (bytes) | R1 Open (ms) | R2 Vol Veld (ms) | R5 Regio Box (ms) | Max Abs Fout (K) | Bit-exact |
|---|---|---:|---:|---:|---:|---:|:---:|
| **harmonie_nl** | ZARR3 | 252,694 | 5.862 | 7.729 | 9.408 | 0.000000 | Ja |
| **harmonie_nl** | NETCDF4 | 619,712 | 2.407 | 0.297 | 0.332 | 0.000000 | Ja |
| **harmonie_nl** | GRIB2 | 305,107 | 3.805 | 2.451 | 2.369 | 0.000000 | Ja |
| **harmonie_nl** | WX1 | 268,710 | 0.183 | 1.863 | 0.466 | 0.000000 | Ja |
| **ecmwf_oper_europe** | ZARR3 | 102,993 | 6.121 | 8.175 | 8.972 | 0.000000 | Ja |
| **ecmwf_oper_europe** | NETCDF4 | 701,964 | 2.487 | 0.315 | 0.348 | 0.000000 | Ja |
| **ecmwf_oper_europe** | GRIB2 | 75,269 | 4.114 | 3.978 | 3.887 | 0.000000 | Ja |
| **ecmwf_oper_europe** | WX1 | 104,890 | 0.207 | 1.632 | 0.269 | 0.000000 | Ja |
