# WX-1 Formal Baseline Cross-Format Benchmark Report

- **Datum / Tijd:** `20261101T230317Z`
- **acceptance.yaml SHA256:** `faa17ac1919a333ae873140272a93161af96508d914475cf600725270793fdb7`
- **Herhalingen:** 30 warm repetitions per read workload

## Resultatentabel

| Dataset | Formaat | Grootte (bytes) | R1 Open (ms) | R2 Vol Veld (ms) | R5 Regio Box (ms) | Max Abs Fout (K) | Bit-exact |
|---|---|---:|---:|---:|---:|---:|:---:|
| **harmonie_nl** | ZARR3 | 252,694 | 6.532 | 7.202 | 7.509 | 0.000000 | Ja |
| **harmonie_nl** | NETCDF4 | 619,712 | 2.676 | 0.275 | 0.314 | 0.000000 | Ja |
| **harmonie_nl** | GRIB2 | 305,107 | 3.972 | 2.292 | 2.244 | 0.000000 | Ja |
| **harmonie_nl** | WX1 | 268,710 | 0.863 | 11.676 | 2.425 | 0.000000 | Ja |
| **ecmwf_oper_europe** | ZARR3 | 102,993 | 6.430 | 6.709 | 8.303 | 0.000000 | Ja |
| **ecmwf_oper_europe** | NETCDF4 | 701,964 | 2.816 | 0.264 | 0.320 | 0.000000 | Ja |
| **ecmwf_oper_europe** | GRIB2 | 75,269 | 4.056 | 3.495 | 3.657 | 0.000000 | Ja |
| **ecmwf_oper_europe** | WX1 | 104,890 | 0.910 | 8.739 | 1.153 | 0.000000 | Ja |
