# WX-1 Formal Baseline Cross-Format Benchmark Report

- **Datum / Tijd:** `20261101T221627Z`
- **acceptance.yaml SHA256:** `faa17ac1919a333ae873140272a93161af96508d914475cf600725270793fdb7`
- **Herhalingen:** 30 warm repetitions per read workload

## Resultatentabel

| Dataset | Formaat | Grootte (bytes) | R1 Open (ms) | R2 Vol Veld (ms) | R5 Regio Box (ms) | Max Abs Fout (K) | Bit-exact |
|---|---|---:|---:|---:|---:|---:|:---:|
| **harmonie_nl** | ZARR3 | 252,694 | 5.761 | 6.701 | 7.755 | 0.000000 | Ja |
| **harmonie_nl** | NETCDF4 | 619,712 | 2.429 | 0.249 | 0.291 | 0.000000 | Ja |
| **harmonie_nl** | GRIB2 | 10,384,810 | 66.569 | 2.176 | 0.057 | 0.000000 | Ja |
| **harmonie_nl** | WX1 | 268,710 | 1.174 | 1.731 | 0.437 | 0.000000 | Ja |
| **ecmwf_oper_europe** | ZARR3 | 102,993 | 5.774 | 7.196 | 7.885 | 0.000000 | Ja |
| **ecmwf_oper_europe** | NETCDF4 | 701,964 | 2.294 | 0.250 | 0.310 | 0.000000 | Ja |
| **ecmwf_oper_europe** | GRIB2 | 3,343,978 | 93.761 | 3.553 | 0.083 | 0.000000 | Ja |
| **ecmwf_oper_europe** | WX1 | 104,890 | 0.893 | 1.465 | 0.255 | 0.000000 | Ja |
