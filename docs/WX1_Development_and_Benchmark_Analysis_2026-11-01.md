# Tekstuele analyse — gecombineerde WX-1 ontwikkel- en benchmarkopdracht

**Datum:** 2026-11-01  
**Beoordeelde basis:** aangeleverde opdrachttekst en projectarchief `WX1_standalone_v0.4.3.zip`.

## Samenvatting

De opdracht is inhoudelijk een sterke en bruikbare koers voor WX-1 als opslaglaag voor een eigen NWP-model. De nadruk op fase-gates, reproduceerbare metingen, lossless restart, expliciete lossy foutgrenzen, behoud van gridsemantiek en CPU-first GPU-ondersteuning is juist. De opdracht is echter niet zonder correcties direct uitvoerbaar: de genoemde basisversie is verouderd ten opzichte van de later aangeleverde verificatie, drie voorgeschreven status-/workflowdocumenten ontbraken, ADR 0019 is al bezet, en Fase 1 vereist implementatie in plaats van alleen verificatie.

## 1. Wat inhoudelijk sterk is

- Fasegewijze uitvoering voorkomt dat prestaties of interoperabiliteit worden geclaimd vóór functionele correctheid.
- Lossless restart en eventueel lossy output zijn terecht gescheiden.
- Eén uniforme adapterinterface en vaste lokale fixtures maken reproduceerbare vergelijking mogelijk.
- Zowel operationele ECMWF-data, ERA5, HARMONIE als eigen regionale/globale JAX-output zijn representatieve NWP-workloads.
- R1–R9, W1–W6, C1–C3 en D1–D5 dekken opslag, selectieve reads, schaling, herstel en downstream gebruik.
- Niet-reguliere grids, cubed-sphere, halos, Arakawa C-staggering, nesting en ensembles moeten expliciete metadata/topologie blijven; geen stille regridding.
- CPU-only blijft een volledig ondersteund installatie-/uitvoeringspad; GPU is optioneel en volgt pas na stabiele CPU-correctheid.

## 2. Correcties en verduidelijkingen

### 2.1 Versiebasis

De opdracht noemt v0.4.1-fix1 als laatste geverifieerde release. In de conversatie is daarna WSL-verificatie voor v0.4.2 én v0.4.3 aangeleverd. v0.4.3 is daarmee de meest recente door gebruiker geverifieerde ontwikkelbasis. De historische v0.4.1-fix1-basis blijft relevant voor compatibiliteit, maar is niet meer de actuele kopieer-/ontwikkelbasis.

### 2.2 Ontbrekende documenten

In het geinspecteerde v0.4.3-projectarchief ontbraken `WX1_ROADMAP_CURRENT.md`, `WX1_CURRENT_STATUS.md` en `WX1_USER_CLI_TEST_INSTRUCTIONS.md`. Deze documentatiewijziging maakt die bestanden aan en verwijst vanuit `WX1_PLAN.md` en de README naar de actuele roadmap/status.

### 2.3 Fase 1 is geen alleen-verificatiefase

De v0.4.3-workspace bevat alleen `wx1-format`, `wx1-registry` en `wx1-cli`. Er is geen PyO3-dependency, maturin-configuratie, installabel Python-package of Python-testpakket. `python/README.md` beschrijft alleen een gepland pakket. NumPy, xarray, Dask en externe adapters zijn niet geïmplementeerd. Fase 1 is dus een ontwikkel- en verificatiefase en is momenteel niet afgerond.

De functionele adapter-smoketests uit Fase 1 moeten worden onderscheiden van de uniforme benchmarkadapterinterface uit Fase 2; anders worden dezelfde taken dubbel ingepland.

### 2.4 ADR-nummerconflict

ADR 0019 bestaat al als `0019-performance-benchmark-harness.md` en blijft de interne Rust-microbenchmark. ADR 0005 blijft het algemene Python-/compute-kader. De nieuwe planning reserveert ADR 0020 voor de concrete PyO3/Python-API, ADR 0021 voor de aparte cross-format-suite, ADR 0022 voor appendable scientific datasets en ADR 0023 voor lazy open; zie de bijgewerkte ADR-index.

### 2.5 72-byte generatie-descriptor

De broncode definieert `GENERATION_DESCRIPTOR_LEN = 72`; de generatie-directory-parser leest de BLAKE3-digest uit bytes 40..72. Daarnaast bestaat `CHUNK_DESCRIPTOR_LEN = 40` voor een ander ouder experimenteel directorypad. De opdracht verwijst terecht naar de 72-byte descriptor, maar documentatie en ADR's moeten beide layouts duidelijk onderscheiden. Wijziging vereist een expliciete versie-/compatibiliteitsanalyse, spec-draft, golden fixtures, tests en ADR.

### 2.6 Lazy open versus integriteit

De huidige `open_generation_snapshot` reconstrueert de generatieketen, parseert descriptors en directory's, verifieert chunkpayloads en indexcatalogi en berekent generatiehashes. Daarmee is open volledig validerend en O(bestandsgrootte) qua gelezen data. Een lazy `open()` kan niet dezelfde volledige-integriteitsgarantie leveren zonder de data te scannen.

De specificatie moet drie garanties scheiden: beperkte structurele open, integriteitscontrole van data bij het lezen en expliciete volledige `validate()`. Ook moeten herstel, hash-bereik en vertrouwen in niet-gelezen chunks worden gedefinieerd.

### 2.7 Parallel writers

“Meerdere ranks/tiles zonder lock” is alleen veilig als onafhankelijke writers geen gedeelde index- of commitstructuur gelijktijdig aanpassen. Gebruik daarom parallelle staginggebieden/buffers en een gecontroleerde merge/commit van de generatie. Zes cubed-sphere tiles zijn niet automatisch twaalf onafhankelijke writers; extra ranks vereisen expliciete sub-tile-/blokverdeling.

### 2.8 Lossy foutgrenzen

Per variabele moeten methode, codecversie, kwantisatieparameter, absolute/relatieve foutdefinitie en gemeten maximale fout worden opgeslagen of gerapporteerd. Test ook NaN, infinity, sentinels, fill values, signed zero en over-/underflow. Restart blijft exact en mag geen lossy pad gebruiken.

### 2.9 Benchmarkdrempels

De gebruiker heeft de numerieke doelen bevestigd: R1 open-tijd-ratio tussen 100 GB en 1 GB maximaal 2,0×; W5 I/O-stall maximaal 5% van compute-only modelstaptijd; D4 async-writer-overhead maximaal 5% versus compute-only en niet slechter dan Zarr 3. W6 gebruikt voor de CPU-first async checkpoint-workload dezelfde 5%-doelwaarde. R2 vereist minimaal 2× lagere latency dan zowel GRIB2 als Zarr 3, koud en warm. Bestandsgrootte moet bij gelijke foutgrens kleiner zijn dan GRIB2 CCSDS. Deze doelen staan in `bench/config/acceptance.yaml`; het zijn acceptatiedrempels, geen behaalde resultaten.

De reeds uitgevoerde v0.4.3 benchmark had één volledige benchmarkinvocatie per filesystem, interne medianen van het harness en alleen synthetische data. Zevenpunt-plus-vergelijkingen met andere formaten, vijf volledige runs, 30 koude/warme herhalingen, cacheflush, echte NWP-fixtures en downstream-workloads zijn niet gemeten.

### 2.10 WSL-benchmarkmethode

`/tmp` en `/mnt/d` zijn afzonderlijke meetlocaties; filesystem/mounttype moet met tooling worden vastgelegd in plaats van verondersteld. De primaire benchmark hoort op lokaal Linux/ext4/NVMe binnen WSL te draaien, niet op `/mnt/c`. `drop_caches` is systeemwijd, vereist privileges en garandeert niet dat alle hardwarecaches leeg zijn. CPU-ID's voor P-cores mogen niet worden gegokt: eerst topology detecteren. Temperatuur, throttling, opslagmodel en CPU-affinity waren in de aangeleverde v0.4.3-run niet gemeten.

## 3. Gelezen en geïnspecteerde bestanden

In het aangeleverde v0.4.3-projectarchief zijn gelezen/geïnspecteerd:

- `AGENTS.md`, `WX1_AGENTS.md`, `WX1_PLAN.md`, `README.md`, `WX1_ARCHITECTURE.md`, `CONTRIBUTING.md`;
- workspace `Cargo.toml` en manifests van `wx1-format`, `wx1-registry` en `wx1-cli`;
- `crates/wx1-format/src/lib.rs`, `scientific.rs`, de array-/streaming-tests en `benches/wx1_performance.rs`;
- `crates/wx1-cli/src/main.rs` en `tests/scientific_workflow.rs`;
- `python/README.md` en `docs/spec/WX1_Python_Compute_Architecture_v0.2.md`;
- `docs/decisions/README.md`, ADR 0019, de v0.4.3 benchmarkdraft, huidige historische roadmap, changelog en relevante wetenschappelijke specificaties.

Niet aanwezig in de oorspronkelijke v0.4.3-projectmap: `WX1_ROADMAP_CURRENT.md`, `WX1_CURRENT_STATUS.md` en `WX1_USER_CLI_TEST_INSTRUCTIONS.md`. De documentatiewijziging voegt ze toe.

De CLI-usage is gecontroleerd tegen `usage()` in `crates/wx1-cli/src/main.rs` en de eerder door de gebruiker aangeleverde WSL-help-output. In deze werkomgeving was `cargo`/`rustc` niet beschikbaar; er is hier geen nieuwe build of CLI-run uitgevoerd.

## 4. Aannames

- De laatst aangeleverde door de gebruiker geverifieerde ontwikkelbasis v0.4.3 is de actuele basis; v0.4.1-fix1 blijft een compatibiliteitsreferentie.
- De volledige terminaluitvoer die de gebruiker heeft geplakt geldt als door de gebruiker uitgevoerde bewijsbron; de huidige sessie heeft die tests niet opnieuw uitgevoerd.
- De benchmarkwaarden uit `/tmp` en `/mnt/d` zijn voorlopige resultaten van de bestaande interne WX-1-benchmark, geen vergelijking met GRIB2/NetCDF4/Zarr3/HDF5.
- De Python- en adapteronderdelen zijn ontwerpdoelen totdat broncode, tests en wheel-builds bestaan.
- De R1/W5/D4-drempels zijn door de gebruiker goedgekeurd en vooraf vastgelegd in `bench/config/acceptance.yaml`; dit zijn doelen, geen behaalde resultaten.
- Geen wire-formatwijziging is toegestaan zonder de vastgelegde compatibiliteitsprocedure.

## 5. Conclusie

De opdracht is een goede strategische specificatie voor WX-1, maar de huidige ontwikkelstatus is nog vóór de formele multi-format benchmarkfase. De eerstvolgende inhoudelijke stap is Fase 1: een echte CPU-only PyO3/maturin-basis met NumPy, xarray lazy read, Dask chunked workflow en functionele lokale adapters. Pas na die gate wordt het volledige benchmarkharnas met vooraf vastgelegde acceptatiedrempels opgezet.


## 6. Aanvullingen na review door de gebruiker

De volgende punten zijn expliciet opgenomen in de leidende specificatie en roadmap:

1. **Eigen ADR voor Python API.** ADR 0005 blijft het algemene compute-/Python-kader. ADR 0020 is gereserveerd voor de concrete PyO3/maturin-package-API. De cross-format benchmark krijgt daarnaast ADR 0021; ADR 0019 blijft uitsluitend de interne Rust-microbenchmark.
2. **Geïndexeerde planner na nulmeting.** De bestaande planner is lineair. De v0.4.3 benchmark meet 4096 chunkreferenties (6228.0 ns op `/tmp`, 6363.0 ns op `/mnt/d` in de aangeleverde runs), maar zegt niets over miljoenen chunks. Na de formele Fase-2-nulmeting wordt een aparte plannerstudie uitgevoerd met 4096, 65536 en 1048576 referenties en verschillende selectiedichtheden. Deze schaalpunten zijn nog niet gemeten.
3. **Twee benchmarksystemen blijven bestaan.** ADR 0019 is de interne Cargo/Rust-microbenchmark voor WX-1-codepaden. ADR 0021 is de uniforme cross-format-suite voor WX-1, GRIB2, NetCDF4 en Zarr 3. De eerste voedt de tweede diagnostisch, maar vervangt deze niet.
4. **Buffered versus durable I/O.** De huidige v0.4.3 `write_mib_s` sluit fsync uit. Daarom worden buffered en durable writes afzonderlijk gemeten in W2 en W4. Een adapter moet haar echte sync-garantie documenteren; `flush()` is niet hetzelfde als duurzame opslag. Procescrash (`kill -9`) en fysieke power loss blijven verschillende tests.
5. **W6 end-to-end checkpoint overhead.** Nieuw workloadontwerp vergelijkt compute-only, synchrone WX-1, asynchrone WX-1, asynchrone Zarr 3 en asynchrone NetCDF4. CPU is eerst; GPU volgt pas na de CPU/API-gate. Meet modelstaptijd, stall, queue/backpressure, durable checkpointtijd en RAM/VRAM.

### 6.1 Vastgelegde acceptatiedrempels

Voorafgaand aan de eerste formele Fase-2-cross-formatmeting zijn vastgelegd: R1 open-tijd 100 GB versus 1 GB maximaal 2,0×; W5 I/O-stall maximaal 5% van compute-only modelstaptijd; D4 en W6 async-writer extra stapduur maximaal 5% ten opzichte van compute-only; R2 ten minste 2× sneller dan zowel GRIB2 als Zarr 3, koud en warm; en bestandsgrootte kleiner dan GRIB2 CCSDS bij gelijke foutgrens. De configuratie staat in `bench/config/acceptance.yaml`. Dit zijn acceptatiedoelen, geen resultaten.
