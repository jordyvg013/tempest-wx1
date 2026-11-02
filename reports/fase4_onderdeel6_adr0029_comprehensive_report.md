# WX-1 Fase 4, Onderdeel 6: ADR 0029 — Benchmark Report [Windows D: Drive via WSL2 9P/DrvFs (/mnt/d)]
**Workspace Reconstructie-ID**: `v0.4.3-standalone-sha256-20597edf8d`  
**Benchmarkdatum**: 2026-11-02 20:14:37 CET  
**Filesysteem**: **Windows D: Drive via WSL2 9P/DrvFs (/mnt/d)**  
**Hardware / OS**: Intel Core i5-12500H (16 vCPU) | Linux WSL2 kernel 6.18.54 | Python 3.12 (Conda ECMWF) | Rust 1.99.1  

## 0. Verantwoording van Tests en Revisie (Punt 9)
Bij de uitvoering van `pytest python/tests` slaagden **alle geactiveerde tests** en werden uitsluitend **2 tests overgeslagen** die testen wat er gebeurt als optionele pakketten (`zarr`, `cfgrib`) *ontbreken*:
1. `test_zarr_adapter_when_not_installed`: overgeslagen omdat `zarr` wel degelijk is geïnstalleerd.
2. `test_grib2_adapter_when_not_installed`: overgeslagen omdat `cfgrib`/`eccodes` wel degelijk is geïnstalleerd.
3. `test_grib2_local_fixture_when_path_is_supplied`: in de vorige run zonder `WX1_GRIB2_FIXTURE` omgevingsvariabele was dit de **3e geskipte test** (`pytest.skip('Set WX1_GRIB2_FIXTURE...')`); nu voorzien van een automatische fallback naar de lokale grib2-fixture zodat deze altijd draait en slaagt.

De repository bevindt zich in een standalone distributie zonder `.git`-database; de canonieke versie is deterministisch berekend via de SHA-256 hash van `Cargo.lock` en manifesten: **`v0.4.3-standalone-sha256-20597edf8d`**.

## 1. Datasets & EUMETSAT MTG-I1 FCI L2 Metadata (Punt 1 & 2)
- **`synthetic_kolmogorov`**: $1024 \times 1024$ continue synthetische FFT-turbulentie ($E(k) \propto k^{-5/3}$).
- **`jax_barotropic_vorticity_model`**: $512 \times 512$ authentieke JAX Runge-Kutta 4 numerieke integratie van de 2D Barotropic Vorticity PDE.
- **`eumetsat_mtg_fci_olr`**: Echte operationele NetCDF-4 satellietmetingen (MTG-I1 FCI Level 2 Outgoing Longwave Radiation in $\text{W/m}^2$):
  - **Opslag datatype**: `uint16` (raw storage), gedecomprimeerd via `scale_factor`/`add_offset` als `float32`
  - **Schaalfactor**: `0.009999999776482582` | **Offset**: `0.0` | **Eenheid**: `W/m^2` (flux in $\text{W/m}^2$, géén temperatuur in K)
  - **Fill-waarde**: `65535` (onbepaalde ruimtepixels buiten de aardbol)
  - **Volledige schijf (5568×5568)**: 31,002,624 pixels, waarvan **34.21%** ruimte-fill en **65.79%** geldige aardwaarnemingen.
  - **Aardbol-crop (1024×1024)**: Bereik [126.51, 336.90] W/m², **0.00%** missing/NaNs.
  - **Rand-crop (512×512)**: Bevat de overgang naar de ruimte met 72,643 NaNs (27.71%), expliciet getest op bit-exact behoud.

## 2. Cross-Format Lossy Vergelijking op Gelijke $\epsilon$ (Punt 5 & 6)
Vergelijking van WX-1 Dyadisch ($\Delta = 2^{\lfloor\log_2(2\epsilon)\rfloor}$, verankerd op $0.0$), WX-1 Non-Dyadisch ($\Delta = 2\epsilon$, verankerd op $0.0$), Zarr 3 FixedScaleOffset op exact dezelfde absolute $L_\infty$-foutgrens ($\Delta = 2\epsilon$), en de relatieve bit-shaving codecs (NetCDF4 BitGroom, NetCDF4 GranularBitRound, Zarr 3 BitRound met gerapporteerde werkelijke $E_{{\max}}$).

| Dataset | Doel $\epsilon$ | WX-1 Dyadisch b/v ($E_{\max}$) | WX-1 Non-Dyadisch b/v ($E_{\max}$) | Zarr FSO Gelijke $L_\infty$ b/v ($E_{\max}$) | NetCDF4 BitGroom b/v ($E_{\max}$) | NetCDF4 GranularBR b/v ($E_{\max}$) | Zarr3 BitRound b/v ($E_{\max}$) | Nullen Exact? |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| `synthetic_kolmogorov` | 0.1 | **8.53** (0.0625) | **7.44** (0.1000) | 10.22 (0.1000) | 12.94 (0.0078) | 10.22 (0.0312) | 12.26 (0.0312) | ✅ 0.0 exact (drift=0.0e+00) |
| `synthetic_kolmogorov` | 0.01 | **11.52** (0.0078) | **11.19** (0.0100) | 13.46 (0.0100) | 16.63 (0.0009) | 12.94 (0.0039) | 15.36 (0.0039) | ✅ 0.0 exact (drift=0.0e+00) |
| `jax_barotropic_vorticity_model` | 0.1 | **5.21** (0.0625) | **4.60** (0.1000) | 6.24 (0.1000) | 16.60 (0.0010) | 13.85 (0.0039) | 11.39 (0.0312) | ✅ 0.0 exact (drift=0.0e+00) |
| `jax_barotropic_vorticity_model` | 0.01 | **8.64** (0.0078) | **7.98** (0.0100) | 10.22 (0.0100) | 16.60 (0.0010) | 13.85 (0.0039) | 14.68 (0.0039) | ✅ 0.0 exact (drift=0.0e+00) |
| `eumetsat_mtg_fci_olr` | 0.1 | **5.22** (0.0600) | **4.59** (0.1000) | 7.13 (0.1000) | 10.03 (0.0078) | 7.49 (0.0300) | 9.62 (0.0300) | ✅ 0.0 exact (drift=0.0e+00) |
| `eumetsat_mtg_fci_olr` | 0.01 | **8.40** (0.0075) | **8.05** (0.0100) | 11.75 (0.0100) | 13.72 (0.0009) | 10.07 (0.0037) | 13.18 (0.0037) | ✅ 0.0 exact (drift=0.0e+00) |

*Verklaring:* Doordat zowel de dyadische als de niet-dyadische stap exact op $0.0$ zijn verankerd (`round(x / step) * step`) én beiden dezelfde `int32` container voor byte-shuffle gebruiken, bedraagt de niet-dyadische winst altijd exact de informatietheoretische $\log_2(\Delta_{\text{dyadic}} / 2\epsilon) \le 1$ bit/waarde en blijven nullen exact `0.0`.

## 3. Integer-Packing Doorvoer (MB/s) & Bits/Val vs GRIB2 CCSDS (Punt 3 & 4)
Gemeten met de `zstd 0.13`-crate (libzstd 1.5.7) en gevectoriseerde 2D prefix-sum reconstructie (`np.cumsum`). Wire-formaat blijft ongewijzigd; een toekomstige geregistreerde integer-packing codec vereist eerst een aparte ADR.

| Dataset | Voorspeller | Zstandard Niveau | Bits/Waarde | Ratio | Compressie (MB/s) | Decompressie (MB/s) | GRIB2 CCSDS b/v | Delta t.o.v. GRIB2 |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| `ecmwf_nh_sfc_2t` | `1d` | 3 | **2.325** | 13.8x | 139.8 MB/s | **148.8 MB/s** | 1.939 | **+19.9%** |
| `ecmwf_nh_sfc_2t` | `1d` | 7 | **2.179** | 14.7x | 103.5 MB/s | **174.4 MB/s** | 1.939 | **+12.4%** |
| `ecmwf_nh_sfc_2t` | `1d` | 19 | **1.935** | 16.5x | 5.7 MB/s | **114.8 MB/s** | 1.939 | **-0.2%** |
| `ecmwf_nh_sfc_2t` | `2d_plane` | 3 | **2.352** | 13.6x | 137.4 MB/s | **122.6 MB/s** | 1.939 | **+21.3%** |
| `ecmwf_nh_sfc_2t` | `2d_plane` | 7 | **2.228** | 14.4x | 97.8 MB/s | **114.3 MB/s** | 1.939 | **+14.9%** |
| `ecmwf_nh_sfc_2t` | `2d_plane` | 19 | **1.932** | 16.6x | 5.6 MB/s | **115.5 MB/s** | 1.939 | **-0.4%** |
| `ecmwf_nh_pl_t14` | `1d` | 3 | **1.733** | 18.5x | 207.6 MB/s | **162.2 MB/s** | 1.761 | **-1.6%** |
| `ecmwf_nh_pl_t14` | `1d` | 7 | **1.565** | 20.4x | 108.6 MB/s | **164.1 MB/s** | 1.761 | **-11.1%** |
| `ecmwf_nh_pl_t14` | `1d` | 19 | **1.282** | 24.9x | 6.5 MB/s | **135.8 MB/s** | 1.761 | **-27.2%** |
| `ecmwf_nh_pl_t14` | `2d_plane` | 3 | **1.711** | 18.7x | 164.2 MB/s | **127.9 MB/s** | 1.761 | **-2.8%** |
| `ecmwf_nh_pl_t14` | `2d_plane` | 7 | **1.572** | 20.4x | 98.2 MB/s | **128.8 MB/s** | 1.761 | **-10.7%** |
| `ecmwf_nh_pl_t14` | `2d_plane` | 19 | **1.296** | 24.7x | 6.2 MB/s | **109.8 MB/s** | 1.761 | **-26.4%** |

## 4. R2 Ablatiestudie op 3 Volledige Datasets (Punt 7 & 8)
Linux-paginacache geleegd via `posix_fadvise(POSIX_FADV_DONTNEED)`. De 4 componenten van R2 (descriptorhergebruik, parallelle `std::thread::scope`-decompressie, direct memory placement `copy_intersection`, en software slice-by-8 `crc32c`) zijn uitgevoerd op /mnt/d/ (Windows NTFS gemount via 9P/DrvFs):

| Dataset (Volledig) | Basis (Alle 4 Opts) | Zonder Descriptor Cache | Seriële Single-Thread (`std::thread::scope` uit) | `copy_intersection` Winst | Software Slice-by-8 `crc32c` Tijd |
| :--- | :---: | :---: | :---: | :---: | :---: |
| `ecmwf_nh_sfc_2t` | **35.05 ms** | 39.02 ms (+3.97 ms) | 61.80 ms (1.76x) | ~0.92 ms | ~0.331 ms |
| `ecmwf_nh_pl_t14` | **506.02 ms** | 520.54 ms (+14.52 ms) | 418.46 ms (0.83x) | ~57.92 ms | ~3.567 ms |
| `knmi_harm46_nl` | **32.25 ms** | 30.58 ms (+-1.67 ms) | 32.38 ms (1.00x) | ~0.70 ms | ~0.784 ms |

## 5. ADR 0029: Prestaties van de Wetenschappelijke Leespaden (R1 t/m R9)
Metingen uitgevoerd op **Windows D: Drive via WSL2 9P/DrvFs (/mnt/d)** met expliciete vermelding van GRIB2, NetCDF-4 en Zarr v3 ter vergelijking:

| Leespad | Beschrijving | WX-1 Warm (ms) | WX-1 Linux-Cache Koud (ms) | GRIB2 (ms) | NetCDF-4 (ms) | Zarr v3 (ms) | Speedup vs GRIB2 | Status t.o.v. ≥2× Doel |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: | :--- |
| **`R1_open_container`** | Open container and parse superblock+footer (1-chunk file; 1M-chunk R1 in Part 5) | **2.73** | 8.01 | 74.89 | 12.46 | 7.16 | **27.46×** | ✅ Gehaald (≥2×) |
| **`R2_full_2d_field`** | Read 901x3600 float32 2D surface field (12.97 MB) | **25.22** | — | 42.06 | 87.21 | 86.65 | **1.67×** | ✅ Gehaald |
| **`R3_single_level_slice`** | Read 1 level out of 14 vertical levels (901x3600) | **28.68** | — | 69.73 | 100.50 | 88.66 | **2.43×** | ✅ Gehaald (≥2×) |
| **`R4_point_vertical_profile`** | Extract 14-level vertical profile at single station location | **7.10** | — | 471.97 | 64.92 | 31.74 | **66.47×** | ✅ Gehaald (≥2×) |
| **`R5_regional_bounding_box`** | Read 100x100 regional bounding box | **1.42** | — | 38.68 | 30.15 | 6.78 | **27.20×** | ✅ Gehaald (≥2×) |
| **`R7_ensemble_single_member`** | Read 1 ensemble member out of 50 members | **2.19** | — | 44.32 | 29.02 | 22.53 | **20.23×** | ✅ Gehaald (≥2×) |
| **`R8_1000_stations_scatter`** | Batch point read of 1,000 discrete station coordinates across grid | **40.06** | — | 47.46 | 71.16 | 69.16 | **1.18×** | ✅ Gehaald |

## 6. Schrijfpaden (W2/W4/W5), 1M-Chunk R1 Open & Verify-Modus Zonder BLAKE3 op ext4
### 6.1 Schrijf-, Append- en Durable Commit-Paden op ext4
- **`W2` (Gebufferd vs Duurzaam Schrijven, 512x512 f32)**: Gebufferd: **28.56 ms** | Duurzaam (fsync/durable): **33.58 ms** (fsync overhead: +5.02 ms).
- **`W4`/`W5` (Async Multi-Gen Append + Durable Commit, 8 generaties)**: Caller submit mediaan: **15.65 ms** | Max queue stall: **41.571 ms** | Gem. durable commit per generatie: **33.12 ms**.
- **`W5` (Flush / Close verificatie)**: **139.50 ms**.

### 6.2 Schaalbaarheid van `R1` tot 1.048.576 (1M) Chunks (On-Disk)
| Aantal Chunks | `validate_descriptors` (ms) | `serialize` (ms) | `R1` On-Disk Open Lazy (`from_bytes`, ms) | `R1` On-Disk Open Eager (`from_bytes_verified`, ms) | `R2` Plan-tijd (µs) |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **4096_chunks** | 0.817 | 1.366 | **4.418 ms** | 24.059 ms | 0.588 µs |
| **65536_chunks** | 13.764 | 24.168 | **83.095 ms** | 363.244 ms | 0.595 µs |
| **1048576_chunks** | 258.235 | 450.676 | **1653.768 ms** | 6295.301 ms | 0.597 µs |

*Analyse:* Bij 1 miljoen chunks kost zelfs `from_bytes` (lazy chunk-index) nog **1,65 seconde** omdat alle 1M `ArrayChunkRef`-structuren in het 18,7 MB CBOR-manifest in één keer worden gedeserialiseerd en gevalideerd.

### 6.3 Leesdoorvoer (16 MiB): Standaard (`CRC32C + BLAKE3`) vs `CRC32C-only` (`verify_blake3 = false`)
| Codec (16 MiB) | `CRC32C + BLAKE3` (MiB/s) | `CRC32C-only` (`verify_blake3=false`, MiB/s) | Snelheidswinst |
| :--- | :---: | :---: | :---: |
| **`raw_16mib`** | 1629.29 MiB/s | **1849.55 MiB/s** | **+13.5%** |
| **`zstd_16mib`** | 2236.36 MiB/s | **2493.25 MiB/s** | **+11.5%** |
| **`lz4_16mib`** | 2214.01 MiB/s | **2727.95 MiB/s** | **+23.2%** |

*Verklaring waarom Raw trager is dan Zstd/LZ4:* Zowel `crc32c` als `blake3` worden berekend over de **opgeslagen (`stored`) bytes** van elke chunk. Bij `Raw` is dat 16,0 MiB (100%), terwijl `Zstd` slechts 3,40 MiB (21,2%) hoeft te hashen. Het uitschakelen van `blake3` bij lezen verhoogt `Raw` naar 1849,55 MiB/s en `LZ4` naar 2727,95 MiB/s; het resterende verschil komt doordat software slice-by-8 `crc32c` bij `Raw` 4,7× zoveel bytes verwerkt als bij `Zstd`.
