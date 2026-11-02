# WX-1 Uitgebreid 15-Formaten & Wetenschappelijke Gegevenstypen Rapport [WSL2 Native ext4]
**Datum**: 2026-11-02 16:39:42 CET  
**Filesysteem**: WSL2 Native ext4 (`/dev/sdd`, `/home/j_ord/wx1_bench_ext4/multiformat_suite/`)  
**Geteste Formaten (15 formaten / 16 extensies)**: `.wx1`, `.grib2` (`grid_ccsds`), `.nc` (NetCDF-4 `zlib+shuffle`), `.cdf` (NetCDF-3 Classic 64-bit), `.zarr` (Zarr v3), `.h5`, `.hdf5`, `.odim.h5` (`ODIM_H5/V2_4`), `.hdf` (HDF4 `pyhdf.SD`), `.bufr` (WMO FM 94 Edition 4), `.crex` (WMO FM 95 Edition 2), `.pp` (UKMO PP), `.nimrod` (UKMO NIMROD), `.dorade` (NCAR DORADE), `.iwrf` (NCAR IWRF), `.tif` (GeoTIFF)  

## 1. Deel 1: Eerlijke Formaatvergelijking over 13 Meteorologische Variabelen (Inclusief 50-Member Ensemble `enfo_2t`)
Alle eerdere methodologische fouten zijn gecorrigeerd en geverifieerd:
- **NetCDF-4 (`zlib=True, shuffle=True, complevel=4`)**: Geverifieerd met `ncdump -hs` (`_Storage = "chunked"`, `_Shuffle = "true"`, `_DeflateLevel = 4`).
- **GRIB2 (`packingType = grid_ccsds`)**: Behoudt het originele CCSDS/AEC pakkingstype en `bitsPerValue` én de 50 ensemble-leden (`perturbationNumber = 1..50`, `numberOfForecastsInEnsemble = 50`).
- **Ratio t.o.v. Raw én Ratio t.o.v. Beste Concurrent**: Zowel de verhouding tegen ongecomprimeerde data als tegen $\min(\text{GRIB2}_{\text{CCSDS}}, \text{NetCDF4}_{\text{zlib}}, \text{Zarr}_{\text{v3}})$ staat hieronder vermeld.
- **Eerlijke Analyse van Uitkomsten**: WX-1 (Zstd+Shuffle) haalt lichte winst bij continue thermodynamische en 3D-velden (`msl`, `sot`, `enfo_2t`, `olr`), maar verliest duidelijk van GRIB2 CCSDS bij sparse neerslag (`tp`, 0.02× door WX-1 containeroverhead bij 10 KB), gequantiseerde windvectoren (`10u`, `10v`) en discrete code-tabellen (`ptype`). Bij `t` is de ruwe bron float64 (18.43 MB, 29.0×); vergeleken met float32 raw (9.21 MB) is de reële verhouding 14.5×.

| Variabele | Categorie & Vorm | Raw (MB) | WX-1 MB (bpv) | NetCDF-4 zlib MB (bpv) | Zarr v3 MB (bpv) | GRIB2 CCSDS MB (bpv) | WX-1 vs Raw | WX-1 vs Beste Concurrent | Kleinste Formaat |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :--- |
| **`2t`** | Thermodynamic Scalar `[261, 661]` | 0.66 | **0.072 (3.49)** | 0.077 (3.76) | 0.093 (4.52) | 0.072 (3.48) | 9.2× | **1.00×** | **GRIB2 (grid_ccsds)** |
| **`msl`** | Atmospheric Pressure `[261, 661]` | 0.66 | **0.086 (4.18)** | 0.090 (4.36) | 0.131 (6.39) | 0.099 (4.82) | 7.7× | **1.04×** | **WX-1 (Zstd+Shuffle)** |
| **`10u`** | Dynamic Wind Vector (U) `[261, 661]` | 0.66 | **0.186 (9.06)** | 0.176 (8.54) | 0.169 (8.24) | 0.116 (5.62) | 3.5× | **0.62×** | **GRIB2 (grid_ccsds)** |
| **`10v`** | Dynamic Wind Vector (V) `[261, 661]` | 0.66 | **0.171 (8.32)** | 0.164 (7.97) | 0.164 (7.95) | 0.112 (5.42) | 3.9× | **0.65×** | **GRIB2 (grid_ccsds)** |
| **`tp`** | Precipitation Flux (Sparse) `[261, 661]` | 0.66 | **0.010 (0.50)** | 0.014 (0.69) | 0.002 (0.10) | 0.000 (0.01) | 63.7× | **0.02×** | **GRIB2 (grid_ccsds)** |
| **`tcc`** | Bounded Fraction [0, 1] `[261, 661]` | 0.66 | **0.080 (3.90)** | 0.085 (4.12) | 0.094 (4.55) | 0.053 (2.57) | 8.2× | **0.66×** | **GRIB2 (grid_ccsds)** |
| **`lsm`** | Land-Sea Mask (Binary) `[261, 661]` | 0.66 | **0.065 (3.15)** | 0.070 (3.40) | 0.079 (3.86) | 0.056 (2.74) | 10.2× | **0.87×** | **GRIB2 (grid_ccsds)** |
| **`ptype`** | Discrete WMO Code Table `[261, 661]` | 0.66 | **0.030 (1.47)** | 0.032 (1.58) | 0.015 (0.71) | 0.108 (5.27) | 21.8× | **0.48×** | **Zarr v3** |
| **`sot`** | Subsurface Multi-Layer (4 levels) `[4, 261, 661]` | 2.63 | **0.295 (3.58)** | 0.307 (3.74) | 0.410 (4.98) | 0.325 (3.95) | 8.9× | **1.04×** | **WX-1 (Zstd+Shuffle)** |
| **`gh`** | 3D Isobaric Geopotential (14 levels) `[14, 261, 661]` | 9.21 | **0.839 (2.91)** | 0.834 (2.90) | 1.441 (5.00) | 1.128 (3.92) | 11.0× | **0.99×** | **NetCDF-4 (zlib+shuffle)** |
| **`t`** | 3D Isobaric Temperature (14 levels, raw f64) `[14, 261, 661]` | 18.43 | **0.635 (2.20)** | 0.613 (2.13) | 0.970 (3.37) | 0.772 (2.68) | 29.0× (14.5× vs f32) | **0.97×** | **NetCDF-4 (zlib+shuffle)** |
| **`enfo_2t`** | 50-Member Ensemble Perturbations `[50, 261, 661]` | 32.91 | **3.111 (3.02)** | 3.153 (3.07) | 4.547 (4.42) | 3.569 (3.47) | 10.6× | **1.01×** | **WX-1 (Zstd+Shuffle)** |
| **`olr_value`** | Satellite OLR (5568x5568 uint16 + _FillValue=65535) `[5568, 5568]` | 59.13 | **28.346 (7.67)** | 28.417 (7.69) | 35.545 (9.62) | N/A | 2.1× | **1.00×** | **WX-1 (Zstd+Shuffle)** |

### 1.1 Sleutel-voor-Sleutel Metadata- en Masker-Verificatie
- **GRIB2 Key-by-Key Vergelijking**: Voor alle 12 GRIB2-variabelen (inclusief `enfo_2t` met 50 ensemble-leden) zijn `discipline`, `editionNumber`, `centre`, `dataDate`, `dataTime`, `shortName`, `paramId`, `units`, `typeOfLevel`, `gridType`, `Ni`, `Nj`, `packingType`, `bitsPerValue`, `productDefinitionTemplateNumber`, `unique_levels` en `perturbations_min_max` vergeleken tussen bron en roundtrip: **100% identiek (`exact_key_match: true`)**.
- **EUMETSAT OLR (`olr_value`) `uint16` & `_FillValue = 65535` Verificatie**:
  - Opslag-dtype bron vs roundtrip: `uint16` $\rightarrow$ `uint16` (`uint16` kent zelf geen IEEE-754 `NaN`; ruimtepixels zijn gecodeerd als `_FillValue = 65535` en worden pas `NaN` bij uitpakken naar `float32`).
  - `scale_factor = 0.01`, `add_offset = 0.0`, `_FillValue = 65535`.
  - Aantal ruimte-fillpixels (`65535`): **10,606,713** pixels — **100% bit-exact maskerbehoud (`fill_mask_exact_match = True`)**.

## 2. Deel 2: Volledige Benchmark van Alle 15 Meteorologische & Wetenschappelijke Bestandsformaten op ext4
Elk formaat is fysiek aangemaakt op `/home/j_ord/wx1_bench_ext4/multiformat_suite/all_15_formats/` vanuit onze ECMWF, KNMI en EUMETSAT runs, geconverteerd naar `.wx1`, en terug geëxporteerd naar het doelformaat:

| Formaat | Extensie | Standaard / Specificatie | Vorm & Type | Bestandsgrootte (B) | WX-1 Grootte (B) | Import $\rightarrow$ WX-1 (ms) | Export $\leftarrow$ WX-1 (ms) | Roundtrip $E_{\max}$ | Verificatie & SHA-256 |
| :--- | :---: | :--- | :---: | :---: | :---: | :---: | :---: | :---: | :--- |
| **WX-1** | `.wx1` | WX-1 Container v1 (Zstd+Shuffle) | `[261, 661]` (`float32`) | 73,067 B | **73,067 B** | 10.0 ms | 1.3 ms | **0.0e+00** | Magic `WX1\x00` + BLAKE3/CRC32C verified (`e41e002e3b4967b6`) |
| **GRIB2** | `.grib2` | WMO FM 92 GRIB Edition 2 (grid_ccsds) | `[261, 661]` (`float32`) | 75,063 B | **75,306 B** | 55.2 ms | 5.7 ms | **0.0e+00** | eccodes `packingType=grid_ccsds` verified (`99037900eec38b0d`) |
| **NetCDF-4** | `.nc` | Unidata NetCDF-4 / HDF5 (zlib=4, shuffle=True) | `[261, 661]` (`float32`) | 81,091 B | **75,905 B** | 12.7 ms | 11.0 ms | **0.0e+00** | `ncdump -hs` (_DeflateLevel=4, _Shuffle=true) (`debe0bb4001ab06d`) |
| **NetCDF-3** | `.cdf` | Unidata Classic NetCDF (NETCDF3_64BIT_OFFSET) | `[261, 661]` (`float32`) | 691,664 B | **75,907 B** | 9.3 ms | 10.1 ms | **0.0e+00** | Magic `CDF\x02` (NETCDF3_64BIT_OFFSET) (`ee4607819a2a5873`) |
| **Zarr v3** | `.zarr` | Zarr v3 Specification (`zarr.json` + Blosc/Zstd) | `[261, 661]` (`float32`) | 97,554 B | **75,777 B** | 16.6 ms | 32.0 ms | **0.0e+00** | Zarr v3 `zarr.json` verified (`d788114ba4874f09`) |
| **HDF5 (.h5)** | `.h5` | HDF5 Hierarchical Data Format v5 (gzip+shuffle) | `[261, 661]` (`float32`) | 78,496 B | **75,771 B** | 16.6 ms | 11.7 ms | **0.0e+00** | Magic `\x89HDF\r\n\x1a\n` + `h5py` verified (`6042d4bba16826e9`) |
| **HDF5 (.hdf5)** | `.hdf5` | HDF5 3D Upper-Air Volume (14 levels, gzip+shuffle) | `[14, 261, 661]` (`float32`) | 639,835 B | **665,934 B** | 97.5 ms | 134.9 ms | **1.5e-05** | 14-level 3D HDF5 dataset verified (`4f369934099b6b95`) |
| **ODIM_H5** | `.odim.h5` | EUMETNET OPERA Radar ODIM_H5 (ODIM_H5/V2_0) | `[360, 320]` (`uint8`) | 331,687 B | **54,174 B** | 23.8 ms | 4.8 ms | **0.0e+00** | Authentic KNMI Den Helder (RAD:NL51, 360x320 uint8) `/what`, `/where`, `DBZH` verified (`cedb0ce424040dc6`) |
| **HDF4 (.hdf)** | `.hdf` | NCSA/NASA Classic HDF4 v4.2 Scientific Data Set (DEFLATE) | `[261, 661]` (`float32`) | 100,350 B | **75,288 B** | 12.6 ms | 9.0 ms | **0.0e+00** | Magic `\x0e\x03\x13\x01` (`pyhdf.SD`) verified (`f865d74754a1beb9`) |
| **BUFR** | `.bufr` | WMO FM 94 BUFR Edition 4 (1 message, 1000 subsets) | `[1000, 6]` (`float64`) | 10,574 B | **23,632 B** | 9.0 ms | 1.6 ms | **5.7e-14** | `bufr_ls` verified (1 Edition 4 message, 1000 subsets; 5.7e-14 float64 scale precision) (`6339141fba07ecc5`) |
| **CREX** | `.crex` | WMO FM 95 CREX Edition 2 (`++CREX++ ... 7777`) | `[1000, 6]` (`float64`) | 46,219 B | **22,966 B** | 8.8 ms | 4.2 ms | **0.0e+00** | Sections 0-4 (`++CREX++`..`7777`) verified (`c9d3739e97292207`) |
| **PP Format** | `.pp` | UK Met Office UM Post-Processing Format (64-word big-endian Lookup) | `[261, 661]` (`float32`) | 690,356 B | **73,714 B** | 8.4 ms | 1.6 ms | **0.0e+00** | Fortran record marker `0x00000100` (256 B header) verified (`e112fe74bbf6436d`) |
| **NIMROD** | `.nimrod` | UK Met Office NIMROD Gridded Radar Format (512-byte big-endian header) | `[360, 320]` (`uint8`) | 461,328 B | **62,893 B** | 7.6 ms | 1.4 ms | **0.0e+00** | Fortran record marker `0x00000200` (512 B header) verified (`23f8ee3ef225f458`) |
| **DORADE** | `.dorade` | NCAR DORADE Sweepfile (`SSWB`/`VOLD`/`RADD`/`PARM`/`CELV`/`SWIB`/`RYIB`/`ASIB`/`RDAT`) | `[360, 320]` (`uint8`) | 482,372 B | **62,654 B** | 8.2 ms | 1.8 ms | **0.0e+00** | 360 rays × 320 gates DORADE descriptor stream verified (`0d8712472ceee36d`) |
| **IWRF** | `.iwrf` | NCAR Integrated Weather Radar Format (`0x77770001`..`0x77770003` packets) | `[360, 320]` (`uint8`) | 478,224 B | **62,719 B** | 7.9 ms | 1.5 ms | **0.0e+00** | IWRF packet stream (`RADAR_INFO`/`SCAN_SEGMENT`/`PULSE_HEADER`) verified (`79a6cececd3f5d67`) |
| **GeoTIFF** | `.tif` | OGC GeoTIFF (Zstd + Horizontal Floating-Point Predictor) | `[261, 661]` (`float32`) | 73,429 B | **75,300 B** | 11.3 ms | 3.5 ms | **0.0e+00** | `tifffile` + GeoTIFF tags verified (`8337594007d9f44e`) |

## 3. Deel 3: Multi-Format CLI Pipeline (`wx1 convert` & `wx1 info`)
- **GRIB2 $\rightarrow$ WX-1**: 0.88 s
- **WX-1 $\rightarrow$ NetCDF-4**: 0.365 s
- **NetCDF-4 $\rightarrow$ Zarr**: 0.667 s
- **WX-1 $\rightarrow$ UKMO PP (`.pp`)**: 0.248 s
