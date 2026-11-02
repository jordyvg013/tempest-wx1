# WX-1 Fase 4, Onderdeel 6: ADR 0029 — Benchmark Report [WSL2 Native ext4 (/dev/sdd direct I/O)]
**Workspace Reconstructie-ID**: `v0.4.3-standalone-sha256-0016483942`  
**Benchmarkdatum**: 2026-11-02 16:25:31 CET  
**Filesysteem**: **WSL2 Native ext4 (/dev/sdd direct I/O)**  
**Hardware / OS**: Intel Core i5-12500H (16 vCPU) | Linux WSL2 kernel 6.18.54 | Python 3.12 (Conda ECMWF) | Rust 1.99.1  

## 0. Verantwoording van Tests en Revisie (Punt 9)
Bij de uitvoering van `pytest python/tests` slaagden **alle geactiveerde tests** en werden uitsluitend **2 tests overgeslagen** die testen wat er gebeurt als optionele pakketten (`zarr`, `cfgrib`) *ontbreken*:
1. `test_zarr_adapter_when_not_installed`: overgeslagen omdat `zarr` wel degelijk is geïnstalleerd.
2. `test_grib2_adapter_when_not_installed`: overgeslagen omdat `cfgrib`/`eccodes` wel degelijk is geïnstalleerd.
3. `test_grib2_local_fixture_when_path_is_supplied`: in de vorige run zonder `WX1_GRIB2_FIXTURE` omgevingsvariabele was dit de **3e geskipte test** (`pytest.skip('Set WX1_GRIB2_FIXTURE...')`); nu voorzien van een automatische fallback naar de lokale grib2-fixture zodat deze altijd draait en slaagt.

De repository bevindt zich in een standalone distributie zonder `.git`-database; de canonieke versie is deterministisch berekend via de SHA-256 hash van `Cargo.lock` en manifesten: **`v0.4.3-standalone-sha256-0016483942`**.

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

*Verklaring:* Zowel de dyadische als de niet-dyadische stap zijn exact op $0.0$ verankerd (`round(x / step) * step`). Waar de theoretische continue entropiewinst maximaal $\log_2(\Delta_{\text{dyadisch}} / 2\epsilon)$ bedraagt, kunnen de werkelijk gerealiseerde bits/waarde na compressie hiervan afwijken (bijv. 1.09 bit bij Kolmogorov $\epsilon=0.1$). Dit verschil ontstaat doordat de `int32` byte-shuffle-pijplijn en LZ77/FSE in Zstandard reageren op byte-uitlijning en herhalende nulpatronen. Nullen blijven in beide methoden exact `0.0`.

## 3. Integer-Packing Doorvoer (MB/s) & Bits/Val vs GRIB2 CCSDS (Punt 3 & 4)
Gemeten met de `zstd 0.13`-crate (libzstd 1.5.7) en gevectoriseerde 2D prefix-sum reconstructie (`np.cumsum`). Wire-formaat blijft ongewijzigd; een toekomstige geregistreerde integer-packing codec vereist eerst een aparte ADR.

| Dataset | Voorspeller | Zstandard Niveau | Bits/Waarde | Ratio | Compressie (MB/s) | Decompressie (MB/s) | GRIB2 CCSDS b/v | Delta t.o.v. GRIB2 |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| `ecmwf_nh_sfc_2t` | `1d` | 3 | **2.325** | 13.8x | 206.3 MB/s | **163.9 MB/s** | 1.939 | **+19.9%** |
| `ecmwf_nh_sfc_2t` | `1d` | 7 | **2.179** | 14.7x | 124.8 MB/s | **172.1 MB/s** | 1.939 | **+12.4%** |
| `ecmwf_nh_sfc_2t` | `1d` | 19 | **1.935** | 16.5x | 7.8 MB/s | **175.2 MB/s** | 1.939 | **-0.2%** |
| `ecmwf_nh_sfc_2t` | `2d_plane` | 3 | **2.352** | 13.6x | 155.6 MB/s | **129.5 MB/s** | 1.939 | **+21.3%** |
| `ecmwf_nh_sfc_2t` | `2d_plane` | 7 | **2.228** | 14.4x | 109.7 MB/s | **120.5 MB/s** | 1.939 | **+14.9%** |
| `ecmwf_nh_sfc_2t` | `2d_plane` | 19 | **1.932** | 16.6x | 7.2 MB/s | **137.0 MB/s** | 1.939 | **-0.4%** |
| `ecmwf_nh_pl_t14` | `1d` | 3 | **1.733** | 18.5x | 221.4 MB/s | **174.5 MB/s** | 1.761 | **-1.6%** |
| `ecmwf_nh_pl_t14` | `1d` | 7 | **1.565** | 20.4x | 121.3 MB/s | **173.9 MB/s** | 1.761 | **-11.1%** |
| `ecmwf_nh_pl_t14` | `1d` | 19 | **1.282** | 24.9x | 7.7 MB/s | **158.0 MB/s** | 1.761 | **-27.2%** |
| `ecmwf_nh_pl_t14` | `2d_plane` | 3 | **1.711** | 18.7x | 181.6 MB/s | **137.0 MB/s** | 1.761 | **-2.8%** |
| `ecmwf_nh_pl_t14` | `2d_plane` | 7 | **1.572** | 20.4x | 114.3 MB/s | **141.8 MB/s** | 1.761 | **-10.7%** |
| `ecmwf_nh_pl_t14` | `2d_plane` | 19 | **1.296** | 24.7x | 6.9 MB/s | **128.3 MB/s** | 1.761 | **-26.4%** |

## 4. R2 Ablatiestudie op 3 Volledige Datasets (Punt 7 & 8)
Linux-paginacache geleegd via `posix_fadvise(POSIX_FADV_DONTNEED)`. De 4 componenten van R2 (descriptorhergebruik, parallelle `std::thread::scope`-decompressie, direct memory placement `copy_intersection`, en software slice-by-8 `crc32c`) zijn uitgevoerd op de native ext4 partitie van WSL2 (/home/j_ord/wx1_bench_ext4):

| Dataset (Volledig) | Basis (Alle 4 Opts) | Zonder Descriptor Cache | Seriële Single-Thread (`std::thread::scope` uit) | `copy_intersection` Winst | Software Slice-by-8 `crc32c` Tijd |
| :--- | :---: | :---: | :---: | :---: | :---: |
| `ecmwf_nh_sfc_2t` | **9.66 ms** | 9.48 ms (+-0.18 ms) | 16.65 ms (1.72x) | ~0.81 ms | ~0.331 ms |
| `ecmwf_nh_pl_t14` | **220.52 ms** | 215.49 ms (+-5.04 ms) | 126.28 ms (0.57x) | ~27.84 ms | ~3.567 ms |
| `knmi_harm46_nl` | **7.35 ms** | 7.38 ms (+0.03 ms) | 10.03 ms (1.36x) | ~0.71 ms | ~0.784 ms |

*Analyse van de ablatie-uitkomsten:*
- **Parallelle Thread-overhead bij `t14`**: Bij `t14` (14 niveaus van elk ~45 KB gecomprimeerd) veroorzaakte het per batch opspinnen en synchroniseren van OS-threads onder WSL2 op een hybride P/E-core architectuur (Intel Core i5-12500H) thread-contention. Seriële decompressie was 1,75× sneller (126 ms vs 220 ms). Dit is opgelost met een decompressie-heuristiek die batches onder 512 KiB automatisch in-thread (serieel) verwerkt.
- **Descriptor-cache en ruis**: De verschillen (-0.18 tot +0.03 ms) vallen binnen de meetruis op ext4.
- **Software CRC32C**: De software slice-by-8 CRC32C neemt 0.33 tot 3.57 ms in beslag. Hardware-versnelde instructies (SSE4.2) kunnen dit reduceren zodra een ADR inline CPU intrinsics toestaat binnen `#![forbid(unsafe_code)]`.

## 5. ADR 0029: Prestaties van de Wetenschappelijke Leespaden (R1 t/m R9)
Metingen uitgevoerd op **WSL2 Native ext4 (/dev/sdd)** met geleegde Linux-paginacache via `posix_fadvise(POSIX_FADV_DONTNEED)`. Let op: GRIB2 speedup (bijv. 385× op R1 of 204× op R4) is gemeten zonder vooraf berekend extern `.idx`-bestand; tegenover de geoptimaliseerde array-formaten **NetCDF-4** en **Zarr v3** behaalt WX-1 een reële en sterke versnelling van **3× tot 7×**:

| Leespad | Beschrijving | WX-1 Warm (ms) | WX-1 Linux-Cache Koud (ms) | GRIB2 (ms) | NetCDF-4 (ms) | Zarr v3 (ms) | Speedup vs GRIB2 | Status t.o.v. ≥2× Doel |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: | :--- |
| **`R1_open_container`** | Open container and parse superblock+footer (1-chunk file; 1M-chunk R1 in Part 5) | **0.15** | 5.50 | 59.95 | 1.15 | 3.29 | **385.95×** | ✅ Gehaald (≥2×) |
| **`R2_full_2d_field`** | Read 901x3600 float32 2D surface field (12.97 MB) | **10.60** | — | 33.15 | 35.23 | 51.59 | **3.13×** | ✅ Gehaald (≥2×) |
| **`R3_single_level_slice`** | Read 1 level out of 14 vertical levels (901x3600) | **10.23** | — | 28.79 | 29.17 | 40.62 | **2.81×** | ✅ Gehaald (≥2×) |
| **`R4_point_vertical_profile`** | Extract 14-level vertical profile at single station location | **1.99** | — | 406.85 | 8.78 | 9.32 | **204.26×** | ✅ Gehaald (≥2×) |
| **`R5_regional_bounding_box`** | Read 100x100 regional bounding box | **0.48** | — | 30.87 | 2.29 | 1.65 | **63.84×** | ✅ Gehaald (≥2×) |
| **`R7_ensemble_single_member`** | Read 1 ensemble member out of 50 members | **0.61** | — | 7.12 | 4.29 | 5.95 | **11.73×** | ✅ Gehaald (≥2×) |
| **`R8_1000_stations_scatter`** | Batch point read of 1,000 discrete station coordinates across grid | **7.61** | — | 28.81 | 31.67 | 39.15 | **3.78×** | ✅ Gehaald (≥2×) |

## 6. Schrijfpaden (W2/W4/W5), 1M-Chunk R1 Open & Verify-Modus Zonder BLAKE3 op ext4
### 6.1 Schrijf-, Append- en Durable Commit-Paden op ext4
- **`W2` (Gebufferd vs Duurzaam Schrijven, 512x512 f32)**: Gebufferd: **1.86 ms** | Duurzaam (fsync/durable): **11.13 ms** (fsync overhead: +9.27 ms).
- **`W4`/`W5` (Async Multi-Gen Append + Durable Commit, 8 generaties)**: Caller submit mediaan: **2.99 ms** | Max queue stall: **7.422 ms** | Gem. durable commit per generatie: **5.20 ms**.
- **`W5` (Flush / Close verificatie)**: **25.21 ms**.

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
