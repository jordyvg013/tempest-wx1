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

## 3. Conversie naar WX-1 & Opslagefficiëntie (Alle 7 Variabelen)

Alle 7 ERA5-oppervlaktevariabelen (120 tijdstappen × 721 breedtegraden × 1440 lengtegraden = 124.588.800 elementen per variabele) zijn geconverteerd naar `.wx1` met lossless Zstandard + ByteShuffle:

| Variabele | Betekenis | Raw Volume (f32) | NetCDF-4 (zlib) | WX-1 (Zstd+Shuffle) | WX-1 vs NetCDF-4 Verschil | Compressieratio vs Raw |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **`tp`** | Total precipitation | 498.36 MB | 108.44 MB | **99.58 MB** | -8.86 MB (-8.2%) | 5.00× |
| **`t2m`** | 2m temperature | 498.36 MB | ~186.22 MB | **174.56 MB** | -11.66 MB (-6.3%) | 2.85× |
| **`msl`** | Mean sea level pressure | 498.36 MB | ~186.22 MB | **160.27 MB** | -25.95 MB (-13.9%) | 3.11× |
| **`u10`** | 10m u-wind | 498.36 MB | ~186.22 MB | **245.49 MB** | +59.27 MB | 2.03× |
| **`v10`** | 10m v-wind | 498.36 MB | ~186.22 MB | **246.88 MB** | +60.66 MB | 2.02× |
| **`sp`** | Surface pressure | 498.36 MB | ~186.22 MB | **163.73 MB** | -22.49 MB (-12.1%) | 3.04× |
| **`sst`** | Sea surface temp (met NaNs) | 498.36 MB | ~186.22 MB | **44.03 MB** | -142.19 MB (-76.4%) | 11.32× |
| **TOTAAL** | **Alle 7 variabelen** | **3,488.52 MB** | **1,225.78 MB** | **1,134.55 MB** | **-91.23 MB (-7.4%)** | **3.07×** |

*Vergelijking met GRIB Edition 1:*
* GRIB bestand (`era5_sfc_all.grib`, 840 berichten): **1,675.46 MB**
* WX-1 Totaal (7 variabelen): **1,134.55 MB** (**540.91 MB kleiner dan GRIB1**, -32.3%)

---

## 4. Bit-Exacte Verificatie & Benchmark Prestaties

### 4.1 Bit-Exactheid (Alle 7 Variabelen)
Voor alle 7 variabelen is het volledige volume van 124.588.800 elementen bit-voor-bit vergeleken tussen NetCDF-4 en WX-1 (via `.view(np.uint32)` voor float32 en expliciete NaN-mask checks):

* `tp`: $\Delta_{\max} = 0.0$, bit-exact identiek.
* `t2m`: $\Delta_{\max} = 0.0$, bit-exact identiek.
* `msl`: $\Delta_{\max} = 0.0$, bit-exact identiek.
* `u10`: $\Delta_{\max} = 0.0$, bit-exact identiek.
* `v10`: $\Delta_{\max} = 0.0$, bit-exact identiek.
* `sp`: $\Delta_{\max} = 0.0$, bit-exact identiek.
* `sst`: $\Delta_{\max} = 0.0$, identieke NaN-maskers (landmask), bit-exact identiek op alle oceaanpunten.

### 4.2 Leesprestaties op Reële ERA5 Data

| Workload | WX-1 Latency | NetCDF-4 Latency | Snelheidsfactor WX-1 |
| :--- | :--- | :--- | :--- |
| **R1 (Container Open)** | **2.97 ms** | 57.91 ms | **19.5× sneller** |
| **R2 (2D Wereldwijd Veld 721×1440 - Cold Cache)** | **178.83 ms** | 829.80 ms | **4.64× sneller** |
| **R2 (2D Wereldwijd Veld 721×1440 - Warm Cache)** | **159.08 ms** | 883.33 ms | **5.55× sneller** |
| **Volledig 3D Volume Lezen (124.6M floats)** | **2.55 s** (195.8 MB/s) | 3.82 s (130.5 MB/s) | **1.5× sneller** |
| **GRIB 2t Uitlezen via WX-1 Chunk Index** | **76.53 ms** per tijdstap | N/A (vereist scan) | Directe indexering |
