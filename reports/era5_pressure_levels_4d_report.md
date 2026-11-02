# WX-1 Rapport: ERA5 4D Pressure Level & Wind-Codec Benchmark

**Datum**: 2026-11-02 23:07:00 CET  
**Locatie Data**: `/mnt/d/Weer/Uitvoer/data/era5/pl/` & `/mnt/d/Weer/Uitvoer/data/era5/sfc/`  
**Bronbestand PL**: `d09d810dc31d23c9a510bf055175ffa9.grib` (25,36 GB, 13.200 berichten, WMO GRIB Edition 1)  
**Doel**: Volledige 4D atmosferische tensoren `[time, level, latitude, longitude]` en empirische evaluatie van compressie- en chunking-optimalisatie voor windvelden.

---

## 1. Structuur van de 25,36 GB ERA5 Pressure Level Dataset

* **Aantal GRIB-berichten**: Exact **13.200 berichten**
* **Decompositie**:
  $$10 \text{ variabelen} \times 11 \text{ verticale niveaus} \times 120 \text{ tijdstappen} = 13.200 \text{ berichten}$$
* **Variabelen (10)**:
  * `t` (Temperatuur, K)
  * `z` (Geopotentiaal, $m^2/s^2$)
  * `u` (U-windcomponent, $m/s$)
  * `v` (V-windcomponent, $m/s$)
  * `w` (Verticale snelheid, $Pa/s$)
  * `q` (Specifieke vochtigheid, $kg/kg$)
  * `r` (Relatieve vochtigheid, %)
  * `pv` (Potentiële vorticiteit, $K \cdot m^2 / (kg \cdot s)$)
  * `cc` (Bewolking, $0-1$)
  * `ciwc` (Wolk-ijswatergehalte, $kg/kg$)
* **Verticale Isobare Niveaus (11)**:
  `[10, 30, 50, 200, 250, 300, 500, 800, 875, 925, 1000]` hPa
* **Tijdstappen (120)**:
  1 t/m 30 september 2026, 6-uurs resolutie (00, 06, 12, 18 UTC).
* **Grid**:
  $721 \times 1440$ (0.25° globaal regulier grid).
* **Volumetrie**:
  Elke variabele omvat $120 \times 11 \times 721 \times 1440 = \mathbf{1.370.476.800 \text{ float32 waarden}} = \mathbf{5,48 \text{ GB}}$ ongecomprimeerd. Totale dataset: **54,82 GB** ongecomprimeerd.

---

## 2. 4D Tensoren in WX-1: Opslag & Slicing Prestaties

De GRIB-adapter (`python/wx1/adapters/grib2.py`) is voorzien van automatische 4D spatio-temporele detectie met geheugen-preallocatie en cache-vriendelijke 2D-slice chunking (`[1, 1, 721, 1440]`):

| Variabele | Betekenis | Raw Volume (f32) | GRIB1 Referentie | WX-1 Bestandsgrootte | Compressieratio vs Raw |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **`t`** | 4D Temperatuur (10..1000 hPa) | 5.481,91 MB | ~2.535 MB | **1.874,48 MB (1,87 GB)** | **2,92×** |
| **`z`** | 4D Geopotentiaal (10..1000 hPa) | 5.481,91 MB | ~2.535 MB | **1.511,26 MB (1,51 GB)** | **3,63×** |

### Benchmark Latenties op 4D Atmosferische Velden

| Workload | WX-1 Latency | Beschrijving / Fysische Verificatie |
| :--- | :--- | :--- |
| **R1 (Container Open)** | **8,95 ms** | Openen en valideren van 1,37 Miljard elementen container |
| **R2 (2D Vlak: T500 / 500 hPa Temperatuur)** | **24,57 ms** | Globaal veld 721×1440, fysiek bereik: 222,81 K tot 274,79 K |
| **R2 (2D Vlak: Z500 / 500 hPa Geopotentiaal)** | **38,66 ms** | Geopotential height: 4564,7 gpm (trog) tot 5978,0 gpm (subtropische rug) |
| **R3 (Volledige 3D Atmosfeer 11 niveaus)** | **223,04 ms** | Simultaan uitlezen van alle 11 isobare niveaus (11,4M floats) |
| **R4 (Virtuele Radiosonde De Bilt, 52.1°N, 5.18°E)** | **178,98 ms** | Volledig verticaal temperatuurprofiel van grondlaag tot stratosfeer |

#### Radiosonde De Bilt Profiel (1 september 2026 00:00 UTC):
* **1000 hPa**: $16,82\,^\circ\text{C}$ (289,97 K)
* **925 hPa**: $12,98\,^\circ\text{C}$
* **875 hPa**: $9,97\,^\circ\text{C}$
* **800 hPa**: $4,76\,^\circ\text{C}$
* **500 hPa**: $-15,60\,^\circ\text{C}$
* **300 hPa**: $-42,00\,^\circ\text{C}$
* **250 hPa**: $-52,07\,^\circ\text{C}$
* **200 hPa** (Tropopauze): $-57,46\,^\circ\text{C}$
* **50 hPa**: $-52,22\,^\circ\text{C}$
* **10 hPa** (Stratosfeer / Ozoninversie): $-40,62\,^\circ\text{C}$

---

## 3. Codec- & Chunking-optimalisatie voor Windvelden (u10)

Uitgevoerd via `bench/runner/bench_wind_codec_optimization.py` op alle 124.588.800 waarden van de reële ERA5 10m U-wind:

| Configuratie | Codec | Chunk Shape | Bestandsgrootte | Compressieratio | 2D Slice Latency | Max Error |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **1. Zstd+Shuffle [Standaard]** | `zstd_shuffle` | `[3, 721, 1440]` | **245,77 MB** | **2,03×** | 31,17 ms | Bit-exact ($0.0$) |
| **2. Pure Zstd (level 3)** | `zstd` | `[3, 721, 1440]` | **306,85 MB** | **1,62×** | 22,75 ms | Bit-exact ($0.0$) |
| **3. LZ4 Fast** | `lz4` | `[3, 721, 1440]` | **472,69 MB** | **1,05×** | 15,64 ms | Bit-exact ($0.0$) |
| **4. Pure Zstd (fijn 2D chunk)** | `zstd` | `[1, 721, 1440]` | **306,88 MB** | **1,62×** | **8,22 ms** | Bit-exact ($0.0$) |
| **5. Pure Zstd (groot ~29MB chunk)** | `zstd` | `[7, 721, 1440]` | **306,84 MB** | **1,62×** | 63,39 ms | Bit-exact ($0.0$) |
| **6. Zstd+Shuffle (groot ~29MB chunk)** | `zstd_shuffle` | `[7, 721, 1440]` | **245,49 MB** | **2,03×** | 67,24 ms | Bit-exact ($0.0$) |
| **7. Error-bounded 0.05 m/s** | `zstd_shuffle` | `[3, 721, 1440]` | **99,76 MB** | **5,00×** | 18,07 ms | $\le 0.031\text{ m/s}$ |
| **8. Error-bounded 0.01 m/s** | `zstd_shuffle` | `[3, 721, 1440]` | **146,20 MB** | **3,41×** | 21,30 ms | $\le 0.0078\text{ m/s}$ |

### Conclusies van de Wind-optimalisatie:
1. **ByteShuffle blijft superieur voor lossless opslag**: Zstd met ByteShuffle (245,77 MB) is **61 MB compacter** dan pure Zstd (306,85 MB), doordat de exponent- en hoogste mantisse-bytes van de windvectoren sterk correleren.
2. **2D Chunking levert een 8× snellere willekeurige slice-toegang op**: Het verkleinen van de chunk naar `[1, 721, 1440]` verlaagt de slice-uitleestijd naar **8,22 ms** (t.o.v. 63,39 ms bij grotere chunks), zónder noemenswaardig compressieverlies (306,88 MB vs 306,84 MB).
3. **Error-Bounded Quantization (`profile="output"`)**: In operationele weersvoorspellingen en post-processing is fysische meetonnauwkeurigheid van wind minimaal $\pm 0.1\text{ m/s}$. Een declared error bound van $\pm 0.01\text{ m/s}$ maakt WX-1 **146,20 MB** (40 MB kleiner dan NetCDF-4), en bij $\pm 0.05\text{ m/s}$ krimpt de windvector naar **99,76 MB** (5,0× compressie, 86 MB kleiner dan NetCDF-4).
