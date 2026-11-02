# WX-1 — Gecombineerde ontwikkel- en benchmarkopdracht

**Status:** leidende ontwikkelspecificatie en roadmapopdracht; dit document beschrijft gewenste functionaliteit, niet automatisch geïmplementeerde functionaliteit.  
**Documentdatum:** 2026-11-01  
**Laatste door de gebruiker geverifieerde ontwikkelbasis:** v0.4.3, op basis van de WSL-uitvoer in deze conversatie.  
**Formaatstatus:** experimenteel; geen stabiel v1.0-interoperabiliteitscontract.

> **Waarheidsregel:** elk testresultaat en benchmarkgetal moet herleidbaar zijn tot een daadwerkelijk uitgevoerde run. Niet uitgevoerde onderdelen worden aangeduid als **niet getest**, **niet gemeten** of **niet geïmplementeerd**. Een ontwerp, stub, voorbeeld-API of README geldt niet als implementatie.

## 1. Doel en beoordelingsmaatstaf

WX-1 wordt ontwikkeld als wetenschappelijke opslag-engine voor multidimensionale data, met meteorologie en een eigen numeriek weersvoorspellingsmodel (NWP) als hoofdtoepassing. Het doel is bruikbare opslag voor regionale en globale JAX-modellen op CPU en GPU, met ensembles, nesting en niet-reguliere grids.

WX-1 moet aantoonbaar concurreren met GRIB2, NetCDF4, Zarr 3 en HDF5. **“Beter” is geen algemene claim:** prestaties en bestandsgrootte worden per workload, dataset, precisie, foutgrens, configuratie en hardware gemeten. Er wordt geen winnaar uitgeroepen als de vergeleken formaten niet dezelfde gegevenssemantiek en precisie leveren.

Parquet/Arrow is uitsluitend een kandidaat voor geschikte puntreeksen-workloads; het is geen algemene multidimensionale-arraybaseline. TileDB en COG zijn optionele aanvullende vergelijkingen.

## 2. Basis, versiebeleid en aannames

### 2.1 Verifieerbare basis

De bij de opdracht gevoegde zin dat v0.4.1-fix1 de laatste geverifieerde release is, is ingehaald door latere WSL-uitvoer die de gebruiker in deze conversatie heeft aangeleverd:

- v0.4.2: 108 workspace-tests, formatting, Clippy, CLI- en handmatige regressiecontroles geslaagd.
- v0.4.3: dezelfde 108 workspace-tests, formatting, Clippy, CLI- en regressiecontroles geslaagd; het benchmarktarget is gebouwd en uitgevoerd op zowel `/tmp` als `/mnt/d`.

Daarom is v0.4.3 de huidige **door de gebruiker geverifieerde ontwikkelbasis** voor deze roadmap. Dit is geen bewering dat WX-1 een stabiel bestandsformaat of een v1.0-contract is. Zie `WX1_CURRENT_STATUS.md` voor de precieze test- en benchmarkstatus.

Als een toekomstige, door de gebruiker aangeleverde statusregistratie een andere basis aanwijst, moet die discrepantie expliciet worden opgelost voordat code wordt gewijzigd. Er mag niet stilzwijgend worden teruggevallen op v0.4.1-fix1.

### 2.2 Wire-formatbehoud

De huidige 72-byte generatie-descriptor is een bestaande wire-layout. De broncode bevat daarnaast een oudere 40-byte descriptor voor het experimentele raw chunk-directory-pad. Deze twee structuren mogen niet worden verward.

Een wijziging aan de 72-byte descriptor, footer, generation directory, indexpagina's, metadata-schema's, codec-ID's, checksum- of hashdekking vereist vóór implementatie:

1. een expliciete compatibiliteitsanalyse;
2. een spec-draft met exacte bytes, offsets, versies en reserved bytes;
3. tests en golden/compatibility fixtures;
4. een ADR;
5. een migratie- en fallbackstrategie.

Geen stilzwijgende wijziging van een bestaande wire-layout. Oude fixtures en leesroutes blijven onderdeel van de compatibiliteitstests.

### 2.3 Overige ontwerpuitgangspunten

- Rust blijft de opslagkern; Python is een officiële, maar nog te implementeren gebruikersinterface.
- CPU-only installatie en werking moeten zonder GPU-toolchain of GPU-runtime mogelijk zijn.
- GPU-functionaliteit is optioneel en capability-based.
- Lossless is standaard. Lossy output vereist expliciete metadata en een aantoonbaar gehaalde foutgrens.
- Restart-data is altijd lossless en bit-exact, inclusief dtype en PRNG-state.
- Geen stille conversie van grid, CRS, eenheden, parametercodes of precisie.
- Bounded buffers, queues en backpressure zijn verplicht voor streaming en parallelle verwerking.

## 3. Verplichte werkvolgorde en gates

Een volgende fase begint pas wanneer de vorige fase aantoonbaar is geverifieerd.

### Fase 0 — documentatie en broninspectie

Lees eerst:

- `WX1_AGENTS.md`
- `AGENTS.md`
- `WX1_PLAN.md`
- `WX1_ROADMAP_CURRENT.md`
- `WX1_USER_CLI_TEST_INSTRUCTIONS.md`
- `WX1_CURRENT_STATUS.md`

Inspecteer daarna de echte projectstructuur, alle relevante Cargo-manifests, broncode, tests, specificaties, ADR's, changelog en de actuele CLI-help. Als een gevraagd bestand ontbreekt, vermeld dat en herstel de documentatie; beweer niet dat het gelezen is.

Rapporteer vóór wijzigingen welke bestanden echt zijn gelezen, welke ontbreken en welke aannames worden gebruikt.

### Fase 1 — huidige versie afronden en Python-/adapterintegratie verifiëren

Implementeer volgens spec-draft → ADR → tests → implementatie:

1. PyO3-bindings en een reproduceerbare maturin-build (ADR 0020; de oudere ADR 0005 blijft het overkoepelende Python-/compute-kader).
2. Een CPU-only installatie/wheel zonder GPU-afhankelijkheden.
3. NumPy roundtrip met dtype, byte-order, shape en bit-/waardencontrole.
4. xarray lazy open/read zonder onbedoelde volledige materialisatie.
5. Dask chunked uitvoering en een representatieve reductie.
6. NetCDF-, Zarr- en GRIB2/ecCodes-adapters met kleine lokale fixtures en roundtrip-/metadata-preservatietests.

De Python-API blijft een dunne binding rond de Rust-kern. De bijbehorende proposal-spec staat in `docs/spec/WX1_Python_API_Draft_v0.1.md`. Kopieën, bufferownership, mutability, GIL-gedrag, foutvertaling en levensduur van datasets/buffers moeten expliciet worden getest en gedocumenteerd. Geen zero-copy-claim zonder bewijs. Optionele wetenschappelijke dependencies blijven buiten de minimale CPU-only installatie.

Python, NumPy, xarray, Dask, NetCDF, Zarr en ecCodes zijn in de v0.4.3-bronbasis niet geïmplementeerd. Fase 1 is dus geen administratieve afvinkronde: het is een functionele ontwikkelfase. CPU-only moet zonder GPU-pakketten blijven werken. GRIB2/ecCodes-tests mogen alleen als geslaagd worden gerapporteerd als de fixture, bibliotheekversie en daadwerkelijke testuitvoer aanwezig zijn.

De functionele adaptertests in Fase 1 zijn smoke-/correctheidstests. De uniforme benchmarkadapter-interface en prestatiemetingen horen bij Fase 2; zo worden implementatie en benchmarkharnas niet dubbel geteld.

### Fase 2 — cross-format benchmarkharnas en formele nulmeting

Na Fase 1:

- bouw de afzonderlijke cross-format-suite (ADR 0021; spec-draft `docs/spec/WX1_Cross_Format_Benchmark_Draft_v0.1.md`) met adapters voor WX-1, GRIB2, NetCDF4 en Zarr 3;
- voer adapter-roundtrips op synthetische lokale fixtures uit;
- gebruik de interne WX-1 Rust-microbench (ADR 0019) als diagnostische component; vervang de cross-format-suite er niet door en behandel microbenchresultaten niet als formatvergelijking;
- leg de acceptatiedrempels vóór de eerste formele vergelijkende meting vast in `bench/config/acceptance.yaml`;
- meet dezelfde workloads op v0.4.3 en op de uitkomst van Fase 1 onder identieke data, precisie, foutgrenzen, hardware en meetsemantiek;
- meet buffered writes en durable writes afzonderlijk; rapporteer `fsync`/durability alleen wanneer de adapter die garantie werkelijk uitvoert;
- bewaar ruwe CSV/JSON, hardware/software-informatie, configuratie, logs en SHA256-manifesten.

De eerdere v0.4.3 benchmark is een **voorlopige single-invocation baseline**, geen volledige formele Fase-2-nulmeting: er zijn geen vijf volledige runs, geen 30 expliciet koude en warme herhalingen per leesworkload, geen externe formaatvergelijking en geen echte ECMWF/ERA5/HARMONIE-fixtures in die meting uitgevoerd.

### Fase 3 — fundamentele opslagwijzigingen

Werk appendable scientific datasets (ADR 0022) en lazy open (ADR 0023) uit. Voor elk onderdeel geldt: spec-draft → ADR → tests → implementatie → volledige regressie → benchmark tegenover de Fase-2-nulmeting.

ADR 0019 blijft het interne Rust-microbenchmarkharnas; ADR 0020 beschrijft de Python API; ADR 0021 is de aparte cross-format-suite. Nieuwe wire-layouts zijn niet impliciet toegestaan.

### Fase 4 — overige onderdelen

Voer de onderdelen in §4 in volgorde uit. Na iedere fase worden de bijbehorende correctheidstests en relevante benchmarks opnieuw uitgevoerd. Geen volgende fase zonder gate-resultaat.

## 4. Functionele ontwikkelonderdelen

Elk onderdeel begint met spec-draft en ADR, gevolgd door tests en pas daarna implementatie. Voor optimalisaties geldt: eerst een reproduceerbare nulmeting, daarna één wijziging tegelijk, en na elke wijziging dezelfde benchmark tegen dezelfde basis.

### 4.1 Appendable scientific datasets

Laat de tijd-as per generatie groeien zodat een NWP-model tijdstap voor tijdstap kan schrijven. Bestaande schema-v2-bestanden zijn nu metadata-manifest-immutabel en weigeren append; dit gedrag mag niet stilzwijgend worden veranderd.

Ontwerpdoelen:

- nieuwe tijdstap schrijft uitsluitend nieuwe data/chunks en gewijzigde index-/manifeststructuren;
- oude tijdstappen worden niet herschreven;
- een tijdstap wordt pas zichtbaar na een volledige, duurzame commit;
- bij crash blijft de laatste volledig gecommitte tijdstap leesbaar;
- recovery kiest deterministisch de laatste geldige generatie;
- groei van tijdsdimensie, variabele metadata en indexmappings heeft expliciete semantiek;
- schema-/versiecompatibiliteit met oude lezers wordt vastgelegd.

**Parallelle producers wijzigen niet ongecoördineerd dezelfde directory, index-root of commit-footer.** Onafhankelijke staginggebieden/tiles mogen parallel worden gevuld; publicatie van de samengevoegde generatie gebeurt via een gecontroleerde commit.

### 4.2 Lazy open

Doel: `open()` leest alleen de minimaal noodzakelijke footer-, preamble- en root-/cataloginformatie; volledige verificatie blijft beschikbaar via `validate()`.

Scheiding van garanties:

- `open()`: structurele controle van de benodigde metadata en rootstructuren; geen claim van volledige datavalidatie;
- data-read: integriteitscontrole van de chunks/pages die daadwerkelijk worden gebruikt;
- `validate()`: volledige controle van alle generaties, directories, indexen, opgeslagen chunkbytes, CRC32C en BLAKE3.

De huidige `open_generation_snapshot` scant herstelkandidaten en valideert de volledige generatieketen, descriptors, chunkpayloads, indexen en generatiehashes. Dit is bestandsgrootte-afhankelijk. Een snelle `open()` vraagt dus een expliciet integriteits- en herstelontwerp; volledige validatie mag niet stilzwijgend verdwijnen.

### 4.3 Lossy codec met gegarandeerde foutgrens

Ontwerp quantisatie plus Zstandard en, waar zinvol, bit-shuffle. De methode, codecversie, kwantisatiestap, foutgrens en foutmaat zijn verplicht in metadata per variabele. Lossy verwerking is nooit stilzwijgend.

Twee afzonderlijke profielen:

- `restart`: lossless en bit-exact, inclusief float32/float64, byte-order, modelconfiguratie, ensemble-/shardidentiteit en PRNG-state;
- `output`: lossy alleen wanneer expliciet gekozen en de vaste foutgrens per variabele aantoonbaar wordt gehaald.

Voor lossless en lossy worden aparte resultaten en tabellen gemaakt. Test ook NaN, infinities, sentinels, fill values, signed zero, overflow en variabelen met absolute versus relatieve foutgrenzen. Codecuitvoering is CPU-threaded; geen GPU-codecs in deze fase.

### 4.4 Parallel schrijven en asynchrone writer

Ondersteun meerdere ranks, tiles of devices die eigen blokken aanleveren. Vereisten:

- geen ongecoördineerde gelijktijdige mutatie van gedeelde index-/commitstructuren;
- expliciete staging- en merge/commitfase;
- bounded queues en backpressure;
- deterministische mapping van tile/chunk naar generatie;
- crashveilig gedrag en recovery;
- asynchrone writer die de JAX-modelstap niet onnodig blokkeert;
- meet device-to-host-kopie, I/O-stall, wachttijd, host-RAM en later VRAM.

Zes cubed-sphere-tiles betekent niet automatisch twaalf onafhankelijke tile-writers. Bij meer workers dan tiles moet de workload expliciet sub-tiles of meerdere blokken per tile definiëren.

### 4.5 Gridbeschrijving voor niet-reguliere grids

Ondersteun als ontwerpdoel Gaussian, reduced Gaussian, gewone lat/lon, Yin-Yang, cubed-sphere, icosahedron, Arakawa C-grid (u/v/w), halo's, geneste domeinen en ensemble-member-as.

Voor cubed-sphere: zes tiles, C-resolutie, tile-oriëntatie, halo-breedte, tile-randen/hoeken en burenrelaties. Voor staggered grids: aparte locatie-/dimensionering van u, v en w. Leg tile-layout vast zodat een tile in de geheugenlayout van het model kan worden gelezen.

Bewaar de oorspronkelijke topologie en coördinaten. Regridding of conversie naar lat/lon gebeurt uitsluitend via een expliciete, gedocumenteerde en testbare stap.

### 4.6 Parameterregisters en eenheden

Volgende minor-release volgens het ontwikkelplan: WMO/GRIB-identificatie, CF-namen, tabel/providercontext, versie, provenance en veilige eenheden. Onbekende codes blijven herkenbaar en worden niet van een verzonnen betekenis voorzien.

### 4.7 Leespaden

Ontwikkel en meet afzonderlijk:

- één veld voor variabele, level, tijd en member;
- één level over alle tijden;
- tijdreeks voor een punt over niveaus;
- regio-uitsnede;
- één tile of alle tiles;
- volledige scan van één variabele;
- eventueel een tweede layout/index voor puntreeksen.

Sub-chunk lezen mag alleen worden geclaimd waar de codec en fysieke layout dit daadwerkelijk ondersteunen. Rapporteer hoeveel bytes/chunks zijn gelezen en hoeveel volledige chunk-decompressies nodig waren.

### 4.8 Geïndexeerde scientific array planner

De v0.4.3-planner is lineair over de chunkreferenties van de variabele. De huidige meting omvat 4096 chunks; zij bewijst geen gedrag bij honderdduizenden of miljoenen chunks. De lineaire aanpak is daarom een bekende schaalbaarheidsrisico/bottleneck, maar de omvang van de praktische impact moet met grotere fixtures worden gemeten.

Na de formele Fase-2-nulmeting (en niet vóór die nulmeting) wordt een geïndexeerde planner onderzocht en als afzonderlijke wijziging beoordeeld (ADR 0028). Eerst worden de bestaande lineaire planner en de kosten van planner, open/validate en geselecteerde reads apart gemeten. Vergelijk vervolgens ten minste de schaalniveaus 4096, 65536 en 1048576 chunkreferenties, meerdere selectievormen en dichtheden, koude/warme toestand en piek-RSS. Deze schaalniveaus zijn testdoelen, geen reeds gemeten resultaten.

Mogelijke indexvormen (bijvoorbeeld multidimensionale intervallen, tile-/blokindex of hergebruik van persistente indexstructuren) blijven alternatieven totdat benchmark en semantiek een keuze rechtvaardigen. De planner moet exact dezelfde chunkselectie opleveren als de huidige implementatie; geen false negatives, geen onbedoelde extra volledige chunk-decompressies en geen wijziging aan wire-layout zonder afzonderlijke compatibiliteitsprocedure.

### 4.9 Interoperabiliteit

GRIB2 import/export via ecCodes, NetCDF- en Zarr-adapters, met lokale roundtrip-fixtures en metadata-preservatie. GRIB2 packing types worden afzonderlijk getest. Cubed-sphere naar lat/lon export vereist expliciete regridding; nooit stilzwijgend.

### 4.10 Robuustheid voor 1.0

Fuzzing, conformance vectors, onafhankelijke wire-testvectors, power-loss-/crash-tests, security review, migratiebeleid en cross-platform builds. `kill -9` is een procescrashtest en op zichzelf geen bewijs van echte power-loss-durability.

### 4.11 Stabiele C-ABI

Ontwerp `wx1-ffi` met cbindgen en golden files, nadat de relevante Rust API en wire-contracten stabiel genoeg zijn. Talen zoals Tempest kunnen later via deze ABI lezen en schrijven. ABI-versie, ownership, foutcodes, thread safety en bufferlevensduur worden expliciet vastgelegd.

### 4.12 GPU-integratie — pas na CPU-correctheid en stabiele API

CPU-first; de core en CPU-installatie hangen nooit af van GPU-pakketten. GPU-werk omvat:

1. overdracht van host JAX-/NumPy-arrays via buffer protocol of DLPack, met documentatie van iedere kopie;
2. asynchrone writer met dubbele buffers en begrensde queue; device-to-host-kopie van stap n overlapt met GPU-berekening van stap n+1 waar backend/runtime dit werkelijk toelaat;
3. restart van geshardede/multi-device state, bit-exact teruglezen en reconstructie;
4. optionele device-read naar bijvoorbeeld JAX/CuPy, pas na de voorgaande onderdelen;
5. geen GPU-codecs.

Geen zero-copy- of overlapclaim zonder meetbaar bewijs. GPU-tests worden alleen uitgevoerd wanneer hardware/runtime aanwezig is; anders expliciet **niet gemeten**.

## 5. ADR-volgorde en bestaande besluitnummers

De geaccepteerde ADR's 0001–0019 blijven ongewijzigd. ADR 0005 beschrijft het algemene Python-/compute-kader; ADR 0020 werkt de concrete PyO3/Python-package-API uit. ADR 0019 betreft uitsluitend de interne WX-1 Rust-microbenchmark; ADR 0021 beschrijft de afzonderlijke cross-format benchmark-suite.

| ADR | Onderwerp | Status |
|---|---|---|
| 0019 | Interne WX-1 Rust-performance-microbenchmark | Bestaand; door gebruiker in WSL gebouwd en gedraaid |
| 0020 | Python API, PyO3 en maturin | Gepland; spec-draft toevoegen, niet geïmplementeerd |
| 0021 | Cross-format benchmarkharnas en adapterprotocol | Gepland; niet geïmplementeerd |
| 0022 | Appendable scientific datasets | Gepland; niet geïmplementeerd |
| 0023 | Lazy open en expliciete volledige validate | Gepland; niet geïmplementeerd |
| 0024 | Lossy quantisatie + Zstandard met foutgrens | Gepland; niet geïmplementeerd |
| 0025 | Parallelle staging, merge en asynchrone writer | Gepland; niet geïmplementeerd |
| 0026 | Niet-reguliere grids en tile-topologie | Gepland; niet geïmplementeerd |
| 0027 | Parameterregisters en eenheden | Gepland; bestaande registry-core deels aanwezig |
| 0028 | Geïndexeerde scientific array planner | Na formele nulmeting; huidige planner is lineair |
| 0029 | Geavanceerde leespaden en alternatieve chunk-layouts | Gepland; niet geïmplementeerd |
| 0030 | GRIB2/NetCDF/Zarr interoperabiliteit | Gepland; adapters niet geïmplementeerd |
| 0031 | v1.0 robustness, security en migratie | Gepland; niet afgerond |
| 0032 | Stabiele C-ABI | Gepland; niet geïmplementeerd |
| 0033 | GPU- en multi-device-integratie | Gepland; niet geïmplementeerd |

Dit zijn geplande identifiers, geen geaccepteerde ontwerpbesluiten totdat het betreffende ADR-document is gereviewd en geaccepteerd. Een ADR wordt niet hernummerd of overschreven nadat het is aangemaakt. Als scope later wordt gesplitst, krijgt het nieuwe besluit een nieuw vrij nummer.

## 6. Benchmarkharnas

### 6.1 Structuur

Doelstructuur:

```text
bench/
  README.md
  config/       # YAML: datasets, formaten, foutgrenzen, runs en acceptatiedrempels
  datasets/     # download-/fixture-scripts, checksums; geen datasets in git
  adapters/     # één adapter per formaat
  workloads/    # W-, R-, C- en D-workloads
  runner/       # timing, geheugen, CPU, bytes, CSV/JSON
  report/       # tabellen, grafieken en interpretatie
  results/      # genummerde runmappen met hardware/software-info
```

Er zijn twee afzonderlijke benchmarksystemen:

**A. Interne Rust-microbenchmark (ADR 0019).** De bestaande `crates/wx1-format/benches/wx1_performance.rs` is een dependency-vrije Cargo-benchmark van interne WX-1-codepaden (codecs, open/validate, array-read en de huidige planner). Deze voedt optimalisatiebeslissingen en regressieonderzoek, maar vergelijkt WX-1 niet met andere formaten.

**B. Cross-format benchmark-suite (ADR 0021).** Het bovenstaande `bench/`-harnas vergelijkt WX-1, GRIB2, NetCDF4 en Zarr 3 via gemeenschappelijke adapters, gelijke fixtures, gelijke precisie/foutgrenzen en vastgelegde workloadsemantiek. Dit harnas is nog niet geïmplementeerd. Het vervangt de Rust-microbenchmark niet; beide rapportages blijven gescheiden en kunnen elkaar diagnostisch aanvullen.

### 6.2 Gemeenschappelijke adapterinterface

Conceptueel contract; dit zijn nog geen bestaande WX-1 API's:

```python
write(dataset, settings)
open(path)
read_field(var, level, time, member)
read_level_all_times(var, level)
read_point_series(var, lat, lon, levels)
read_region(var, level, time, bbox)
read_tile(var, level, time, tile)
scan_all(var)
close()
```

Adapters moeten dimension names, coordinate semantics, time/calendar, vertical coordinate, units, CRS, missing values, dtype en precision expliciet normaliseren of als verschil rapporteren. Geen stille metadata- of gridconversie.

### 6.3 Formaten

- WX-1;
- GRIB2 via ecCodes: simple, CCSDS en JPEG2000/ondersteunde packing;
- NetCDF4: zlib, zstd waar build beschikbaar is, shuffle;
- Zarr v2 en v3: blosc, zstd en sharding waar ondersteund;
- HDF5: h5py, gzip/zstd waar plugin/build beschikbaar is;
- Parquet/Arrow uitsluitend voor puntreeksen;
- ruwe F32 + Zstandard als eenvoudige baseline;
- optioneel TileDB en COG.

Registreer bibliotheekversie, buildopties en beschikbaarheid van iedere codec. Een niet beschikbare codec wordt niet stilzwijgend vervangen.

### 6.4 Datasets

Offline herhaalbare datasetset, met download alleen in expliciete scripts, metadata-manifest en SHA256:

1. ECMWF Open Data operationeel 0,25°: drukniveaus en oppervlak;
2. ECMWF ensemble: 51 members;
3. ERA5: één maand, 0,25°, uurlijks;
4. HARMONIE of ander 2,5 km-veld voor Nederland;
5. eigen regionale JAX-NWP-run met per-tijdstap output en restart;
6. eigen globale JAX-NWP-output op cubed-sphere: C96 en C192 fixtures; C384 schaaltest; zes tiles, per-tijdstap output en restart;
7. synthetische randgevallen: NaN, sentinels, lege dimensies en tile-randen.

Elk manifest registreert bron, versie/datum, variabelen, eenheden, dimensions, precision, licentie/voorwaarden, fixture-generatie en SHA256. Netwerktoegang hoort alleen in downloadstappen, niet in tests of benchmarkruns.

### 6.5 Foutgrenzen en vergelijkbaarheid

Voorbeelden van vaste foutgrenzen: T ±0,01 K; p ±0,1 Pa; u/v ±0,01 m/s; q relatieve fout 1e-4; neerslag ±0,001 mm. De definitieve per-variabele grenzen staan versiebeheerd in YAML vóór de meting.

Voor elk lossy bestand worden max absolute fout, RMSE en bias berekend en vergeleken met de ingestelde grens. Lossless resultaten worden apart gerapporteerd, met bit-exact waar dat de eis is. Nooit lossless en lossy resultaten in dezelfde ranglijst mengen.

### 6.6 Workloads

**Schrijven**

- W1: volledige dataset (bestandsgrootte, wandtijd, CPU-tijd, piek-RAM);
- W2: append per tijdstap, minimaal 100 stappen; meet buffered en durable commit afzonderlijk, inclusief tijd per stap en bewijs dat eerdere payloads niet worden herschreven;
- W3: parallel schrijven met 1, 2, 4, 8 en 12 ranks/workers; zes cubed-sphere tiles kunnen verder in onafhankelijke blokken worden verdeeld, maar tile-aantal en writer-aantal worden apart gerapporteerd;
- W4: restart schrijven en terugladen, bit-vergelijking inclusief PRNG-state; rapporteer buffered en durable checkpointtijd afzonderlijk;
- W5: globale cubed-sphere met zes tiles per tijdstap, C96 tot C384, doorvoer GB/s, stappen/uur en modelstap-stall;
- W6: end-to-end checkpoint-overhead in een representatieve NWP-modelloop. Vergelijk compute-only (geen I/O), synchrone WX-1-writer, asynchrone WX-1-writer, asynchrone Zarr 3-writer en asynchrone NetCDF4-writer. Meet modelstaptijd (mediaan/p95), writerduur, stall-/wachttijd, queuebezetting/backpressure, tijd tot durable checkpoint, piek-RAM en, indien beschikbaar, VRAM. CPU is de eerste verplichte uitvoering; GPU volgt pas na de CPU/API-gate. W6 is een workloadmeting, geen bewijs van niet-blokkerende uitvoering zonder gemeten stall-/wachttijd.

**Lezen — afzonderlijk koud en warm, minimaal 30 metingen per workload**

- R1: open-tijd tot eerste veld;
- R2: één veld;
- R3: één level over alle tijden;
- R4: tijdreeks op één punt over alle levels;
- R5: regio-uitsnede Nederland;
- R6: volledige scan van één variabele;
- R7: ensemble: één member versus alle members;
- R8: willekeurige 1000 velden;
- R9: één tile versus alle zes tiles.

**Robuustheid**

- C1: crash tijdens schrijven op vooraf gedefinieerde punten (na staging, tijdens index-/manifestbouw, vóór commit, tijdens commit en na durable commit); gebruik `kill -9` voor procescrash en controleer daarna open/recovery. Een procescrash is geen fysieke stroomuitval; echte power-loss-tests blijven afzonderlijk nodig;
- C2: één byte corruptie in data en index; detectie moet worden aangetoond;
- C3: 1, 10 en 100 GB; open-tijd als functie van bestandsgrootte.

**Downstream**

- D1: xarray lazy read plus Dask-gemiddelde;
- D2: laden naar JAX/NumPy plus rekenkernel, bijvoorbeeld vorticiteit;
- D3: AI-modelinvoer tot eerste inferentie, bijvoorbeeld Pangu/NeuralGCM indien model en runtime lokaal beschikbaar zijn;
- D4: JAX-model op CPU en GPU met asynchrone WX-1-writer, stapduur met/zonder I/O, stall-tijd, RAM/VRAM tegenover Zarr 3 en NetCDF4;
- D5: GPU-naar-host-overdracht, tijd en aantal kopieën per veld.

Niet beschikbare hardware/modellen worden **niet gemeten**, niet vervangen door verzonnen waarden.

### 6.7 Metrics

Bestandsgrootte, bits/value, wall time (mediaan, p95, min, max, standaarddeviatie), CPU-tijd, piek-RSS, piek-VRAM, bytes gelezen, syscalls/seeks indien meetbaar, foutmaten voor lossy, bitwise- of foutgrenscorrectheid. Meetsemantiek en de inclusie/exclusie van fsync, decode, integriteit, indexbouw en assembleertijd worden per workload vastgelegd.

Voor schrijfacties worden **buffered write** en **durable write** als aparte meetmodi gerapporteerd. Buffered write omvat de normale writer-/file-write-operaties zonder expliciete duurzaamheidssynchronisatie. Durable write omvat de expliciet gedocumenteerde sync-volgorde die de betreffende adapter garandeert (bijvoorbeeld bestandssync en, bij atomische creatie/vervanging waar vereist, directorysync). Een `flush()` of succesvolle userspace-write mag niet als duurzame commit worden aangeduid. Iedere adapter rapporteert welke duurzaamheidsgarantie werkelijk wordt geleverd; alleen gelijkwaardige garanties mogen direct worden vergeleken. De v0.4.3 microbenchmark sluit `fsync` uit en is daarom geen durable-write-meting.

### 6.8 Methode en reproduceerbaarheid

- Lokale NVMe in WSL/ext4 voor primaire meting; niet `/mnt/c`. Leg filesystem en mount/device vast; ga niet uit van ext4 zonder `findmnt`/mountcontrole.
- Extra `/mnt/d`- of Windows-mounted meting is een afzonderlijke filesystemvergelijking, geen primaire NVMe-resultaatmeting.
- `sync` en `drop_caches` vóór koude tests waar toegestaan; noteer of dit daadwerkelijk lukte. Zonder cachecontrole heet de test niet aantoonbaar koud.
- Linux `drop_caches` is systeemwijd en vereist passende privileges. Cacheverwijdering garandeert niet dat alle controller-/devicecaches leeg zijn.
- CPU-affinity met `taskset` en vaste threadaantallen; identificeer P-/E-core topology eerst, gebruik geen veronderstelde CPU-ID's.
- Performance power mode, temperatuur en throttling vastleggen waar meetbaar; anders **niet gemeten**.
- Minimaal vijf volledige runs; minimaal 30 leesherhalingen per workload en toestand; eerste run als warm-up uitsluiten.
- Formaatvolgorde per volledige run randomiseren met vastgelegde seed.
- Leg CPU, GPU, VRAM, RAM, schijf, OS/kernel, driver, Python, Rust, JAX, bibliotheekversies, compilerflags, WX-1-versie/commit en filesystem vast.
- Extra MinIO/object-store-simulatie voor WX-1, Zarr en Parquet indien de adapter operationeel is.

### 6.9 Acceptatiedrempels

De gebruiker heeft de volgende functionele doelen opgegeven:

- R2: ten minste 2× sneller dan GRIB2 en Zarr 3, afzonderlijk koud en warm;
- bij gelijke foutgrens kleiner dan GRIB2 CCSDS;
- R1: open-tijd onafhankelijk van bestandsgrootte, zelfde orde bij 1 en 100 GB;
- W2: constante tijd per stap, zonder herschrijven van eerder opgeslagen data;
- W5: schrijfdoorvoer houdt de modelstap bij zonder stall boven een vooraf vastgelegd percentage;
- D4: extra stapduur door async WX-1-writer blijft onder een vooraf vastgelegd percentage en is niet slechter dan Zarr 3;
- C1: de laatste volledig gecommitte stap blijft geldig;
- C2: 100% detectie van de vastgelegde corruptiegevallen;
- W4: bit-exact restart.

De volgende numerieke drempels zijn door de gebruiker goedgekeurd en worden vóór de eerste formele cross-formatmeting vastgelegd in `bench/config/acceptance.yaml`: R1 100-GB/1-GB open-tijd-ratio maximaal 2,0; W5 maximale I/O-stall maximaal 5% van de compute-only modelstaptijd; D4 extra modelstaptijd door de async writer maximaal 5% ten opzichte van de compute-only referentie en niet slechter dan de gemeten Zarr 3-referentie. R2 blijft minimaal 2× sneller dan zowel GRIB2 als Zarr 3, afzonderlijk koud en warm. Bestandsgrootte moet kleiner zijn dan GRIB2 CCSDS bij gelijke foutgrens. Deze drempels worden vóór de eerste formele vergelijkende run vastgezet en na inzage van resultaten niet aangepast.

W6 rapporteert dezelfde end-to-end modelstap-, stall- en queue-metrics voor de CPU-uitvoering en gebruikt voor de WX-1 asynchronous overhead dezelfde 5%-doelwaarde als D4; de GPU-uitvoering valt onder de latere GPU-gate. C1, C2 en W4 blijven respectievelijk: laatst volledig duurzaam gecommitte stap blijft geldig; alle vastgelegde corruptiegevallen worden gedetecteerd; restart is bit-exact.

### 6.10 Rapportage en reproduceerbaarheid

- één CSV per run en machine-/softwaremanifest;
- JSON met ruwe meetgegevens;
- tabellen/grafieken: bestandsgrootte versus leestijd; open-/leestijd versus bestandsgrootte; parallelle schaling; stall-tijd per modelstap;
- aparte secties “waar WX-1 verliest” en “beperkingen van de meting”;
- reproduceerbaar via `make bench-data`, `make bench-run`, `make bench-report` zodra deze Make-targets daadwerkelijk zijn geïmplementeerd;
- benchmarkadapter roundtriptests op kleine lokale fixtures.

Deze Make-targets zijn doelinterface, geen bestaande commando's zolang `Makefile` en targets ontbreken.

### 6.11 Benchmarkfasen

1. Harnas en adapters voor WX-1, GRIB2, NetCDF4 en Zarr 3; roundtriptests op synthetische fixtures.
2. R1 t/m R9.
3. W1 t/m W6 (W6 CPU-first; GPU-uitbreiding na de GPU/API-gate).
4. overige formaten.
5. robuustheid en downstream D1 t/m D5.

## 7. Release- en testrapportage

Iedere release-oplevering:

- ZIP met één projectroot; geen `target/`, `.git/`, cache, lokale data of persoonlijke bestanden;
- lijst gewijzigde bestanden;
- compatibiliteitsimpact en wire-formatimpact;
- tests die wel en niet zijn uitgevoerd;
- Rust-tests, Python-tests, wheel-builds en benchmarks afzonderlijk rapporteren;
- geen `-fixN` zonder concrete fout in door de gebruiker aangeleverde uitvoer;
- eerder geverifieerde release blijft behouden als herstelbasis;
- alle testcommando's in één chatbericht, volgens `WX1_USER_CLI_TEST_INSTRUCTIONS.md`.

## 8. Huidige implementatiestatus

De feitelijke status op documentdatum staat in `WX1_CURRENT_STATUS.md`. In het kort:

- v0.4.3 Rust-basis en de eerste interne benchmark zijn door de gebruiker in WSL geverifieerd;
- De v0.4.3-baseline bevatte nog geen Python/PyO3-integratie; de huidige Fase-1-werkrevisie bevat experimentele bindings-, packaging-, NumPy-, xarray- en externe reader-adapterprototypes. De native build en integratiegate zijn nog niet geverifieerd;
- multi-format benchmarkharnas, lokale ECMWF/ERA5/HARMONIE-fixtures en de volledige W/R/C/D-suite ontbreken;
- appendable schema-v2 en lazy open zijn niet geïmplementeerd;
- de huidige schema-v2 scientific manifest blijft immutable en `open_generation_snapshot` blijft volledig valideren.


## Vastgelegde interne planner-nulmeting en rapportagenotatie

De v0.4.3 interne planner-meting bij 4096 chunkreferenties is bevestigd als plausibele voorlopige nulmeting: **6228.0 ns per plan**, oftewel **6.228 us per plan** (ongeveer 6.2 us). `std::hint::black_box` is aanwezig in de bestaande benchmark. Gebruik niet `6.228 ns`, omdat dat letterlijk 6.228 nanoseconde zou betekenen. Canonieke schrijfwijze is `6228.0 ns` of `6.228 us`.

De schaalpunten 65536 en 1048576 chunkreferenties zijn gepland en **niet gemeten**. De schaalmeting behoort tot Fase 2 en wordt niet door Fase 1 vervangen. Alle rapportage gebruikt een punt als decimaalteken en geen duizendtalscheiding, ook voor CSV/JSON-presentatie en tabellen. Zie `bench/config/internal-planner-baseline.yaml`.
