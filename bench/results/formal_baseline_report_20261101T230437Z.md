# WX-1 Formal Baseline Cross-Format Benchmark Report

- **Datum / Tijd:** `20261101T230437Z`
- **acceptance.yaml SHA256:** `faa17ac1919a333ae873140272a93161af96508d914475cf600725270793fdb7`
- **Herhalingen:** 30 warm repetitions per read workload

## Resultatentabel

| Dataset | Formaat | Grootte (bytes) | R1 Open (ms) | R2 Vol Veld (ms) | R5 Regio Box (ms) | Max Abs Fout (K) | Bit-exact |
|---|---|---:|---:|---:|---:|---:|:---:|
| **harmonie_nl** | ZARR3 | 252,694 | 6.606 | 7.722 | 8.110 | 0.000000 | Ja |
| **harmonie_nl** | NETCDF4 | 619,712 | 2.538 | 0.269 | 0.344 | 0.000000 | Ja |
| **harmonie_nl** | GRIB2 | 305,107 | 4.277 | 2.297 | 2.333 | 0.000000 | Ja |
| **harmonie_nl** | WX1 | 268,710 | 0.854 | 12.176 | 2.502 | 0.000000 | Ja |
| **ecmwf_oper_europe** | ZARR3 | 102,993 | 6.563 | 7.399 | 7.823 | 0.000000 | Ja |
| **ecmwf_oper_europe** | NETCDF4 | 701,964 | 2.696 | 0.289 | 0.316 | 0.000000 | Ja |
| **ecmwf_oper_europe** | GRIB2 | 75,269 | 4.087 | 4.097 | 3.728 | 0.000000 | Ja |
| **ecmwf_oper_europe** | WX1 | 104,890 | 0.905 | 9.497 | 1.184 | 0.000000 | Ja |
