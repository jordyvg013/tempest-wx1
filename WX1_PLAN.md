# WX-1 — Current planning authority and historical implementation plan

> **Current plan:** future development order, ADR numbering, benchmark gates and current status are governed by [`WX1_ROADMAP_CURRENT.md`](WX1_ROADMAP_CURRENT.md), [`WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md`](WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md) and [`WX1_CURRENT_STATUS.md`](WX1_CURRENT_STATUS.md). This file below preserves the historical v0.3.x/v0.4.x implementation plan and verification history; it must not override the current roadmap.

---

# WX-1 — Ontwikkelplan

**Document:** `WX1_PLAN.md`  
**Project:** WX-1 — open wetenschappelijk opslagformaat en opslag-engine  
**Huidige door de gebruiker geverifieerde basis:** v0.4.3, WSL  
**Status:** v0.4.3 is door de gebruiker geverifieerd met 108 workspace-tests, formatting, Clippy met `-D warnings`, CLI- en regressiecontroles, benchmarktarget-build en twee uitgevoerde benchmarklocaties (`/tmp` en `/mnt/d`). De benchmark is een voorlopige interne synthetische baseline, geen multi-format benchmarkacceptatie. Fase 1 heeft nu een experimenteel PyO3/maturin- en Python-prototype; native compilatie, wheel, echte NumPy-roundtrip, Dask en externe optionele adapters zijn nog niet volledig geverifieerd.  
**Laatste Fase-1 WSL-voortgang:** de PyO3-crate compileerde nadat `LD_LIBRARY_PATH` naar de Conda-omgeving was gezet; 4 native tests en 112 workspace-tests slaagden. Clippy meldde twee `manual_contains`-diagnostics; phase1-fix1 bevatte de equivalente `.contains(&0)`-correcties. Daarna faalde de volledige Python-suite in WSL met 20 passed, 2 failed en 2 skipped. De twee fouten betroffen de xarray-backend `**kwargs`-signature en het doorgeven van een backend-instance in plaats van een class. Phase1-fix2 corrigeert beide en wacht op WSL-herverificatie. Wheel-/NumPy-/Dask-/externe adaptergates blijven open.  
**Canonieke actuele planning:** `WX1_ROADMAP_CURRENT.md`; feitelijke status: `WX1_CURRENT_STATUS.md`; gecombineerde opdracht: `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md`. Historische fasen hieronder blijven als projectgeschiedenis behouden.

---

## 1. Projectdoel

WX-1 wordt een open, platformonafhankelijk opslagformaat en opslag-engine voor wetenschappelijke multidimensionale gegevens. De eerste belangrijke toepassing is meteorologische en klimatologische data, maar het formaat moet ook bruikbaar zijn voor andere wetenschappelijke datasets.

WX-1 moet uiteindelijk:

- grote datasets efficiënt kunnen opslaan en gedeeltelijk uitlezen;
- metadata, dimensies, coördinaten, variabelen en tijdinformatie betrouwbaar vastleggen;
- meerdere compressiemethoden ondersteunen;
- data-integriteit en herstel na onderbroken schrijfacties bieden;
- indexen gebruiken om relevante chunks snel te vinden;
- met beperkte RAM en GPU-geheugen kunnen werken;
- vanuit Rust en Python bruikbaar zijn;
- interoperabel zijn met bestaande wetenschappelijke ecosystemen;
- een stabiele, volledig gedocumenteerde specificatie hebben.

De prioriteit is eerst een correct, duurzaam en controleerbaar opslagformaat. Optimalisatie en GPU-versnelling komen pas nadat de opslagsemantiek en foutafhandeling betrouwbaar zijn.

## 2. Historische uitgangspositie: v0.4.1-fix1

De gebruiker heeft v0.4.1-fix1 in WSL gebouwd en bevestigd:

- `cargo fmt --all` en `cargo fmt --all -- --check` geslaagd;
- `cargo test --workspace`: 77 `wx1-format`-unit-tests, 8 wetenschappelijke array-integratietests, 7 scientific-streaming-integratietests, 12 `wx1-registry`-tests en 1 CLI-workflowtest geslaagd (105 totaal);
- Clippy met `-D warnings` geslaagd zonder waarschuwingen;
- v0.3.9 minor-5 en v0.4.x minor-6 profielregressies geslaagd;
- CLI-help en de wetenschappelijke create/inspect/validate/slice-workflow werken;
- no-overwrite, ongeldige selectie, afwijzen van append op wetenschappelijke schema-v2-bestanden en generieke generatie-/indexregressies zijn handmatig bevestigd.

Dit blijft een experimentele basis en geen stabiele interoperabiliteitsstandaard.

### Huidige beperkingen

1. v0.3.9 tijd-, verticale en ruimtelijke indexsleutels blijven expliciete ordinal/opaque identifiers. Kalender-, fysieke niveau- en CRS-semantiek worden als afzonderlijke metadata in v0.4.0 toegevoegd; de index-wire-layout is niet gewijzigd.
2. Tijd- en verticale coördinaten, eenheden en CRS worden opgeslagen maar niet automatisch omgerekend, genormaliseerd of ruimtelijk bevraagd.
3. GRIB1/GRIB2, ecCodes, NetCDF, Zarr, Python/xarray en officiële WMO/provider-tabellen zijn nog niet geïmplementeerd.
4. Indexverwijdering, wijziging/vervanging van bestaande mappings, compaction, vacuum en index-repair ontbreken.
5. LZ4-block-encoding en -decoding materialiseren één volledige begrensde chunk (maximaal 64 MiB); LZ4 sub-chunk streaming en parallelle codec-executie zijn nog niet geïmplementeerd.
6. Het formaat, de wire-layouts en metadata-/indexstructuren zijn experimenteel en kunnen vóór v1.0 wijzigen.

---

## 3. Ontwerpprincipes

### 3.1 Correctheid vóór snelheid

- Geen stille datacorruptie.
- Geen stilzwijgende interpretatie van onbekende verplichte features.
- Geen onveilige aannames over offsets, lengtes, dimensies of integer-overflow.
- Elke lees- en schrijfoperatie valideert de relevante grenzen.
- Foutmeldingen moeten het probleem zo specifiek mogelijk benoemen.

### 3.2 Begrensd geheugengebruik

- Geen vereiste om een volledige dataset in RAM te laden.
- Lees en schrijf chunks en bereiken waar mogelijk incrementeel.
- Stel expliciete limieten in voor metadata, directory, chunkgrootte, buffers en decompressie.
- Maak batchgrootte en bufferomvang configureerbaar.
- Voorkom ongecontroleerde allocaties op basis van waarden uit een bestand.

### 3.3 Duurzaamheid en herstelbaarheid

- Gebruik expliciete commitrecords en controleerbare checksums/hashes.
- Schrijf nieuwe generaties zo dat een eerdere geldige generatie niet voortijdig verloren gaat.
- Maak herstelgedrag deterministisch.
- Publiceer nieuwe bestanden via tijdelijke bestanden en atomische rename waar het platform dit ondersteunt.
- Documenteer verschillen tussen atomische publicatie en herstel van append-only bestanden.

### 3.4 Compatibiliteit

- Gebruik expliciete major- en minor-versies.
- Behoud onbekende optionele metadata waar mogelijk.
- Weiger onbekende verplichte features.
- Definieer byte-order expliciet; little-endian is de standaard, big-endian wordt ondersteund.
- Wijzig bestaande wire-formatvelden niet stilzwijgend.

### 3.5 Wetenschappelijke betekenis behouden

- Bewaar parameter-ID's, broncodes, tabelversies, eenheden en herkomst.
- Maak onderscheid tussen ontbrekende, onbekende en niet-ondersteunde parameters.
- Pas geen eenheidsconversies of semantische wijzigingen stilzwijgend toe.
- Bewaar voldoende provenance om de oorsprong en interpretatie van data te reconstrueren.

---

## 4. Historische roadmap

**Let op:** de onderstaande versiehistorie blijft behouden als ontwikkelgeschiedenis. De actuele volgorde, gates en nog openstaande onderdelen zijn leidend vastgelegd in `WX1_ROADMAP_CURRENT.md`. Die actuele roadmap plaatst Python/CPU-only/NumPy/xarray/Dask/adapters vóór de formele multi-format benchmarkfase.


## Fase A — v0.3.8: duurzame directory en commitgeneraties

**Doel:** de huidige experimentele container omzetten naar een container met een duurzame directory en herstelbare commits.

**Status:** door de gebruiker geverifieerd in v0.3.8-fix1. De 72-byte directory-v2 descriptor, per-chunk BLAKE3, footer-gebaseerde generaties, herstel en v1-compatibiliteit vormen de bevestigde basis. Zie `docs/spec/WX1_Durable_Directory_and_Generations_Draft_v0.3.8.md` en ADR 0014.

### Werkpakketten

1. **Persistent chunkdirectory-formaat**
   - Leg de directory-layout exact vast.
   - Definieer versie, entry-count, offsets, lengtes, codec, flags en integriteitsvelden.
   - Valideer sortering, unieke chunk-ID's, overlappende bereiken en bestandgrenzen.
   - Stel harde limieten vast voor directorygrootte en aantal entries.

2. **BLAKE3 per chunk**
   - Sla een BLAKE3-digest per chunk duurzaam op.
   - Definieer exact of de hash betrekking heeft op opgeslagen bytes of ongecomprimeerde bytes.
   - Gebruik één consistente keuze in de specificatie en implementatie.
   - Behoud CRC32C voor snelle foutdetectie.
   - Documenteer dat de commit-hash en chunk-hash verschillende doelen dienen.

3. **Append-only commitgeneraties**
   - Definieer generatie-ID's en commitvolgorde.
   - Schrijf nieuwe directory- en commitinformatie zonder een eerdere geldige generatie te overschrijven.
   - Maak een commit pas zichtbaar nadat alle vereiste data duurzaam is geschreven.
   - Definieer flush- en sync-verplichtingen.

4. **Herstel**
   - Scan bij openen naar geldige commitrecords.
   - Kies deterministisch de laatst volledig geldige generatie.
   - Negeer onvolledige trailing writes zonder geldige eerdere data te verliezen.
   - Geef een duidelijke fout als geen geldige generatie gevonden wordt.
   - Bied waar passend een expliciete inspectie-/repairmodus; repareer nooit stilzwijgend.

5. **Tests**
   - Afgebroken write op meerdere punten.
   - Truncated directory en footer.
   - Ongeldige offsets en integer-overflow.
   - Beschadigde chunkdata en hash mismatch.
   - Geldige vorige generatie gevolgd door ongeldige nieuwe generatie.
   - Meerdere succesvolle generaties.
   - Zowel little-endian als big-endian.

### Acceptatiecriteria

- Een onderbroken nieuwe commit maakt een eerdere geldige generatie niet onleesbaar.
- Een bestand opent uitsluitend op basis van een volledig gevalideerde generatie.
- Elke chunk kan afzonderlijk op integriteit worden gecontroleerd.
- Directorygegevens worden nooit blind vertrouwd.
- Tests bewijzen herstelgedrag, niet alleen het normale schrijf-/leespad.
- `fmt`, workspace-tests en Clippy met `-D warnings` slagen.

---

## Fase B — v0.3.9: persistente indexen

**Doel:** chunks opzoeken via persistente, generation-pinned indexen zonder de volledige directory te scannen. De primaire directory blijft de gezaghebbende bron.

**Status:** door de gebruiker in WSL geverifieerd in v0.3.9-fix3. De release bevat footer v2 (80 bytes), 4096-byte index-/catalogpagina's, een verplichte chunk-ID-B+tree, optionele caller-keyed secundaire B+trees, copy-on-write-inserties/splitsingen en exact-/range-lookups. De fix3-release corrigeerde de gevonden fixture-scope- en Clippy-problemen. In WSL zijn 87 workspace-tests, fmt, Clippy met `-D warnings`, CLI-help en de handmatige index-, append-, fallback-, corruptie- en repairtests geslaagd. Productiewire-format en ADR 0015 blijven ongewijzigd.

### Indexen en scope van v0.3.9

1. **Chunk-ID-index**
   - Verplichte unieke chunk-ID naar de primaire directory.
   - Moet exact dezelfde zichtbare chunk-ID-set bevatten als de volledige directoryketen.

2. **Variabele-index**
   - Caller-provided variabele-ID naar één of meer chunkverwijzingen.
   - De canonieke variabeledefinitie en providercontext worden later door het wetenschappelijke datamodel/register geleverd.

3. **Tijdindex**
   - Caller-provided unsigned ordinal in v0.3.9; dit is nadrukkelijk geen tijdstempel of interval.
   - Kalender, referentietijd, geldigheidstijd en intervalsemantiek volgen in v0.4.0.

4. **Verticale index**
   - Caller-provided level ordinal in v0.3.9; dit is geen druk/hoogte/modelniveau of fysieke eenheid.
   - Type, waarde en eenheid volgen in v0.4.0.

5. **Ruimtelijke tile-index**
   - Caller-provided opaque/unsigned tile ID in v0.3.9; geen geometrie- of CRS-query.
   - Grid, CRS, asvolgorde, longitude-wrapping en bounding boxes volgen in v0.4.0.

6. **Generieke persistente B+tree**
   - Vaste 4096-byte pagina's met page-level BLAKE3.
   - Footer v2 bevat de catalogus-pointer; de mutable header blijft onveranderd.
   - Immutable pages en path-copy inserties; bestaande subtrees kunnen tussen generaties worden hergebruikt.
   - Exact-key en inclusieve byte-key range lookups.
   - Delete, replacement, merges, compaction, vacuum en index-repair zijn buiten scope.

### Acceptatiecriteria

- Indexresultaten worden altijd gevalideerd tegen de primaire directory.
- De verplichte chunk-ID-index komt exact overeen met de zichtbare directory.
- Een ontbrekende optionele index geeft een specifieke `IndexNotPresent`-fout; een corrupte index wordt niet stilzwijgend vertrouwd.
- Een nieuwe generatie publiceert chunks, directory, gewijzigde indexpagina's en catalogus vóór de footer; eerdere generaties blijven leesbaar.
- Ongewijzigde indexsubtrees worden niet overschreven; gepinde snapshots blijven hun oude root gebruiken.
- Tests dekken byte-orders, pagina-integriteit, splitsingen op leaf- en rootniveau, duplicate keys, ontbrekende chunkreferenties, corruptie, fallback en writer failure op iedere append-byte.
- `fmt`, workspace-tests en Clippy met `-D warnings` slagen.
- De v0.3.8-minor-4/footer-v1 en legacy v1-directory/`WX1C`-readpaden blijven werken. Append naar v0.3.8 is niet ondersteund; herschrijven naar een nieuw v0.3.9-bestand is vereist.

## Fase C — v0.4.0: wetenschappelijk datamodel en multidimensionale chunks

**Doel:** van generieke byte-chunks naar wetenschappelijke arrays en variabelen.

**Status:** v0.4.0-fix1 is door de gebruiker in WSL geverifieerd (97 workspace-tests, fmt/fmt-check en Clippy geslaagd; CLI- en container-/appendregressies geslaagd). Zie `docs/spec/WX1_Scientific_Datamodel_Draft_v0.4.0.md` en ADR 0016.

### Werkpakketten

- Formeel datamodel voor datasets, groepen, variabelen, dimensies en coördinaten.
- Duidelijke scheiding tussen metadata en payload.
- Datatypes voor gehele getallen, floats, booleans en waar nodig complexere wetenschappelijke types.
- Expliciete missing-value- en fill-value-semantiek.
- Endianness en numerieke representatie.
- Chunkvormen per dimensie.
- Dimensies zoals tijd, hoogte/niveau, latitude, longitude en ensemble.
- Ondersteuning voor scalar-, 1D-, 2D-, 3D- en hogere-dimensionale variabelen.
- Validatie dat chunkvormen en offsets overeenkomen met variabeledefinities.
- Metadata voor units, standaardnamen, long names, CRS/grid mapping en provenance.
- Duidelijke regels voor attributen en onbekende metadata.

### Acceptatiecriteria

- Een variabele kan als multidimensionale array worden beschreven, geschreven en teruggelezen via schema-v2 metadata en de bestaande generation writer.
- Deelgebieden lezen uitsluitend de intersecterende chunks; elke geselecteerde chunk wordt nog volledig gedecodeerd.
- Shape, datatype, payload-byte-order, chunkindeling, fill-/missing-values en modelmetadata blijven expliciet behouden.
- De kandidaat bevat tests voor randchunks, gedeeltelijke reads, beide payload-byte-orders, ongeldige chunkdekking, ontbrekende chunks, datatype-/lengtevalidatie en dimension-product-overflow.
- Rank-zero scalars en zero-length-dimensies hebben expliciete schrijf-/leessemantiek en regressietests.
- De 64 MiB array-outputlimiet en output-productoverflows worden zonder grote allocatie getest.
- Alle bovenstaande v0.4.0-tests en profiel-/CLI-/containerregressies zijn door de gebruiker in WSL uitgevoerd en geslaagd in `v0.4.0-fix1`.

---

## Fase D — v0.4.1: begrensde generatie-codecpijplijn en wetenschappelijke CLI

**Doel:** de wetenschappelijke Rust-API via een begrensd generatie-schrijfpad en een bruikbare native CLI end-to-end beschikbaar maken.

**Status:** implementatie en conformance-tests opgenomen in `v0.4.1-fix1`; de gebruiker heeft fix1 in WSL bevestigd met 105 workspace-tests, fmt, Clippy en handmatige CLI-/indexregressies. Zie `docs/spec/WX1_Bounded_Codec_Pipeline_and_Scientific_CLI_Draft_v0.4.1.md` en ADR 0017.

### Werkpakketten

- Reader-based generation writer als additive API, met configureerbare buffer van 1 byte tot 8 MiB.
- Raw en Zstandard via een bounded stream buffer schrijven.
- LZ4-block behouden en maximaal één raw chunk materialiseren, begrensd op 64 MiB; geen claim van sub-chunk streaming.
- CRC32C en BLAKE3 over exact de opgeslagen bytes tijdens het schrijven berekenen; bestaande generation digest en commitvolgorde behouden.
- Wetenschappelijk chunk-ID-, shape- en raw-length-validatie vóór schrijven; Bool8-payloads tijdens de stream op waarden 0/1 controleren.
- Atomic file publication voor de reader-based generation writer, inclusief tijdelijke-file cleanup bij fouten.
- CLI-workflow `create-scientific-demo`, `validate-scientific` en `read-array-slice`.
- CLI-selecties blijven begrensd op 64 MiB; geselecteerde gecomprimeerde chunks worden nog volledig gedecodeerd.

### Acceptatiecriteria voor v0.4.1

- Buffer- en raw-chunklimieten worden vóór allocatie of schrijven afgedwongen.
- Bron-streams die voortijdig eindigen, onjuiste wetenschappelijke raw-lengtes en ongeldige Bool8-bytes leveren geen geldige generatie-footer op.
- Reader-writer-roundtrip voor Raw, Zstandard en LZ4 werkt, inclusief little-/big-endian containerheaders.
- Bestaande v0.3.8-, v0.3.9- en v0.4.0-compatibiliteitstests blijven slagen.
- CLI maakt een deterministisch wetenschappelijk bestand, inspecteert/valideert alle chunks en extraheert een exact controleerbare arrayselectie.
- Format, workspace-tests en Clippy met `-D warnings` slagen.

### Expliciet doorgeschoven

- LZ4 frame/sub-chunk streaming: vereist een aparte codec-identiteit en wire-formatbeslissing; bestaande LZ4-block bytes blijven ongewijzigd.
- Parallelle codec-workers, bounded queues en backpressure: optionele uitbreiding, niet nodig voor het single-worker bounded pad in v0.4.1.
- Benchmarks voor throughput, latency, CPU en RAM: apart prestatieonderzoek na functionele verificatie.
- Streaming van de selectie-output naar de uiteindelijke CLI-file; nu wordt maximaal 64 MiB als `ScientificArray` gematerialiseerd.

Deze uitgestelde onderdelen mogen niet als geïmplementeerd worden gerapporteerd.

---

## Fase D2 — v0.4.2: wetenschappelijke leesplanning en herstelhardening

**Doel:** het wetenschappelijke leespad beter inspecteerbaar en herbruikbaar maken, en aanvullende regressiebewijzen toevoegen voor grote chunkverzamelingen en corrupte/afgebroken wetenschappelijke bestanden. Geen wire-formatwijziging.

**Eerste implementatie in deze kandidaat:**

- Additieve `ArrayReadPlan`- en `ScientificSnapshot::plan_array_slice`-API; de planner valideert rang, dimensiegrenzen en de bestaande uitvoerlimiet van 64 MiB.
- De planner retourneert alleen chunk-ID's die de gevraagde selectie raken. `read_array_slice` gebruikt dezelfde planner, zodat de publiek inspecteerbare planning en de feitelijke leesroute gelijk blijven.
- Regressietest met een 64×64 U16-array in 256 4×4-chunks; een 3×3-selectie die vier chunks kruist moet exact vier chunk-ID's plannen en de verwachte row-major bytes opleveren.
- Wetenschappelijke payload-corruptietest die CRC32C-detectie tijdens het openen controleert.
- Hersteltest voor een wetenschappelijk bestand met een onafgemaakte tail; openen behoudt de laatst geldige generatie en expliciete repair kapt alleen de tail af.

**Acceptatiestatus:** door de gebruiker in WSL geverifieerd. Alle 108 workspace-tests, formatting, Clippy met `-D warnings`, CLI-help, de volledige wetenschappelijke demo/slice- en corruptie-/herstelregressies, compatibiliteit, overwrite-/append-guards en de generieke generatie-/indexcontrole zijn geslaagd.

**Doorgeschoven naar v0.4.3 en latere fasen:**

- Representatieve throughput-, latency- en RAM-benchmarks voor Raw, Zstandard en LZ4 (benchmarkharnas gestart in fase D3).
- Geavanceerdere chunkplanning/zoekstructuren voor zeer grote chunk maps; de eerste planner loopt de variabele chunkreferenties lineair af.
- Uitbreiding van tijd-, verticale en ruimtelijke selectie pas nadat kalender-, eenheden-, asvolgorde-, longitude-wrapping- en CRS-semantiek expliciet zijn vastgelegd.
- Verdere ontwerpbeoordeling voor een stabiele importer-API; de nieuwe Rust API is experimenteel en vormt nog geen stabiel extern contract.

GRIB2/ecCodes-import blijft geblokkeerd tot deze semantiek en importer-contracten zijn beoordeeld. Geen GRIB2-, NetCDF-, Zarr-, Python- of GPU-integratie wordt in v0.4.2 geclaimd.


## Fase D3 — v0.4.3: prestatie- en geheugenbaseline (door gebruiker geverifieerd)

**Doel:** reproduceerbare metingen verzamelen voordat de arrayplanner, chunkindeling of codec-buffers worden geoptimaliseerd. Correctheid en begrensd geheugen blijven leidend. Geen wire-formatwijziging.

**Eerste implementatie in deze kandidaat:**

- Dependency-vrije Cargo bench target `wx1_performance` in `wx1-format`.
- Drie geïsoleerde codecworkloads: Raw, Zstandard en LZ4, ieder op een deterministisch synthetisch 2048×2048 F32-veld verdeeld over 64 chunks van 256×256.
- Metingen van writerlatentie/throughput, open-/validatietijd, volledige array-read, 256×256 window-read, bestandsgrootteverhouding en Linux process peak RSS.
- Aparte plannerworkload met 4096 chunks van 32×32 en herhaalde 256×256-selectieplanning; meet de huidige lineaire planner en leest dezelfde selectie.
- `WX1_BENCH_DIR` voor keuze van het tijdelijke benchmarkbestandssysteem; elk meetgeval draait in een apart child process zodat `VmHWM` per workload kan worden gerapporteerd.
- Geen extra crate dependency; geen aanpassing van het bestandsformaat of de bestaande lees-/schrijfsemantiek.

**Door gebruiker aangeleverde verificatie:** WSL fmt/fmt-check, 108 workspace-tests, Clippy, CLI-/regressiecontroles, `cargo bench --no-run` en benchmarkuitvoering op `/tmp` en `/mnt/d` zijn geslaagd. De gemeten cijfers en beperkingen staan in `WX1_CURRENT_STATUS.md`.

**Nog niet voltooid:** vijf onafhankelijke volledige runs, 30 koude/warme reads, cachecontrole, echte meteorologische data, externe formaatvergelijkingen, CPU-affinity/temperatuurregistratie en CSV/JSON-runarchief. Deze eerdere interne benchmark sluit de formele Fase-2-suite dus niet af.

De benchmark gebruikt synthetische data; `write_mib_s` omvat compressie, integriteitsberekeningen, indexopbouw en reguliere bestandswrites maar geen `fsync`. `file_ratio` omvat metadata, indexen en footers. `VmHWM` is proces-high-water-RSS, geen per-functie allocatiemeting. Zie de v0.4.3-benchmarkdraft en ADR 0019.

---

## Fase E — v0.5.0: Python-API en wetenschappelijke integraties

**Doel:** WX-1 praktisch bruikbaar maken in wetenschappelijke workflows.

### Werkpakketten

- Rust core als primaire implementatie.
- Python bindings via PyO3 en maturin.
- Python package met stabiele, begrijpelijke API.
- NumPy arrays lezen/schrijven.
- xarray backend/engine.
- Dask lazy en chunked execution.
- pandas waar tabelvormige metadata of tijdreeksen passend zijn.
- MetPy voor meteorologische berekeningen in de Python/research-laag.
- cf-xarray voor CF-conventies waar passend.
- Import/export adapters voor NetCDF, Zarr en GRIB; adapters mogen optionele dependencies zijn.
- Roundtrip- en interoperabiliteitstests met representatieve datasets.

### Acceptatiecriteria

- Python kan bestanden openen, metadata inspecteren, variabelen lezen en schrijven.
- NumPy- en xarray-integraties respecteren lazy/chunked gedrag.
- Dask kan grotere datasets verwerken zonder volledige materialisatie.
- Conversies documenteren welke metadata exact behouden blijven en welke beperkingen bestaan.

---

## Fase F — v0.5.x: parameterregisters en meteorologische semantiek

**Doel:** betrouwbare ondersteuning van officiële en eigen parameterdefinities.

### Werkpakketten

- WMO- en GRIB1/GRIB2-identificatie behouden.
- Providercontext ondersteunen voor ECMWF, KNMI, NOAA, DWD, Météo-France, UK Met Office en andere bronnen.
- Tabelversies en bronprovenance opslaan.
- CSV, JSON, YAML, TOML en Excel import/export waar haalbaar.
- Validatie, diff, versiebeheer en expliciete overlays.
- Onbekende codes behouden zonder ze een verzonnen betekenis te geven.
- Geen stille overschrijving van officiële tabellen.
- Eenheden en dimensies valideren; alleen aantoonbaar compatibele conversies toestaan.

### Acceptatiecriteria

- Twee parameters met dezelfde numerieke code maar verschillende tabellen/providers blijven onderscheidbaar.
- De oorspronkelijke code en broncontext zijn reproduceerbaar.
- Overrides zijn expliciet, versieerbaar en voorzien van provenance.
- Roundtrip- en compatibiliteitstests zijn aanwezig.

---

## Fase G — v0.6.x: parallelle verwerking en GPU

**Doel:** schaalbare verwerking op CPU en optioneel GPU.

### Werkpakketten

- CPU blijft altijd een volledig ondersteunde backend.
- Selecteerbare CPU-only, GPU-only waar ondersteund, en hybride uitvoering.
- Capability-detectie per backend en bewerking.
- Geen stilzwijgende GPU-fallback zonder zichtbare status of expliciete configuratie.
- Begrensd VRAM-gebruik; ontwerp en test met 6 GB VRAM als praktisch doel.
- Batchverwerking en streaming tussen opslag, RAM en GPU.
- GPU-ondersteuning alleen voor bewerkingen waarvoor een correcte en zinvolle implementatie bestaat.
- Deterministische resultaten waar de numerieke bewerking dat kan garanderen.
- Vergelijk CPU- en GPU-resultaten met vastgelegde toleranties.

### Acceptatiecriteria

- CPU-uitvoering werkt zonder GPU-runtime.
- GPU-capaciteit wordt gecontroleerd vóór uitvoering.
- VRAM-limieten en batchgroottes zijn instelbaar.
- Fallbacks en niet-ondersteunde bewerkingen zijn expliciet.
- Correctheidstests gaan vóór benchmarks.

---

## Fase H — v1.0.0: stabiele specificatie en productiegeschiktheid

**Doel:** een stabiele, gedocumenteerde en interoperabele eerste hoofdversie.

### Vereisten voor v1.0

- Formele wire-format-specificatie.
- Vastgelegde compatibiliteitsregels.
- Referentie-implementatie en conformance tests.
- Fuzzing van parsers, metadata, directory, indexen en decompressie.
- Security review van alle bestandsgestuurde allocaties en offsets.
- Crash/recovery-tests.
- Cross-platform tests.
- Reproduceerbare builds en release-artifacts.
- Documentatie voor implementers en gebruikers.
- Migratie-/compatibiliteitsbeleid.
- Licentie, CONTRIBUTING en releaseproces op orde.

Geen v1.0-release zolang kritieke herstel-, integriteits- of compatibiliteitsproblemen openstaan.

---

## 5. Teststrategie voor iedere release

Voer minimaal uit:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p wx1-cli -- --help
```

Voeg per nieuwe functionaliteit gerichte tests toe.

### Verplichte testcategorieën

- Normale roundtrip.
- Truncated en malformed files.
- Onjuiste offsets, lengtes en integer-overflow.
- Byte-order little-endian en big-endian.
- Beschadigde metadata, directory, index, chunk en commit.
- CRC32C- en BLAKE3-validatie.
- Limieten voor RAM, buffers, metadata, chunks en decompressie.
- Herstel na onderbroken writes.
- Compatibiliteit met oudere fixtures.
- Fuzzing voor alle parser-ingangen zodra de basis stabiel is.

### Rapportage

Een release is pas als getest aangemerkt wanneer de daadwerkelijke uitvoer is gecontroleerd. Vermeld altijd:

- exacte release/commit;
- commando's;
- aantal geslaagde en mislukte tests;
- Clippy- en fmt-resultaat;
- CLI-/integratietests;
- bekende beperkingen;
- tests die de gebruiker nog zelf moet uitvoeren.

Maak onderscheid tussen:
- **gebouwd**;
- **lokaal getest door ontwikkelaar**;
- **door gebruiker getest**;
- **niet getest**.

---

## 6. Releasebeleid

- Gebruik SemVer zodra de compatibiliteitsregels formeel zijn vastgesteld.
- Tot v1.0 mogen experimentele onderdelen wijzigen, maar breaking changes moeten worden gedocumenteerd.
- Elke release krijgt release notes en een bijgewerkte changelog.
- ZIP-archieven bevatten één duidelijk benoemde projectmap.
- Geen tijdelijke buildbestanden, lokale caches of persoonlijke data in release-archieven.
- Voeg testcommando's en bekende beperkingen toe aan iedere release.
- Maak geen claim dat een release volledig is als onderdelen alleen als ontwerp of stub bestaan.

## 7. Directe volgende stap

De gebruiker heeft **v0.4.2** in WSL bevestigd: 108 workspace-tests, fmt/fmt-check, Clippy met `-D warnings`, de volledige wetenschappelijke regressiehand-off en generieke generatie-/indexcontrole zijn geslaagd. De actieve kandidaat is **v0.4.3 — reproduceerbare prestatie- en geheugenbaseline**.

De volgende verplichte WSL-verificatie is:

1. `cargo fmt --all`, format-check, `cargo test --workspace` en Clippy met `-D warnings`.
2. `cargo bench -p wx1-format --bench wx1_performance --no-run` om het custom bench target expliciet te bouwen.
3. `cargo bench -p wx1-format --bench wx1_performance` om Raw, Zstandard, LZ4 en 4096-chunk planner workloads uit te voeren.
4. Volledige benchmarkuitvoer bewaren met machine-/filesystemcontext; benchmarkoutput is geen pass/fail-test en levert nog geen algemene performanceclaim op.
5. Op basis van metingen één geprioriteerde optimalisatie kiezen.

De planner blijft lineair en geselecteerde gecomprimeerde chunks worden volledig gedecodeerd totdat de benchmark een concrete optimalisatierichting onderbouwt. GRIB2/ecCodes-import blijft afhankelijk van expliciete kalender-, niveau-, CRS- en importersemantiek; Python en GPU blijven latere roadmapfasen.
