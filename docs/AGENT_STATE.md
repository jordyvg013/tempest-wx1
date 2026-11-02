# WX-1 Agent State

- **Datum:** 2026-11-02
- **Huidige fase:** Fase 4 (Onderdeel 1: ADR 0028 + Onderdeel 2: ADR 0024 Lossy Quantisatie & Byte-Shuffle) VOLTOOID & GEVALIDEERD.
- **Projectlocatie:** `/mnt/d/AI/weer-ai/Tempest/WX-1`
- **Test- en scratchlocatie (ext4):** `/home/j_ord/WX-1-TESTS`
- **Conda-omgeving:** `Tempest` (`/home/j_ord/miniconda3/envs/Tempest`, Python 3.12.16)
- **Rust toolchain:** rustc 1.99.1, cargo 1.99.1
- **GPU:** NVIDIA GeForce RTX 3060 Laptop GPU (6144 MiB VRAM), driver 615.78.05
- **acceptance.yaml SHA256:** `faa17ac1919a333ae873140272a93161af96508d914475cf600725270793fdb7` (onveranderd)

## Fase 4 Mijlpaal 1: ADR 0028 Geïndexeerde Array Planner (2026-11-02)

### Schaling van de Array Planner: Lineair ($O(N)$) vs Geïndexeerd ($O(k)$)
Gemeten met 100 herhalingen op een release-build voor een slice die exact $k = 4$ chunks snijdt:

| Aantal Chunks in Raster | Rasterindeling | Lineaire Planner ($O(N)$) | ADR 0028 Geïndexeerde Planner ($O(k)$) | Versnelling |
|---|---|---:|---:|---:|
| **4.096 chunks** | $64 \times 64$ | 5.000 ns (5,00 µs) | **244 ns** (0,24 µs) | **20× sneller** |
| **65.536 chunks** | $256 \times 256$ | 167.000 ns (167,00 µs) | **225 ns** (0,23 µs) | **742× sneller** |
| **1.048.576 chunks** | $1024 \times 1024$ | 6.682.000 ns (6.682,00 µs / **6,68 ms**) | **205 ns** (0,21 µs / **0,0002 ms**) | **32.500× sneller** |

### Benchmark Resultaten na ADR 0028 & ADR 0023 (Release Build, 30 herhalingen)

| Dataset | Formaat | Grootte (1 veld) | R1 Open (ms) | R2 Vol Veld (ms) | R5 Regio Box (ms) | Max Fout (K) | Bit-exact |
|---|---|---:|---:|---:|---:|---:|:---:|
| **harmonie_nl** (390x390) | ZARR3 | 252,694 B | 6.101 | 6.906 | 8.171 | 0.000000 | Ja |
| **harmonie_nl** (390x390) | NETCDF4 | 619,712 B | 2.560 | 0.254 | 0.327 | 0.000000 | Ja |
| **harmonie_nl** (390x390) | GRIB2 (1 veld) | 305,107 B | 3.865 | 2.194 | 2.345 | 0.000000 | Ja |
| **harmonie_nl** (390x390) | **WX1 (ADR 0028)** | 268,710 B | **0.194** | **1.780** | **0.459** | 0.000000 | **Ja** |
| **ecmwf_oper_europe** (261x661) | ZARR3 | 102,993 B | 5.979 | 6.532 | 8.317 | 0.000000 | Ja |
| **ecmwf_oper_europe** (261x661) | NETCDF4 | 701,964 B | 2.465 | 0.270 | 0.334 | 0.000000 | Ja |
| **ecmwf_oper_europe** (261x661) | GRIB2 (CCSDS, 1 veld) | 75,269 B | 3.988 | 3.757 | 3.903 | 0.000000 | Ja |
| **ecmwf_oper_europe** (261x661) | **WX1 (ADR 0028)** | 104,890 B | **0.198** | **1.543** | **0.254** | 0.000000 | **Ja** |

## Samenvatting Fase 4 Onderdeel 1:
1. **O(k) Planner:** `plan_array_slice` en `read_array_slice` rekenen nu rechtstreeks via coördinaat-strides de benodigde sub-grid chunk-indexen uit in plaats van $N$ chunks te scannen.
2. **Schaalbaarheid:** Bij 1.048.576 chunks duurt plannen nu **205 nanoseconden** in plaats van **6,68 milliseconden**.
3. **Veiligheid en Integriteit:** 100% safe Rust in `crates/wx1-*`, 119 Rust workspace tests geslaagd, Clippy 0 warnings, `cargo fmt` clean, 23 Python tests geslaagd. Bit-exacte resolutie gegarandeerd met transparante fallback naar lineaire scan bij niet-reguliere rasters.

## Fase 4 Mijlpaal 2: ADR 0024 Lossy Quantisatie & Byte-Shuffle (2026-11-02)

### Gescheiden Resultaten: Lossless (`restart`) vs Lossy (`output`) op 50-Member Ensemble Cubes (34,50 MB raw float32 per kubus)

#### Tabel A — Lossless (`restart`-profiel, bit-exact)
| Dataset (50×261×661) | Variant | Grootte (B) | % van Raw | % van GRIB2_CCSDS | Schrijf (ms) | Open med/p95 (ms) | Lees 50-lid med/p95 (ms) | Lees 1-lid med/p95 (ms) | Bit-exact |
|---|---|---:|---:|---:|---:|---:|---:|---:|:---:|
| **ENFO sfc `2t`** | WX-1 `restart` (Zstd) | 4.769.368 | 13,82% | 127,42% | 138,85 | 0,181 / 0,532 | 74,51 / 77,60 | 1,150 / 1,616 | **Ja** |
| **ENFO sfc `2t`** | WX-1 `restart` (ByteShuffle+Zstd) | **3.221.962** | **9,34%** | **86,08%** | 127,44 | 0,167 / 0,321 | 65,82 / 74,66 | 1,104 / 1,456 | **Ja** |
| **ENFO pl `t@500`** | WX-1 `restart` (Zstd) | 4.296.468 | 12,45% | 159,55% | 132,25 | 0,164 / 0,258 | 69,64 / 73,75 | 1,089 / 1,620 | **Ja** |
| **ENFO pl `t@500`** | WX-1 `restart` (ByteShuffle+Zstd) | **2.896.989** | **8,40%** | 107,58% | 112,89 | 0,174 / 0,270 | 66,10 / 70,55 | 1,086 / 1,233 | **Ja** |
| **Continu f32 model** | WX-1 `restart` (Zstd) | 26.750.942 | 77,53% | n.v.t. | 236,01 | 0,164 / 0,269 | 118,09 / 126,93 | 2,074 / 2,341 | **Ja** |
| **Continu f32 model** | WX-1 `restart` (ByteShuffle+Zstd) | **18.529.876** | **53,70%** | n.v.t. | 152,08 | 0,170 / 0,191 | 95,01 / 103,94 | 1,729 / 1,880 | **Ja** |

#### Tabel B — Lossy (`output`-profiel, gegarandeerde foutgrens $\epsilon = 0,01\text{ K}$)
| Dataset (50×261×661) | Variant | Grootte (B) | % van Raw | % van GRIB2_CCSDS | Max Abs Fout (K) | RMSE (K) | Bias (K) | Grens $\le 0,01$ K |
|---|---|---:|---:|---:|---:|---:|---:|:---:|
| **ENFO sfc `2t`** | GRIB2_CCSDS referentie (9-bit, $\epsilon = 0,03125\text{ K}$) | 3.742.930 | 10,85% | 100,00% | $\le 0,031250$ | — | — | n.v.t. |
| **ENFO sfc `2t`** | WX-1 `output` ($\epsilon = 0,01\text{ K}$, ByteShuffle+Zstd) | **3.222.885** | **9,34%** | **86,11%** | **0,007629** | 0,004552 | 0,000192 | **Ja** |
| **ENFO pl `t@500`** | GRIB2_CCSDS referentie (9-bit, $\epsilon = 0,03125\text{ K}$) | 2.692.878 | 7,80% | 100,00% | $\le 0,031250$ | — | — | n.v.t. |
| **ENFO pl `t@500`** | WX-1 `output` ($\epsilon = 0,01\text{ K}$, ByteShuffle+Zstd) | 2.839.006 | 8,23% | 105,43% | **0,007660** | 0,004628 | 0,000236 | **Ja** |
| **Continu f32 model** | WX-1 `output` ($\epsilon = 0,01\text{ K}$, ByteShuffle+Zstd) | **8.814.215** | **25,55%** | n.v.t. | **0,007812** | 0,004510 | 0,000017 | **Ja** |

### Verificatie & Tests:
- **125 Rust workspace tests** geslaagd (`cargo test --workspace`), 0 warnings (`cargo clippy --workspace --all-targets -- -D warnings`), `cargo fmt --all -- --check` clean.
- **25 Python pytest tests** geslaagd (`pytest python/tests`).
- **Alle geconverteerde testbestanden (`.nc`, `.zarr`, `.wx1`, `.grib2`) staan uitsluitend op de D-schijf** in de `test_formats`-submappen van de drie brondatasets (`/mnt/d/Weer/Uitvoer/data/...`), 0 B aan testdata op de WSL C-schijf.
- Volledige benchmarkresultaten van alle 5 datasets (`ecmwf_nh_sfc_2t`, `ecmwf_nh_pl_t14`, `knmi_harm46_2t_12steps`, `ecmwf_enfo_sfc_2t`, `ecmwf_enfo_pl_t500`) zijn opgeslagen in `bench/results/d_drive_all_datasets_benchmark.json`.


