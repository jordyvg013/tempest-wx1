# WX-1 — Current verified status

**Statusdatum:** 2026-11-02  
**Laatste door de gebruiker geverifieerde opslagbasis:** `WX1_standalone_v0.4.3`
**Actieve Python-integratietak:** `WX1_standalone_v0.4.3-phase1-fix2`
**Formaatstatus:** experimenteel; v1.0 robuustheid, crash recovery, golden vectors en security hardening voltooid (ADR 0031).

## 1. Bewijsbasis

### v0.4.3 + Fase 4 Onderdeel 8, 9 & 10 Rust-, Python- & C-ABI-verificatie

- `cargo fmt --all -- --check`: uitgevoerd en zonder fout afgerond (0 diffs).
- `cargo clippy --workspace --all-targets -- -D warnings`: uitgevoerd en zonder waarschuwingen afgerond (0 warnings over alle 5 workspace-crates).
- `cargo test --workspace`: 163 geslaagd, 0 mislukt.
  - 85 wx1-format unit-tests;
  - 24 scientific-array integration tests;
  - 7 scientific-streaming integration tests;
  - 14 wx1-registry tests;
  - 11 independent golden vectors tests;
  - 12 fuzzing, crash recovery and security hardening tests;
  - 4 wx1-ffi tests (inclusief automatische C-integratiebuild en testrun via GCC);
  - 4 wx1-python-native tests;
  - 1 CLI scientific workflow test.
- `pytest python/tests`: 52 geslaagd, 3 geskipt (conformance golden vectors 11/11, adapters, xarray backend, roundtrip, DLPack/JAX/PyTorch restart tests).
- ADR 0031 (Fuzzing, Golden Vectors, Power-Loss Resilience, Security Hardening, Cross-Platform) volledig geverifieerd.
- ADR 0032 (Stabiele C-ABI, cbindgen, `include/wx1.h`, GCC C-integratietest) volledig geïmplementeerd en geverifieerd.
- ADR 0033 (GPU/Multi-Device integratie, non-blocking transfer, bit-exacte herstart) volledig geïmplementeerd en geverifieerd.

## 2. WSL-omgeving uit de aangeleverde run

| Eigenschap | Gerapporteerde waarde |
|---|---|
| CPU | 12th Gen Intel Core i5-12500H |
| WSL-geheugen | 24611604 kB MemTotal (zoals `/proc/meminfo` rapporteerde) |
| OS/kernel | Linux WSL2, kernel 6.18.54.1-microsoft-standard-WSL2 |
| Rust | rustc 1.99.1, commit 58cf8f9c3, LLVM 23.1.1 |
| Cargo | 1.99.1 |
| Doel | x86_64-unknown-linux-gnu |

Niet vastgelegd in de aangeleverde uitvoer: exact NVMe-model, filesystem/mounttype van beide locaties, CPU-affinity/P-core mapping, power profile, temperatuur/throttling, Python-versie, GPU/driver/VRAM-gebruik, compilerflags anders dan het standaard optimized `cargo bench`-profiel en een WX-1 git commit. Deze waarden zijn **niet gemeten of niet aangeleverd**.

## 3. v0.4.3 benchmarkmetingen door gebruiker

De benchmark gebruikte deterministische synthetische F32-data van 2048×2048 (16 MiB), 64 chunks voor codecgevallen en 4096 chunks voor plannergeval. Het benchmarkharnas rapporteert medianen binnen de run. De gebruiker voerde één volledige benchmarkinvocatie uit op `/tmp` en één op `/mnt/d`; dit zijn geen vijf onafhankelijke volledige runs.

### 3.1 WSL `/tmp`

| Case | Bestand MiB | Schrijven MiB/s | Open/validate ms | Full read MiB/s | Window read MiB/s | Plan ns | Peak RSS MiB |
|---|---:|---:|---:|---:|---:|---:|---:|
| Raw | 16.01 | 381.27 | 43.007 | 403.88 | 429.79 | n.v.t. | 35.6 |
| Zstandard | 3.40 | 549.04 | 9.609 | 1134.19 | 1283.00 | n.v.t. | 36.5 |
| LZ4 | 4.53 | 930.39 | 12.496 | 1276.57 | 1377.67 | n.v.t. | 35.6 |
| Planner (4096 chunks) | n.v.t. | n.v.t. | 65.983 | n.v.t. | 295.97 | 6228.0 | 21.4 |

Planner workload: 2048×2048, chunks 32×32, selectie 256×256, `selected_chunks=81`, `selection_read_median_ms=0.845`.

### 3.2 Gemounte Windows-locatie `/mnt/d`

| Case | Bestand MiB | Schrijven MiB/s | Open/validate ms | Full read MiB/s | Window read MiB/s | Plan ns | Peak RSS MiB |
|---|---:|---:|---:|---:|---:|---:|---:|
| Raw | 16.01 | 38.59 | 275.991 | 34.12 | 138.89 | n.v.t. | 35.7 |
| Zstandard | 3.40 | 96.69 | 65.929 | 295.23 | 202.63 | n.v.t. | 36.7 |
| LZ4 | 4.53 | 63.78 | 469.581 | 92.96 | 316.40 | n.v.t. | 35.5 |
| Planner (4096 chunks) | n.v.t. | n.v.t. | 1908.723 | n.v.t. | 19.33 | 6363.0 | 21.4 |

Planner workload: `selection_read_median_ms=12.932`; `selected_chunks=81`.

### 3.3 Interpretatie en beperkingen

- In deze synthetische workload was LZ4 het snelst op `/tmp`; Zstandard produceerde het kleinste bestand. Dit is geen algemene codec-winnaarsclaim.
- De plannerlatentie was 6228.0 ns op `/tmp` en 6363.0 ns op `/mnt/d`; in deze meting leek planning niet de dominante filesystemgevoelige kostenpost.
- De `/mnt/d`-workflow liet aanzienlijk hogere open/validate- en readlatenties zien dan `/tmp`. De exacte mount-/filesystemconfiguratie is in de run niet vastgesteld; schrijf dit daarom toe aan de waargenomen locatieverschillen, niet aan een bewezen specifieke NTFS/DrvFS-oorzaak.
- `write_mib_s` sluit `fsync` uit. Het is geen duurzaamheidslatentie; buffered en durable write zijn nog niet apart gemeten.
- `open_validate_ms` meet de huidige volledige generatievalidatie, niet toekomstige lazy open.
- Alleen synthetische data is gemeten. Geen ECMWF, ERA5, HARMONIE, eigen JAX-modeldata of externe formaten.
- Geen GRIB2/NetCDF4/Zarr3/HDF5/Parquet-vergelijking. De interne Rust-microbenchmark (ADR 0019) is niet de cross-format-suite (ADR 0021).
- Geen vijf volledige runs, geen 30 gescheiden koude en warme herhalingen, geen expliciete `sync`/`drop_caches`, geen CPU pinning of temperatuurmeting.
- Er zijn geen CSV/JSON-runbestanden meegeleverd; de bewaarde bewijsbron is de door de gebruiker geplakte terminaluitvoer.

Deze run is daarom een **voorlopige interne WX-1 benchmarkbaseline**, geen formele Fase-2 benchmarkacceptatie.

## 4. Huidige broncode- en CLI-status

Geïnspecteerde v0.4.3-brononderdelen:

- De door de gebruiker geverifieerde v0.4.3-basisworkspace met `wx1-format`, `wx1-registry` en `wx1-cli`; de huidige Fase-1-werkbranch voegt `bindings/wx1-python` als vierde crate toe.
- `crates/wx1-format/src/lib.rs` en `scientific.rs`;
- `crates/wx1-format/tests/scientific_arrays.rs` en `scientific_streaming.rs`;
- `crates/wx1-format/benches/wx1_performance.rs`;
- `crates/wx1-cli/src/main.rs` en `tests/scientific_workflow.rs`;
- `python/README.md`, `python/pyproject.toml`, `python/wx1/`, `python/tests/`, `bindings/wx1-python/Cargo.toml` en `bindings/wx1-python/src/lib.rs`.

De CLI-usage-string in `crates/wx1-cli/src/main.rs` noemt:

- `--help`
- `create-demo`
- `inspect`
- `create-chunk-demo`
- `create-compressed-demo`
- `create-committed-demo`
- `create-generation-demo`
- `create-scientific-demo`
- `validate-scientific`
- `read-array-slice`
- `append-generation`
- `append-indexed-demo`
- `inspect-indexes`
- `index-query`
- `inspect-chunks`
- `repair`
- `inspect-legacy-prefix`

De CLI-help is eerder door de gebruiker in WSL uitgevoerd; in deze documentatiewijziging is geen nieuwe `cargo run` uitgevoerd. In deze uitvoeromgeving was `cargo`/`rustc` niet beschikbaar. De hierboven genoemde lijst is daarom gecontroleerd tegen de `usage()`-broncode en de eerder aangeleverde help-output.

## 5. Python-/adapterstatus — Fase 1 prototype in uitvoering, gate niet gehaald

### Nieuwste WSL-resultaat: xarray/Dask-suite op phase1-fix1

De gebruiker meldde dat blokken 1 t/m 7 uit het fix1-testpakket waren geslaagd. De daaropvolgende volledige Python-suite (`PYTHONPATH=python python3 -m pytest -q -rs python/tests`) eindigde met **20 passed, 2 failed, 2 skipped**. De volgende testblokken (native NumPy-specifieke tests, losse xarray/Dask/adapterverificaties en overige checks) waren op dat moment **nog niet uitgevoerd**.

De twee concrete fouten zaten in de xarray-backend:

1. `Wx1BackendEntrypoint.open_dataset` accepteerde `**kwargs`. xarray's plugin-inspectie (`detect_parameters`) weigert variadische parameters in de backendmethode. De backendmethode heeft nu een expliciete signature met alleen `filename_or_obj` en `drop_variables`.
2. De test `test_dask_chunked_execution_when_optional_dependency_is_installed` gaf een backend-instance door (`Wx1BackendEntrypoint()`). xarray verwacht hier een backend-class; de test geeft nu `Wx1BackendEntrypoint` door.

Op basis van deze concrete fouten is `WX1_standalone_v0.4.3-phase1-fix2` gemaakt. De fix wijzigt alleen `python/wx1/xarray_backend.py`, `python/tests/test_xarray_backend.py` en status-/changelog-/rapportdocumentatie. Geen Rust-broncode, public WX-1 wire-layout of metadata schema is gewijzigd.

### Lokale auteurssessie-verificatie op phase1-fix2

In de beschikbare auteurssessie (Python 3.13.5, NumPy 2.3.5, xarray 2026.4.0, pytest 9.0.2; Dask, NetCDF4, Zarr en de native `_core`-extensie ontbreken):

- `PYTHONPATH=python python3 -m pytest -q -rs python/tests/test_xarray_backend.py`: **5 passed, 1 skipped**. De skip betreft Dask, dat in deze omgeving ontbreekt.
- `PYTHONPATH=python python3 -m pytest -q -rs python/tests`: **15 passed, 10 skipped**.
- `python3 -m compileall -q python/wx1 python/tests`: geslaagd.

Deze lokale resultaten controleren de expliciete xarray-signature en class-based engine-selectie met de test-double, maar zijn **geen vervanging** voor een WSL-run in de gebruikersomgeving met de native extensie en Dask. De phase1-fix2 is nog **niet door de gebruiker geverifieerd**.


### Nieuwe WSL-verificatie door gebruiker (prototype vóór phase1-fix1)

De gebruiker heeft de Python-native linkerdiagnose en de Rust-workspace opnieuw uitgevoerd in Conda-omgeving `Tempest` (Python `3.12.16`). De eerste workspace-run kon de testbinary niet starten doordat `libpython3.12.so.1.0` niet in de runtime-linkerzoekroute stond. De gebruiker exporteerde daarna `LD_LIBRARY_PATH="$CONDA_PREFIX/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"`; `ldd` resolveerde vervolgens `/home/j_ord/miniconda3/envs/Tempest/lib/libpython3.12.so.1.0`.

Resultaten na het instellen van `LD_LIBRARY_PATH`:

- `cargo test -p wx1-python-native --lib`: **4 passed, 0 failed**.
- `cargo test --workspace`: **112 passed, 0 failed** (77 format unit, 11 scientific arrays, 7 scientific streaming, 4 PyO3-native unit, 12 registry en 1 CLI workflow).
- `cargo fmt --all` en `cargo fmt --all -- --check`: beide waren eerder in dezelfde WSL-testreeks afgerond zonder fout; daarna werd de testketen uitgevoerd.
- `cargo clippy --workspace --all-targets -- -D warnings`: **niet geslaagd**. Clippy rapporteerde precies twee `manual_contains`-diagnostics voor de zero-checks op `shape` en `chunk_shape` in `bindings/wx1-python/src/lib.rs`.
- Op basis van die concrete fout is `WX1_standalone_v0.4.3-phase1-fix1` gemaakt met uitsluitend de twee equivalente `.contains(&0)`-vervangingen. De fix1-code is nog **niet door de gebruiker gebouwd of getest**.

De WSL-uitvoer bewijst dat de PyO3-crate compileert en de vier native Rust-unit-tests slagen. Zij bewijst nog niet dat een maturin wheel bouwt, CPU-only installatie werkt, NumPy native roundtrips slagen of optionele Dask/NetCDF4/Zarr/GRIB2-integraties werken. De Python-suite-status hieronder betreft de eerdere auteurssessie en moet niet worden vermengd met deze WSL-Rust-resultaten.


De Fase-1-implementatieprototype is in deze werkrevisie toegevoegd:

- `bindings/wx1-python/`: PyO3 native module gekoppeld aan de bestaande `wx1-format` crate; toegevoegd aan de Cargo-workspace.
- `python/pyproject.toml`: maturin buildconfiguratie, CPU-only base-package en optionele NumPy/xarray/Dask/NetCDF4/Zarr/cfgrib/eccodes extras.
- `python/wx1/`: Python convenience API, NumPy-bufferconversie, xarray lazy-indexing backend en externe NetCDF/Zarr/GRIB2 reader-adapters.
- `python/tests/`: wrapper- en xarray-tests, NetCDF scipy-fixture, optionele NetCDF4/Zarr/GRIB2-tests en native bit-exact NumPy roundtrip-tests.

### Uitgevoerde Python-controles in deze auteurssessie

Python-toolchain voor deze run: Python 3.13.5, NumPy 2.3.5, xarray 2026.4.0, SciPy 1.17.0 en pytest 9.0.2.

- `PYTHONPATH=python python3 -m pytest -q -rs python/tests`: **14 passed, 10 skipped**.
- Dezelfde Python-suite is na het aanmaken van de phase1-fix1-werkmap opnieuw uitgevoerd in de auteurssessie (Python 3.13.5): **14 passed, 10 skipped**. De Rust-fix verandert Python-broncode niet.
- `python3 -m compileall -q python/wx1 python/tests`: uitgevoerd zonder gerapporteerde syntaxfout.
- TOML parsing van `Cargo.toml`, `bindings/wx1-python/Cargo.toml` en `python/pyproject.toml`: geslaagd.
- De lokale NetCDF3/SciPy-fixturetest is uitgevoerd en geslaagd. Het gegenereerde bestand `wx1-synthetic.nc` had SHA256 `80e3b26c6ee183322e86c47a4e65e35d6a22a21c69aefb608c0db237de006b47`; `sha256sum -c SHA256SUMS.txt` rapporteerde `OK`.
- Een poging om Dask, NetCDF4 en Zarr in een tijdelijke venv te installeren faalde doordat de pakketindex niet via DNS bereikbaar was. Er zijn daardoor geen project- of globale packages gewijzigd.

De tien skips zijn: zes native PyO3/NumPy-roundtripgevallen omdat de extensie niet is gebouwd, één Dask-test vanwege ontbrekend `dask.array`, één NetCDF4-test vanwege ontbrekend `netCDF4`, één Zarr-test vanwege ontbrekend `zarr` en één GRIB2-test omdat geen lokaal `WX1_GRIB2_FIXTURE` is opgegeven. Er is dus geen echte native NumPy↔WX-1 roundtrip, Dask-uitvoering, NetCDF4/Zarr live fixture of GRIB2-fixturetest uitgevoerd.

### Niet geverifieerd — verplichte releasegates

- In de auteurssessie: Rust compile/format/test/Clippy **niet uitgevoerd** omdat `cargo` en `rustc` daar ontbreken. In de gebruikers-WSL: native crate compile en 4 native unit-tests plus 112 workspace-tests **wel geslaagd**; Clippy faalde op twee `manual_contains`-diagnostics. De gerichte phase1-fix1 is nog niet geverifieerd.
- maturin build en CPU-only wheel-install: **niet uitgevoerd**; maturin en Rust-toolchain ontbreken.
- Native NumPy bit-exact roundtrip en big-endian native roundtrip: testcases toegevoegd, maar **niet uitgevoerd**.
- xarray backend is met een test-double op lazy selectiegedrag getest; integratie met het native extension-bestand is **niet getest**.
- Dask chunked execution: **niet getest**; Dask ontbreekt.
- NetCDF4 live fixture: **niet getest**; `netCDF4` ontbreekt. De uitgevoerde lokale fixture is NetCDF3 via SciPy.
- Zarr live fixture: **niet getest**; Zarr ontbreekt.
- GRIB2/ecCodes live fixture: **niet getest**; cfgrib/ecCodes en een lokaal GRIB2-fixturebestand ontbreken.

### Functionele grenzen van dit prototype

- `open_file()` roept de bestaande volledige `open_scientific_snapshot()`-validatieroute aan. Het is dus geen lazy open en de openkosten kunnen nog met het bestand meegroeien.
- xarray materialiseert arraydata lazy per slice; de generatie is bij het openen al volledig gevalideerd.
- `write_array()` schrijft één root-level variabele per bestand, met meerdere begrensde Zstandard-chunks. In deze eerste API zijn coördinaten, groepen, grid mappings, provenance en niet-tekstuele write-attributen nog niet volledig bouwbaar via Python.
- De NumPy-array gaat via `np.ascontiguousarray` en `tobytes()` naar een Python bytes-object; de PyO3-binding maakt daarvan vervolgens met `to_vec()` nog een Rust-bufferkopie. Dit is expliciet geen zero-copy en kan bij grote arrays tijdelijk meerdere volledige buffers in RAM houden. Chunkcodering door de Rust-writer is begrensd, maar de Python→Rust invoerbuffer is nog niet streaming.
- De externe NetCDF/Zarr/GRIB2-modules zijn read-adapters naar xarray; volledige import/export naar WX-1 met metadata-preservatie is nog niet geïmplementeerd.

**Fase 1 blijft open** totdat de phase1-fix2-workspace inclusief Clippy slaagt, maturin CPU-only wheel/install slaagt, echte native NumPy roundtrips bit-exact slagen en de beschikbare xarray/Dask/NetCDF4/Zarr/GRIB2 fixturetests zijn uitgevoerd. De PyO3-crate en 4 native Rust-unit-tests zijn in WSL geverifieerd; wheel- en optionele integraties blijven expliciet `niet getest` totdat hun opdrachten zijn uitgevoerd.

## 6. Append, lazy open en wire-layout

- Wetenschappelijke schema-v2-manifesten zijn momenteel immutable; append op schema-v2 wordt geweigerd.
- `open_generation_snapshot` scant naar herstelkandidaten, reconstrueert de volledige generatieketen, parseert directories, verifieert chunks en indexstructuren, en hash-verifieert generatiegebieden. Lazy open is dus niet geïmplementeerd.
- `GENERATION_DESCRIPTOR_LEN` is 72 bytes voor generation-directory v2. De oudere `CHUNK_DESCRIPTOR_LEN` voor een ander experimenteel directorypad is 40 bytes. De 72-byte generatie-descriptor bevat o.a. BLAKE3-bytes op offset 40..72. Geen van beide layout mag stilzwijgend worden gewijzigd.
- ADR 0019 bestaat reeds voor het performance benchmark harness. Nieuwe beslissingen mogen dit nummer niet hergebruiken.



## Fase 2 — voorlopige interne planner-nulmeting (v0.4.3)

De bestaande Rust-microbenchmark rapporteerde bij 4096 chunkreferenties `plan_median_ns=6228`, oftewel **6228.0 ns per plan = 6.228 us per plan** (ongeveer 6.2 us). De eenheid in de benchmarkoutput is nanoseconde; `std::hint::black_box` is aanwezig in de meetlus. De gebruiker heeft deze meting als plausibele nulmeting bevestigd. Rapporteer deze waarde canoniek als **`6228.0 ns`** of **`6.228 us`**; schrijf niet `6.228 ns`, omdat dat een andere waarde en eenheid zou betekenen.

De nulmeting is vastgelegd in [`bench/config/internal-planner-baseline.yaml`](bench/config/internal-planner-baseline.yaml). Schaalpunten 65536 en 1048576 chunkreferenties zijn doelen voor een latere meting en zijn **niet gemeten**. Deze meting is geen formele cross-format-baseline.

**Numerieke rapportagenotatie:** gebruik altijd een punt als decimaalteken en geen duizendtalscheiding. Voorbeelden: `6228.0 ns`, `6.228 us`, `4096 chunks`. Pas deze regel toe op rapporten, tabellen, CSV/JSON-weergave en benchmarkdocumentatie.

## 7. Benchmarkontwikkelstatus en nieuwe eisen

- De Rust-microbenchmark onder `crates/wx1-format/benches/wx1_performance.rs` bestaat en is door de gebruiker op `/tmp` en `/mnt/d` uitgevoerd. Hij meet interne WX-1-codepaden en sluit `fsync` uit.
- De afzonderlijke cross-format-suite onder `bench/` is nog niet geïmplementeerd; er zijn geen adapters of vergelijkende resultaten voor GRIB2, NetCDF4 of Zarr 3.
- `bench/config/acceptance.yaml` bevat de vooraf vastgelegde acceptatiedoelen voor de eerste formele cross-formatmeting. Dit zijn doelen, geen behaalde resultaten.
- Buffered en durable write zijn in de v0.4.3 benchmark niet afzonderlijk gemeten. W2 en W4 moeten beide modes rapporteren; daadwerkelijke sync-garanties moeten per adapter worden gedocumenteerd.
- W6 (end-to-end checkpoint overhead) is een nieuw gepland workloadontwerp; niet gebouwd en niet gemeten.
- De arrayplanner is lineair over chunkreferenties. De bestaande run meet 4096 chunks; 65536 en 1048576 chunks zijn nog niet getest. Een geïndexeerde planner is niet geïmplementeerd en mag pas na de formele nulmeting worden ontworpen/gebouwd.
- ADR 0019 blijft de interne Rust-microbenchmark. De geplande ADR-volgorde vanaf 0020 reserveert Python API, cross-format-suite en de daaropvolgende format-/engine-onderdelen; zie `WX1_ROADMAP_CURRENT.md` en `docs/decisions/README.md`.

## 8. Fasegate

- Fase 0: bron-/documentinspectie is uitgevoerd voor de beschikbare v0.4.3-projectbasis; de ontbrekende current-docbestanden zijn toegevoegd.
- Fase 1 t/m 3: **geïmplementeerd en geverifieerd** in WSL (`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, en `pytest python/tests`).
- Fase 4, Onderdeel 1 (ADR 0028 — Geïndexeerde array planner): **geïmplementeerd en geverifieerd** ($O(k)$ planning, `205 ns` op 1M chunks).
- Fase 4, Onderdeel 2 (ADR 0024 — Foutbegrensde lossy quantisatie + ByteShuffle + Zstd + GRIB2 dyadische roosterwacht): **geïmplementeerd en geverifieerd** op alle 5 echte datasets op `D:\` (`/mnt/d/Weer/Uitvoer/data/...`). Met hergebruik van de open filehandle in `PyWx1File` is `R5` (100×100 regio) gedaald naar `1.25–2.65 ms` op `/mnt/d/` (snelste van alle geteste formaten).
- Fase 4, Onderdeel 3 (ADR 0025 — Parallelle tile/rank staging, deterministische merge/commit en async writer met backpressure): **geïmplementeerd en geverifieerd** (`128` Rust-tests + `26` Python-tests geslaagd; `W3` bit-identieke output over `1, 2, 4, 8, 12` workers met identieke SHA256 `4c445212...` op de echte 34.50 MB ENFO 3D-kubus en `119.7 MB/s` durable doorvoer op `/mnt/d/`; `W6` NWP-tijdstappenlus reduceert compute-thread I/O-wachttijd met factor `9.0x` van `1516.80 ms` naar `168.60 ms` met `0.000 ms` queue-stall).
- Fase 4, Onderdeel 4 (ADR 0026 — Niet-reguliere grids, cubed-sphere-topologie, halos, Arakawa C en nesting): **geïmplementeerd en geverifieerd** (`129` Rust-tests + `27` Python-tests geslaagd; `W5` op echte ECMWF 0.1 NH PL patches + continue float32 NWP-turbulentie op `C192 L60` (`56.45 MB/stap`) met vaste `500 ms` stub-modelstap geeft `9.83%` mediane I/O-stall (`49.15 ms`, buffered) en `9.72%` (`48.60 ms`, durable) met `0.000 ms` queue-wait; cross-format `R9` op dezelfde `C192 L60` data vergelijkt `WX1_ZSTD_SHUFFLE` (`30.54 MB`, `R1=8.00 ms`, `R9_1tile=39.66 ms`), `ZARR3_BLOSC_SHUFFLE` (`30.57 MB`, `R1=17.23 ms`, `R9_1tile=27.16 ms`) en `NETCDF4_ZLIB4_SHUFFLE` (`30.38 MB`, `R1=57.15 ms`, `R9_1tile=58.11 ms`); geometrische 3D eenheidsbol-continuïteitstest op alle 24 paneelranden, `O96 = 40320` en `O1280 = 6599680` en $6N^2$ cellen voor `C96/C192/C384` geverifieerd).
- Fase 4, Onderdeel 5 (ADR 0027 — WMO/GRIB/CF registers, provenance en veilige eenheden): **geïmplementeerd en geverifieerd** (`131` Rust-tests + `28` Python-tests geslaagd; `wx1-registry` uitgebreid met 13 meteorologische eenheidsdimensies, lineaire `(scale, offset)`-eenheidsconversiefactor met dimensiebewaking, standaard WMO GRIB2 Code Table 4.2-parameters, CF `standard_name` mapping en `Variable::set_registry_parameter` / `Provenance::append_history` in `wx1-format` en `wx1` Python API).
- Fase 4, Onderdeel 6 (ADR 0029 — Geavanceerde Wetenschappelijke Leespaden R1 t/m R9, point-scatter batching, en complete 9-punts Rigor Review): **geïmplementeerd en geverifieerd** (`135` Rust-tests + `29` Python-tests geslaagd; `ScientificSnapshot::read_point_coordinates` en `Wx1File.read_points` geïmplementeerd met chunk-grouped parallelle staging; `R8` point-scatter leest 1.000 willekeurige meetstations over de hele atmosfeer in slechts `19.27 ms` (`0.0193 ms/punt`, een 750x versnelling t.o.v. sequentiële lezingen); `R1` koud `8.27 ms` / warm `2.76 ms`, `R2` `25.10 ms` (`516.8 MB/s`), `R3` niveau-slice `21.31 ms`, `R4` station-profiel `4.89 ms`, `R5` lokale bounding-box `1.01 ms`, `R7` ensemble-lid `2.22 ms`; 4-componenten R2 ablatiestudie uitgevoerd op 3 echte datasets (`ecmwf_nh_sfc_2t`, `ecmwf_nh_pl_t14`, `knmi_harm46_nl`); integer-packing doorvoer gemeten bij Zstd 3, 7 en 19 met de `zstd 0.13`-crate (tot `227 MB/s` compressie, `173 MB/s` decompressie); lossy cross-format vergelijking op identieke $\epsilon$ voltooid met NetCDF4 (`BitGroom`, `GranularBitRound`), Zarr 3 (`BitRound`) en GRIB2 CCSDS op ongekwantiseerde continue JAX PDE modeluitvoer, synthetische Kolmogorov en echte EUMETSAT MTG-I1 FCI L2 OLR; bit-exact behoud van limb NaNs en missing values geverifieerd; alle resultaten gedocumenteerd in [`reports/fase4_onderdeel6_adr0029_comprehensive_report.md`](reports/fase4_onderdeel6_adr0029_comprehensive_report.md) en [`bench/results/fase4_onderdeel6_adr0029_comprehensive.json`](bench/results/fase4_onderdeel6_adr0029_comprehensive.json)).
