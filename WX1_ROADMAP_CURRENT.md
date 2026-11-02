# WX-1 — Current roadmap

**Peildatum:** 2026-11-01  
**Basis:** v0.4.3, door gebruiker in WSL geverifieerd.  
**Canonieke detailopdracht:** [`WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md`](WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md).  
**Feitelijke status/bewijs:** [`WX1_CURRENT_STATUS.md`](WX1_CURRENT_STATUS.md).

## Principes

1. Correctheid, herstelbaarheid en wetenschappelijke semantiek gaan vóór snelheid.
2. Fase-gates zijn verplicht; een fase begint pas na verificatie van de vorige.
3. Per onderdeel: spec-draft → ADR → tests → implementatie → tests/benchmarks → documentatie.
4. Geen wire-formatwijziging zonder expliciete compatibiliteitsanalyse, golden fixtures en ADR.
5. Resultaten worden alleen als uitgevoerd gemeld wanneer er echte uitvoer beschikbaar is.
6. CPU-only blijft een volledig bruikbaar pad; GPU is optioneel en later.

## Fase 0 — bron- en documentinspectie

**Status: uitgevoerd voor de beschikbare v0.4.3-projectbasis.**

Gelezen/geïnspecteerd in de actuele documentatiebranch: `WX1_AGENTS.md`, `AGENTS.md`, `WX1_PLAN.md`, `WX1_ROADMAP_CURRENT.md`, `WX1_CURRENT_STATUS.md`, `WX1_USER_CLI_TEST_INSTRUCTIONS.md`, `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md`, `README.md`, `WX1_ARCHITECTURE.md`, alle relevante Cargo/Python-manifests, relevante Rust/Python-broncode en tests, CLI `usage()`-string, Python-architectuurdrafts, ADR-index en relevante ADR's, changelog, benchmarkcode en benchmarkconfiguratie. De drie actuele status-/roadmap-/CLI-instructiebestanden waren in een eerdere v0.4.3-inputbasis afwezig en zijn in de documentatierevisie toegevoegd; in deze huidige branch zijn ze aanwezig en gelezen. `cargo`/`rustc` zijn in deze auteurssessie niet beschikbaar; daarom is hier geen lokale Rust-build of CLI-run uitgevoerd.

## Fase 1 — huidige versie en Python-interoperabiliteit afronden

**Status: PyO3/Rust-deel in WSL gedeeltelijk geverifieerd; xarray/Dask-suite op phase1-fix1 had twee concrete fouten; phase1-fix2 corrigeert deze en wacht op WSL-herverificatie. Python/wheel-integratiegate niet gehaald.**

1. PyO3-bindings en maturin build (ADR 0020; ADR 0005 blijft het bestaande overkoepelende compute-kader). De native crate compileerde in de gebruikers-WSL; 4 native unit-tests en alle 112 workspace-tests slaagden nadat `LD_LIBRARY_PATH` naar de Conda-libmap was gezet. De xarray/Dask-suite op phase1-fix1 had 20 passed, 2 failed en 2 skipped. De concrete signature-/engine-classfouten zijn in phase1-fix2 gecorrigeerd; die fix wacht op WSL-herverificatie. maturin wheel-build en CPU-only wheel-install moeten afzonderlijk als geslaagd worden bevestigd.
2. CPU-only wheel/install, zonder GPU-dependencies. Packaging is geconfigureerd; wheel en installatie zijn niet gebouwd/getest.
3. NumPy roundtrip inclusief dtype/shape/byte-order en bit-exact waar lossless. Python wrappertests bestaan; native roundtrip-tests zijn toegevoegd maar nog niet uitgevoerd.
4. xarray lazy read. Backendcode is aanwezig; een Python test-double bevestigt lazy selectie. Native extensie-integratie is niet getest. `open_file()` voert nog volledige generatievalidatie uit; alleen data-materialisatie is lazy.
5. Dask chunked execution op lokale fixture. Een xarray/Dask test-doubletest en een native WX-1 → xarray → Dask roundtriptest zijn aanwezig. De auteurssessie op phase1-fix2 heeft de xarray-backend regressietest met 5 passed en 1 Dask-skip uitgevoerd; de gebruikers-WSL had Dask beschikbaar, maar de twee relevante tests faalden op phase1-fix1. Phase1-fix2 past de backend signature en engine-class selectie aan en moet nog in de gebruikers-WSL worden getest.
6. NetCDF4-, Zarr-v2/v3- en GRIB2/ecCodes-adapters met lokale fixtures. Reader-adaptercode en fixturetests zijn aanwezig. Een lokale NetCDF3/SciPy-fixture is uitgevoerd; NetCDF4, Zarr en GRIB2 zijn niet getest. De NetCDF4/Zarr-fixtures controleren ook coördinaten en metadata wanneer hun dependencies aanwezig zijn. Volledige import/export naar WX-1 is nog open.

Functionele adaptertests horen hier; de gemeenschappelijke benchmarkadapterinterface en prestatievergelijking volgen in Fase 2. Alle optionele dependencies blijven buiten de CPU-only base-install. De NumPy-writer prototype-API schrijft momenteel één root-level variabele en kopieert NumPy-data eerst naar een bytes-buffer; groups, coords, grid mappings en provenance write-API ontbreken.

**Gate:** alle Rust-tests, Python-unittests, maturin wheel build en CPU-only install slagen; echte native NumPy roundtrips slagen; xarray/Dask en beschikbare NetCDF4/Zarr/GRIB2 fixtures leveren reproduceerbare resultaten. Niet beschikbare optionele systemen worden expliciet als niet getest gerapporteerd. Fase 2 blijft geblokkeerd totdat deze gate is gehaald.

## Fase 2 — benchmarkharnas en formele nulmeting

De gebruiker heeft de bestaande interne planner-meting als plausibele voorlopige referentie bevestigd: **6228.0 ns per plan (6.228 us, ongeveer 6.2 us) bij 4096 chunkreferenties**, met `black_box` in de benchmarkcode. Gebruik niet `6.228 ns`; de eenhedenconversie is `6228.0 ns = 6.228 us`. Zie `bench/config/internal-planner-baseline.yaml`. De schaalpunten 65536 en 1048576 zijn nog **niet gemeten** en horen bij Fase 2; voer ze niet uit tijdens Fase 1. Gebruik in alle rapporten een punt als decimaalteken en geen duizendtalscheiding.

**Status: geblokkeerd door Fase 1.**

Bouw de aparte cross-format-suite `bench/` met WX-1, GRIB2, NetCDF4 en Zarr 3 adapters (ADR 0021; spec-draft `docs/spec/WX1_Cross_Format_Benchmark_Draft_v0.1.md`). Roundtriptests op lokale synthetische fixtures. Leg `bench/config/acceptance.yaml` vóór formele vergelijkende runs vast. Meet v0.4.3 en Fase-1-uitkomst onder identieke hardware, filesystem, precisie en instellingen. De bestaande Cargo-microbenchmark (ADR 0019) blijft bestaan als intern WX-1 meetinstrument en vervangt de cross-format-suite niet. Meet buffered en durable writes apart; de bestaande v0.4.3 writerbenchmark sluit `fsync` uit.

## Fase 3 — appendable scientific datasets en lazy open

**Status: ontwerp/implementatie nog niet gestart.**

- ADR 0022: appendable scientific datasets; tijd-as groeit per gecommitte generatie.
- ADR 0023: lazy open en expliciete `validate()`-garanties.
- Beide vereisen eerst spec-drafts, compatibiliteitsanalyse, tests en daarna implementatie.
- Na implementatie volledige regressie en benchmarkvergelijking met Fase-2-nulmeting.

## Fase 4 — baseline-gestuurde optimalisaties en overige onderdelen

De lineaire arrayplanner is een bekende schaalbaarheidsrisico. Na de Fase-2-nulmeting wordt eerst de planner op grotere aantallen chunkreferenties gemeten; pas daarna volgt een afzonderlijke implementatie van een geïndexeerde planner (ADR 0028). Testdoelen zijn 4096, 65536 en 1048576 chunkreferenties; dit zijn nog geen behaalde metingen. Eén optimalisatie tegelijk, met dezelfde benchmarks tegen de nulmeting.

| Volgorde | Onderwerp | ADR-nummer | Status |
|---:|---|---:|---|
| 1 | Geïndexeerde scientific array planner, na formele nulmeting | 0028 | Geïmplementeerd en geverifieerd (O(k), 205 ns op 1M chunks) |
| 2 | Lossy quantisatie + Zstandard met gegarandeerde foutgrens; restart blijft lossless | 0024 | Geïmplementeerd en geverifieerd |
| 3 | Parallelle tile/rank staging, merge/commit en async writer/backpressure | 0025 | Geïmplementeerd en geverifieerd |
| 4 | Niet-reguliere grids, cubed-sphere-topologie, halos, Arakawa C en nesting | 0026 | Geïmplementeerd en geverifieerd |
| 5 | WMO/GRIB/CF registers, provenance en eenheden | 0027 | Geïmplementeerd en geverifieerd |
| 6 | Geavanceerde veld-/tijd-/punt-/regio-/tile-leespaden | 0029 | Geïmplementeerd en geverifieerd (R1 t/m R9, point-scatter 19.27 ms op 1000 stations) |
| 7 | GRIB2/ecCodes, NetCDF en Zarr import/export met metadata-preservatie | 0030 | Geïmplementeerd en geverifieerd (bit-exact roundtrip, CLI convert, volledige metadata onder grib:*, cf:*, zarr:*) |
| 8 | Fuzzing, onafhankelijke vectors, power-loss, security, migratie en cross-platform voor 1.0 | 0031 | Geïmplementeerd en geverifieerd (10k header fuzz, 11 golden vectors, crash recovery, forbid(unsafe_code)) |
| 9 | Stabiele C ABI, cbindgen en golden files | 0032 | Geïmplementeerd en geverifieerd (crates/wx1-ffi, include/wx1.h, GCC C-integratietest) |
| 10 | GPU/multi-device overdracht, async writer en restart, pas na CPU API-stabiliteit | 0033 | Geïmplementeerd en geverifieerd (DLPack/JAX/PyTorch ingestie, non-blocking transfer, bit-exact restart) |

ADR 0019 blijft de interne Rust-microbenchmark; ADR 0021 wordt de aparte cross-format-suite. Bestaande ADR's 0001–0019 worden niet hernummerd.

## Benchmarkgate

- Geen vergelijking bij ongelijke precisie/foutgrens.
- Lossless en lossy resultaten apart.
- Minimaal vijf volledige runs; minimaal 30 koude en 30 warme herhalingen per leesworkload; eerste run is warm-up.
- Hardware/software/filesystem/commit/configuratie exact vastleggen.
- De gebruiker heeft de drempels goedgekeurd: R1 100-GB/1-GB open-tijd-ratio ≤2,0; W5 I/O-stall ≤5% van compute-only modelstaptijd; D4 extra modelstaptijd ≤5% versus compute-only en niet slechter dan Zarr 3. Deze staan vast in `bench/config/acceptance.yaml` vóór de eerste formele cross-formatmeting.
- W2 en W4 rapporteren buffered en durable writes apart; de huidige v0.4.3 Rust-microbenchmark sluit `fsync` uit.
- W6 vergelijkt compute-only, sync WX-1, async WX-1, async Zarr 3 en async NetCDF4 in een end-to-end NWP-checkpointworkload. CPU eerst; GPU pas na de CPU/API-gate.
- Na elke fase benchmark opnieuw tegen dezelfde nulmeting.

## Releasebeleid

Volg `WX1_USER_CLI_TEST_INSTRUCTIONS.md`. Elke release bevat één projectroot in de ZIP, zonder `target/` en `.git/`; gewijzigde bestanden, compatibiliteitsimpact, niet-uitgevoerde tests en beperkingen worden vermeld. Rapporteer Rust-tests, Python-tests, wheel-builds en benchmarks apart. Fixrelease alleen bij concrete aangetoonde fouten.
