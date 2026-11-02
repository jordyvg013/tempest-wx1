# WX-1 Rapport: ERA5 Surface Reanalysis & Multi-Format Verificatie

**Datum**: 2026-11-02 21:13:00 CET  
**Locatie Data**: `/mnt/d/Weer/Uitvoer/data/era5/sfc/` (gekopieerd vanaf `D:\Downloads\era5\sfc`)  
**Data**: ECMWF ERA5 Surface Reanalysis (Oktober 2026 / herfst), 120 wereldwijde tijdstappen op 0.25° grid (`[120, 721, 1440]`).

---

## 1. Beoordeling van de ChatGPT Feedback

De analyse van ChatGPT is **inhoudelijk zeer scherp en technisch correct**:

1. **Ongestructureerde Roosters (MPAS / ICON)**:  
   * *ChatGPT heeft gelijk*: Een 1D array van punten opslaan is niet hetzelfde als een volledige mesh. Een echt roosterschema voor ICON (driehoekig) of MPAS (Voronoi hexagonen) vereist cel-naar-knoop connectiviteitstabellen (`cells_on_edge`, `vertices_on_cell`, dual-mesh topologie). WX-1 kan dit bewaren als UGRID-conforme metadata-arrays.
2. **FP16 en BF16**:  
   * *ChatGPT heeft gelijk*: WX-1 ondersteunde F32, F64 en integer packing. Echte native half-precision datatypes (`F16` en `BF16`) zijn essentieel voor moderne AI/NWP modellen (zoals FourCastNet, GraphCast, FuXi).
3. **Exacte NWP Restarts (1000 stappen test)**:  
   * Het voorgestelde experiment (1000 stappen ononderbroken versus 500 stappen + checkpoint + herstart 500 stappen) is de ultieme gouden standaard voor bit-exacte deterministische NWP-modellen.
4. **Meteorologische Betekenis in Conversies**:  
   * Bewaren van accumulatiewindows (`stepType: accum` vs `instant`), forecast lead-times, kalenders en WMO productdefinities is essentieel. Dit is precies wat onze ADR 0030 adapter doet onder de `grib:*` en `cf:*` namespaces.

---

## 2. Analyse van de ERA5 Bestanden

In `D:\Downloads\era5\sfc` stonden 3 bestanden:

| Bestand | Formaat | Bestandsgrootte | Inhoud & Structuur |
| :--- | :--- | :--- | :--- |
| `52caa53d869b374b6d7a1f560a1a3fab.grib` | WMO GRIB Edition 1 | **1.675 GB** (1,675,463,280 B) | 840 GRIB-berichten: 7 variabelen (`10u`, `10v`, `2t`, `msl`, `sp`, `sst`, `tp`) × 120 tijdstappen op 721×1440 globaal grid. |
| `data_stream-oper_stepType-instant.nc` | NetCDF-4 (HDF5) | **1.117 GB** (1,117,335,162 B) | 6 momentane variabelen (`u10`, `v10`, `t2m`, `msl`, `sst`, `sp`) op shape `[120, 721, 1440]`. |
| `data_stream-oper_stepType-accum.nc` | NetCDF-4 (HDF5) | **108.44 MB** (108,442,483 B) | 1 geaccumuleerde variabele: `tp` (Total precipitation in meters) op shape `[120, 721, 1440]`. |

Samen vormen de twee NetCDF-bestanden exact dezelfde data als het 1.67 GB grote GRIB-bestand.

---

## 3. Conversie naar WX-1 & Opslagefficiëntie

Alle bestanden zijn gekopieerd naar `/mnt/d/Weer/Uitvoer/data/era5/sfc/` en geconverteerd naar `.wx1`:

* **`era5_sfc_accum_tp.wx1`** (Total Precipitation, 120 × 721 × 1440 f32):
  * Ongecomprimeerd raw volume: **498.36 MB**
  * NetCDF-4 (zlib): **108.44 MB**
  * **WX-1 (Zstd+ByteShuffle)**: **99.58 MB**  
    *(WX-1 is **8.86 MB kleiner** dan NetCDF-4, een compressieratio van **5.00×** vs raw).*
* **`era5_sfc_t2m.wx1`** (2m Temperatuur, 120 × 721 × 1440 f32):
  * NetCDF-4: ~186.22 MB
  * **WX-1**: **174.56 MB** (*2.85× compressie vs raw volume van 498.36 MB*).
* **`era5_sfc_msl.wx1`** (Luchtdruk op zeeniveau, 120 × 721 × 1440 f32):
  * **WX-1**: **160.27 MB** (*3.11× compressie vs raw volume van 498.36 MB*).
* **`era5_sfc_grib_2t.wx1`** (GRIB 2t tijdreeks 120 stappen):
  * GRIB raw message stream: ~239.35 MB
  * **WX-1**: **168.39 MB**.

---

## 4. Bit-Exacte Verificatie & Benchmark Prestaties

Uitgevoerd via `bench/runner/bench_era5_surface_real.py`:

### 4.1 Bit-Exactheid
* **2m Temperatuur (`t2m`)**: Volledig volume van 124,588,800 data-elementen vergeleken tussen NetCDF-4 en WX-1:
  $$\Delta_{\max} = 0.0 \quad \text{(100\% Bit-Exact Identiek)}$$
* **Total Precipitation (`tp`)**:
  $$\Delta_{\max} = 0.0 \quad \text{(100\% Bit-Exact Identiek)}$$

### 4.2 Leesprestaties op Reële ERA5 Data

| Workload | WX-1 Latency | NetCDF-4 Latency | Snelheidsfactor WX-1 |
| :--- | :--- | :--- | :--- |
| **R1 (Container Open)** | **2.97 ms** | 57.91 ms | **19.5× sneller** |
| **R2 (2D Wereldwijd Veld 721×1440)** | **126.50 ms** | 695.35 ms | **5.5× sneller** |
| **Volledig 3D Volume Lezen (124.6M floats)** | **2.55 s** (195.8 MB/s) | 3.82 s | **1.5× sneller** |
| **GRIB 2t Uitlezen via WX-1** | **76.53 ms** per tijdstap | N/A | Directe chunk-indexering |
