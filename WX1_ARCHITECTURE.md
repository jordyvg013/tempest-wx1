# WX-1 — Architectuurbeslissingen

**Document:** `ARCHITECTURE.md`  
**Project:** WX-1 — open wetenschappelijk opslagformaat en opslag-engine  
**Basis:** historische architectuurkeuzes tot en met v0.3.7-fix3; actuele implementatiestatus staat in `WX1_CURRENT_STATUS.md` en de leidende roadmap in `WX1_ROADMAP_CURRENT.md`  
**Status:** architectuurkader; onderdelen die nog niet volledig geïmplementeerd zijn, zijn als zodanig gemarkeerd.

---

## 1. Doel en status van dit document

Dit document legt de gekozen architectuurrichting van WX-1 vast. Het is bedoeld als leidraad voor implementatie, code reviews, specificaties en toekomstige Architecture Decision Records (ADR's).

Een architectuurbeslissing is niet automatisch een bewijs dat de functionaliteit al bestaat. Daarom gebruikt dit document drie statussen:

- **Gekozen:** richting is vastgesteld.
- **Gedeeltelijk geïmplementeerd:** er is werkende code, maar de volledige doelarchitectuur ontbreekt nog.
- **Gepland:** keuze of ontwerp moet nog technisch worden uitgewerkt en gevalideerd.

Bij strijdigheid geldt deze volgorde:
1. De formele wire-format-specificatie voor de exacte bytes en validatieregels.
2. Geaccepteerde ADR's voor de motivatie en evolutie van keuzes.
3. Dit architectuurdocument voor het overkoepelende ontwerp.
4. De implementatie, die bij afwijking moet worden gecorrigeerd of gedocumenteerd.

Een ADR mag een eerdere beslissing vervangen, maar moet expliciet vermelden wat verandert en waarom.

---

## 2. Architectuur in hoofdlijnen

WX-1 wordt opgebouwd uit een kleine, betrouwbare opslagkern met daaromheen gespecialiseerde lagen.

```text
┌──────────────────────────────────────────────────────────────┐
│                     Gebruikersinterfaces                     │
│        Rust API · Python API · CLI · toekomstige tools       │
├──────────────────────────────────────────────────────────────┤
│                  Wetenschappelijke integratie                │
│ NumPy · xarray · Dask · pandas · MetPy · CF · adapters        │
├──────────────────────────────────────────────────────────────┤
│                    Query- en array-engine                    │
│ Variabele/tijd/ruimte/niveau selectie · batching · planning   │
├──────────────────────────────────────────────────────────────┤
│                     Index- en directorylaag                  │
│ Chunk directory · B+tree · tijd · variabele · ruimte · niveau│
├──────────────────────────────────────────────────────────────┤
│                       Container-engine                       │
│ Header · metadata · chunks · commits · herstel · validatie   │
├──────────────────────────────────────────────────────────────┤
│                        Codec-laag                             │
│ Raw · Zstandard · LZ4 · checksums · hashes · streaming        │
├──────────────────────────────────────────────────────────────┤
│                     Platform-/I/O-laag                       │
│ Bestanden · positioned I/O · sync · atomische publicatie     │
└──────────────────────────────────────────────────────────────┘
```

De opslagkern moet zelfstandig kunnen functioneren zonder Python, NumPy, xarray, MetPy, Dask of GPU-runtime. Hogere lagen mogen de kern gebruiken, maar de kern mag niet afhankelijk worden van deze ecosystemen.


## 2A. Actuele engine-eisen uit de leidende roadmap

De volgende punten zijn ontwikkelvereisten, geen bewering dat ze al volledig zijn geïmplementeerd:

- **Open/read/validate:** `open()` moet na de lazy-open-fase beperkte structurele metadata/rootinformatie lezen; `read()` controleert de daadwerkelijk gelezen chunks; `validate()` blijft de expliciete volledige integriteitscontrole. Alleen footer lezen is geen volledige validatie.
- **Python API:** Python is een dunne PyO3/maturin-laag op de Rust-kern. CPU-only blijft onafhankelijk van GPU-pakketten. Bufferownership, kopieën, GIL en resourcelevensduur worden expliciet getest. ADR 0005 is het overkoepelende kader; ADR 0020 werkt het concrete pakket/API-contract uit.
- **Write durability:** buffered write en durable commit zijn afzonderlijke garanties en meetmodi. `flush()` is niet automatisch durable. Bestands- en directorysync worden alleen geclaimd wanneer daadwerkelijk uitgevoerd en getest.
- **Parallelle producers:** onafhankelijke tiles/blokken schrijven naar eigen staginggebieden; een gecontroleerde merge/commit publiceert index/root/footer. Geen ongecoördineerde gedeelde indexmutatie.
- **Planner:** de huidige arrayplanner is lineair. Een geïndexeerde variant wordt pas na de formele nulmeting ontworpen en afzonderlijk benchmark- en correctheidgetest.
- **Benchmarks:** de interne Rust-microbenchmark (ADR 0019) en de cross-format-suite (ADR 0021) hebben verschillende doelen en blijven afzonderlijk gerapporteerd.

## 3. Taalkeuze: Rust als opslagkern

<badge>Gekozen</badge>

Rust is de primaire implementatietaal voor de opslagkern.

### Redenen

- Sterke controle over geheugen en ownership.
- Geschikt voor veilige parsing van binaire bestanden.
- Goede prestaties voor I/O, compressie en indexering.
- Mogelijkheid om een stabiele native library en CLI te leveren.
- FFI-integratie met Python via PyO3.
- Minder noodzaak om kritieke parserlogica in meerdere talen te dupliceren.

### Consequenties

- Wire-format-logica wordt één keer in Rust geïmplementeerd.
- Python gebruikt dezelfde kern via bindings.
- Geen afzonderlijke Python-implementatie van checksums, offsets of directoryvalidatie.
- Publieke Rust API en Python API worden apart ontworpen, maar delen dezelfde semantiek.

C en C++ kunnen via zorgvuldig afgebakende dependencies of FFI worden gebruikt wanneer een aantoonbaar voordeel bestaat. Ze worden niet de primaire opslagkern.

## 4. Scheiding tussen formaat en engine

<badge>Gekozen</badge>

WX-1 maakt onderscheid tussen:

1. **Bestandsformaat:** exacte binaire representatie, metadata, chunks, directory, indexen en commits.
2. **Opslag-engine:** lezen, schrijven, valideren, indexeren, herstel en caching.
3. **Array-/query-engine:** selecteren en combineren van wetenschappelijke data.
4. **Bindings en integraties:** Python, CLI en adapters.

### Redenen

- Andere programmeertalen kunnen het formaat implementeren zonder de Rust-engine te gebruiken.
- De engine kan intern optimaliseren zonder het wire-format te wijzigen.
- De Python-API hoeft niet samen te vallen met de binaire datastructuur.
- Formaatcompatibiliteit kan onafhankelijk worden getest.

### Consequentie

De specificatie moet voldoende precies zijn om een onafhankelijke reader te bouwen. De Rust-implementatie is de referentie-implementatie, niet de enige mogelijke implementatie.

## 5. Containerstructuur

<badge>Gedeeltelijk geïmplementeerd</badge>

Een WX-1-bestand bestaat conceptueel uit:

```text
┌──────────────────────────┐
│ Container header         │
├──────────────────────────┤
│ Metadata (deterministic  │
│ CBOR)                    │
├──────────────────────────┤
│ Chunk payload region(s)  │
├──────────────────────────┤
│ Persistent directory     │
├──────────────────────────┤
│ Index structures         │
├──────────────────────────┤
│ Commit record / footer   │
└──────────────────────────┘
```

De uiteindelijke indeling kan door append-only generaties meerdere directory-, index- en commitrecords bevatten. De exacte layout moet in de wire-format-specificatie worden vastgelegd.

### Header

- Expliciete magic bytes en formaatversie.
- Expliciete byte-order.
- Expliciete flags en reserved bytes.
- Lengtes en offsets met gecontroleerde grenzen.
- Onbekende verplichte features worden afgewezen.
- Reserved bytes worden volgens de specificatie gevalideerd.

De experimentele header van v0.3.3 en latere versies is een eerste implementatiestap; de definitieve header-layout is nog niet als stabiele v1-specificatie vastgesteld.

### Metadata

<badge>Gedeeltelijk geïmplementeerd</badge>

- Deterministische CBOR voor metadata.
- Little-endian als standaard voor numerieke wire-formatvelden, tenzij een veld expliciet anders is gedefinieerd.
- Metadata heeft harde maximale groottes en aantallen.
- Metadata bevat datasetidentiteit, profiel-/feature-informatie en wetenschappelijke beschrijvingen.
- Onbekende optionele metadata blijft waar mogelijk behouden.
- Metadata mag geen impliciete of onbegrensde allocaties veroorzaken.

Deterministische CBOR betekent dat dezelfde logische metadata volgens vastgelegde canonicalisatie dezelfde byte-representatie oplevert. De precieze canonicalisatieprofielkeuze moet in de specificatie eenduidig worden benoemd.

## 6. Chunk-gebaseerde opslag

<badge>Gedeeltelijk geïmplementeerd</badge>

Wetenschappelijke arrays worden in chunks opgeslagen. De chunk is de primaire eenheid voor opslag, compressie en integriteitscontrole.

### Ontwerpkeuzes

- Meerdere chunks per container.
- Iedere chunk heeft een unieke ID binnen de container of dataset-scope zoals de specificatie vastlegt.
- Chunkmetadata bevat ten minste codec, offset, opgeslagen lengte en oorspronkelijke lengte.
- Chunks moeten onafhankelijk leesbaar en valideerbaar zijn.
- De directory is de gezaghebbende bron voor chunklocaties.
- De chunkindeling moet multidimensionale tile-/chunkvormen kunnen beschrijven.

### Chunkgrootte

Er komt geen universele vaste chunkgrootte voor alle datasets. De optimale grootte hangt af van:

- dimensies en vorm van variabelen;
- typische querypatronen;
- compressieverhouding;
- opslagmedium en I/O-latency;
- parallelisme;
- beschikbare RAM en VRAM.

De engine moet instelbare chunkvormen en limieten ondersteunen. Er komen veilige standaardinstellingen en harde bovengrenzen.

## 7. Compressie-architectuur

<badge>Gedeeltelijk geïmplementeerd</badge>

De codec-laag is modulair. Codec-identificatie en codecparameters worden expliciet opgeslagen.

### Gekozen codecs

| Codec | Rol | Status |
|---|---|---|
| Raw | Geen compressie, eenvoudige range access | Geïmplementeerd |
| Zstandard | Standaard lossless compressie | Geïmplementeerd |
| LZ4 block | Snelle lossless compressie | Geïmplementeerd |
| Streaming LZ4 | Beperkt geheugengebruik | Nog uit te werken |

### Principes

- Lossless is de standaard.
- Lossy compressie is nooit impliciet.
- Lossy methoden moeten expliciet in metadata worden aangegeven, inclusief parameters en fout-/tolerantiebeleid.
- Onbekende verplichte codecs worden afgewezen.
- Onbekende optionele codecmetadata mag niet tot onveilige interpretatie leiden.
- De engine valideert opgeslagen en gedecomprimeerde lengtes.
- Decompressie heeft een harde outputlimiet.
- Codecselectie kan per chunk verschillen.

LZ4-block-decodering materialiseert in de huidige experimentele implementatie nog een volledige chunk. De implementatie mag daarom nog niet als volledig streaming voor alle codecs worden beschreven.

## 8. Integriteitsarchitectuur

<badge>Gedeeltelijk geïmplementeerd</badge>

CRC32C en BLAKE3 vervullen verschillende rollen.

### CRC32C

- Snelle foutdetectie op chunkniveau.
- Geschikt om onbedoelde beschadiging vroeg te detecteren.
- De specificatie moet vastleggen of CRC32C de opgeslagen bytes of een andere exact omschreven byte-reeks beschermt.

### BLAKE3

- Sterkere hash voor chunk- en commitintegriteit.
- Een commit-hash beschermt de exact gedefinieerde inhoud van een generatie/bestand.
- Een chunk-hash maakt onafhankelijke validatie van een chunk mogelijk.
- De specificatie moet vastleggen welke bytes worden gehasht, in welke volgorde en met welke domeinscheiding.

### Belangrijke scheiding

Een commit-hash en een chunk-hash zijn niet uitwisselbaar:

- De commit-hash beschermt de samenhang van een generatie.
- De chunk-hash beschermt een afzonderlijke chunk.
- CRC32C biedt snelle foutdetectie, maar vervangt geen cryptografische hash.

In v0.3.7-fix3 bestaat BLAKE3-controle van de commit-footer. Een duurzame BLAKE3-digest per chunk in de persistente directory is nog werk voor een volgende release.

## 9. Crash safety en commitmodel

<badge>Gepland</badge>

De doelarchitectuur gebruikt append-only generaties.

### Beoogd schrijfpad

1. Schrijf nieuwe chunkdata.
2. Valideer de geschreven data.
3. Schrijf een nieuwe directory.
4. Schrijf of update de bijbehorende indexstructuren.
5. Flush/sync de vereiste data.
6. Schrijf het commitrecord als laatste.
7. Flush/sync het commitrecord.
8. Beschouw de generatie pas daarna als gecommit.

### Openen en herstel

- Scan geldige commitrecords volgens de specificatie.
- Valideer generatie, directory, indexen en hashes.
- Kies de laatst volledig geldige generatie.
- Negeer een onvolledige trailing generatie als een eerdere geldige generatie beschikbaar is.
- Geef een duidelijke fout als geen geldige generatie resteert.
- Repareer of herschrijf nooit stilzwijgend.

### Nieuwe bestanden

Voor het publiceren van een volledig nieuw bestand kan een tijdelijke file gevolgd door flush/sync en atomische rename worden gebruikt, waar het platform dit ondersteunt.

Deze aanpak is niet hetzelfde als append-only herstel van bestaande bestanden. Beide mechanismen moeten afzonderlijk worden gespecificeerd en getest.

## 10. Indexarchitectuur

**Status: geïmplementeerd in de experimentele v0.3.9-bron; v0.3.9-fix3 is door de gebruiker in WSL geverifieerd.**

De chunkdirectory blijft de primaire bron van waarheid. Indexen versnellen selectie, maar bepalen niet zelfstandig waar willekeurige bytes gelezen mogen worden.

### Generieke index

De experimentele v0.3.9-implementatie gebruikt een persistente B+tree voor sleutelgebaseerde lookups. De bestaande `ScientificChunkKey` levert expliciete ordinal-keys; de semantiek van timestamps, fysieke niveaus en CRS-geometrie wordt niet in v0.3.9 afgeleid.

Geschikt voor bijvoorbeeld:
- chunk-ID;
- variabele-ID;
- tijdsleutels;
- niveau-/dimensiesleutels;
- samengestelde sleutels.

### Gespecialiseerde indexen

Niet elke query past optimaal in één B+tree. Daarom kunnen gespecialiseerde indexen worden gebruikt:

- ruimtelijke index voor bounding boxes en gridgebieden;
- tijdsintervalindex voor overlapqueries;
- eventueel grid-gebaseerde indexen voor reguliere meteorologische grids.

### Veiligheidsregels

- Indexverwijzingen worden tegen de directory gevalideerd.
- Indexen bevatten versie- en integriteitsinformatie.
- Een beschadigde index mag nooit een onveilige offset opleveren.
- De engine moet een index kunnen afwijzen en, waar praktisch, opnieuw opbouwen uit de primaire directory.
- De v0.3.9-verplichte chunk-ID-index moet exact de primaire directory dekken. Ontbrekende optionele indexen geven `IndexNotPresent`; corrupte pagina’s/catalogi geven specifieke indexfouten. De index is geen fallbackbron voor willekeurige payload-offsets.

## 11. Wetenschappelijk datamodel

<badge>Experimenteel geïmplementeerd in v0.4.0</badge>

WX-1 is een multidimensionaal wetenschappelijk formaat, geen generieke blobcontainer. De exacte schema-v2 en arraychunkregels staan in `docs/spec/WX1_Scientific_Datamodel_Draft_v0.4.0.md` en ADR 0016.

### Conceptuele objecten

- Dataset
- Groep of namespace, indien nodig
- Variabele
- Dimensie
- Coördinaat
- Chunk
- Parameterdefinitie
- Grid-/CRS-definitie
- Provenance-/bronrecord
- Attributen en extensies

### Typische dimensies

- tijd;
- forecast/reference time;
- ensemble/member;
- verticale coördinaat;
- latitude;
- longitude;
- hoogte;
- bodemlaag;
- overige gebruikersdimensies.

### Semantiek

- Tijdkalender, tijdzone-/epochbasis en precisie zijn expliciet.
- Geldigheidstijd en modelrun-/referentietijd zijn aparte begrippen.
- Verticale niveaus bewaren type, waarde en eenheid.
- Ruimtelijke metadata bewaren CRS, gridmapping, asvolgorde en longitude-conventie.
- Missing values en fill values hebben formele semantiek.
- Variabeletypen en numerieke representaties zijn expliciet.

## 12. Parameterregisters en providercontext

<badge>Gedeeltelijk geïmplementeerd</badge>

WX-1 ondersteunt officiële en eigen parameterdefinities via een versieerbaar register.

### Identiteit van een parameter

Een parameter wordt niet uitsluitend door een numerieke code geïdentificeerd. De identiteit kan context bevatten zoals:

- standaard of discipline;
- tabelsoort en tabelnummer;
- tabelversie;
- centre/provider;
- subcentre;
- producttemplate;
- lokale tabelcontext;
- oorspronkelijke externe code.

### Providers

Het ontwerp houdt rekening met onder meer:

- WMO;
- ECMWF;
- KNMI;
- NOAA;
- DWD;
- Météo-France;
- UK Met Office;
- eigen gebruikers-/projecttabellen.

### Import en overrides

- Import uit CSV, JSON, YAML, TOML en Excel is voorzien.
- Officiële definities worden niet stilzwijgend overschreven.
- Overrides zijn expliciet en versieerbaar.
- Nieuwe definities krijgen provenance en namespace.
- Onbekende codes blijven behouden zonder verzonnen betekenis.
- Eenheden worden gecontroleerd op fysieke dimensiecompatibiliteit.

Het register is een semantische laag bovenop de opslagstructuur; het mag de oorspronkelijke bronidentificatie niet vernietigen.

## 13. I/O en opslagbackend

<badge>Gepland</badge>

De I/O-laag abstraheert bestandstoegang en platformverschillen.

### Vereisten

- Positioned reads waar beschikbaar.
- Sequentiële en gedeeltelijke reads.
- Begrensde buffering.
- Correcte flush/sync-semantiek.
- Duidelijke foutafhandeling bij korte reads en writes.
- Ondersteuning voor grote bestanden via geschikte integerbreedtes.
- Geen afhankelijkheid van één besturingssysteem.

Mogelijke toekomstige backends zoals object storage of remote range reads worden pas toegevoegd nadat de lokale bestandssemantiek stabiel is. Het wire format mag niet afhankelijk zijn van een specifieke backend.

## 14. Parallelle verwerking

<badge>Gepland</badge>

Parallelisme wordt bovenop de correcte seriële opslagsemantiek gebouwd.

### Principes

- Chunkoperaties zijn de primaire eenheid van parallel werk.
- Gebruik begrensde workerpools en queues.
- Backpressure voorkomt onbeperkte geheugengroei.
- Writes naar gedeelde directory-/commitstructuren worden gecontroleerd geserialiseerd of via een expliciet concurrencyprotocol uitgevoerd.
- Dezelfde file region mag niet ongecontroleerd door meerdere writers worden gewijzigd.
- Resultaten en fouten blijven deterministisch te rapporteren.

Eerst correcte single-writer- en readersemantiek; daarna parallelle compressie, decompressie en queryplanning.

## 15. CPU- en GPU-architectuur

<badge>Gepland</badge>

CPU is de verplichte basisbackend. GPU is optioneel.

### Uitvoeringsmodi

- CPU-only;
- GPU-only voor expliciet ondersteunde bewerkingen;
- hybride CPU/GPU.

### Capability model

Iedere backend meldt welke bewerkingen, datatypes en codecs worden ondersteund. De planner kiest alleen een backend die de vereiste bewerking correct kan uitvoeren.

### Geheugenbeheer

- VRAM-gebruik is begrensd.
- Batchgrootte is configureerbaar.
- Data wordt waar mogelijk gestreamd tussen opslag, RAM en GPU.
- Ontwerpdoel is bruikbaarheid op systemen met circa 6 GB VRAM, zonder te beloven dat iedere dataset of bewerking daarop past.

### Fallback

Fallback naar CPU moet expliciet configureerbaar en zichtbaar zijn. Een gebruiker mag niet ten onrechte denken dat een bewerking op GPU is uitgevoerd.

GPU-ondersteuning wordt pas toegevoegd nadat de relevante CPU-bewerking correct en getest is.

## 16. Python- en wetenschappelijke integraties

<badge>Gepland</badge>

De Python-laag gebruikt de Rust-kern via PyO3 en maturin.

### Integraties

- NumPy: arrays en dtype-conversies.
- xarray: datasets, variabelen, coördinaten en backend.
- Dask: lazy en distributed/chunked verwerking.
- pandas: tabulaire metadata en tijdreeksen waar passend.
- MetPy: meteorologische berekeningen in de Python/research-laag.
- cf-xarray: CF-conventies en metadata-interoperabiliteit.
- NetCDF, Zarr en GRIB: import/export-adapters.

Deze bibliotheken zijn geen verplichte dependencies van de Rust-opslagkern. Integraties moeten aangeven welke metadata en semantiek exact behouden blijven.

## 17. Versies en compatibiliteit

<badge>Gekozen</badge>

- Gebruik expliciete formaat-major- en minor-versies.
- Major-versiewijzigingen mogen incompatibele wire-formatwijzigingen bevatten.
- Minor-versies voegen alleen compatibele mogelijkheden toe volgens de formele regels.
- Feature flags maken optionele en verplichte mogelijkheden onderscheidbaar.
- Onbekende verplichte features worden geweigerd.
- Onbekende optionele metadata wordt waar mogelijk behouden.
- De engine rapporteert duidelijk wanneer een bestand nieuwer is dan de implementatie ondersteunt.

De precieze compatibiliteitsmatrix moet vóór v1.0 formeel worden vastgelegd.

## 18. Foutmodel en betrouwbaarheid

<badge>Gekozen</badge>

- Fouten zijn expliciet en getypeerd.
- Onderscheid minimaal: I/O-fout, ongeldige header, ongeldige metadata, ongeldige directory, ongeldige index, codec-fout, CRC-mismatch, hash-mismatch, ongeldige commit en niet-ondersteunde feature.
- Een fout op één laag wordt niet onterecht als fout van een andere laag gepresenteerd.
- Geen panics op onbetrouwbare bestandsinput.
- Inspectie is read-only.
- Reparatie is expliciet en gescheiden van inspectie.
- Geen stille dataverlies- of fallbackpaden.

## 19. Security- en robustnessmodel

WX-1-bestanden kunnen van onbekende bronnen komen en worden daarom als onbetrouwbare input behandeld.

Belangrijke risico's:

- integer-overflow;
- out-of-bounds reads/writes;
- grote allocaties;
- decompression bombs;
- corrupte indexverwijzingen;
- malformed CBOR;
- corrupte commitketens;
- resource exhaustion door te veel chunks of metadata;
- kwetsbaarheden in codec-dependencies.

Mitigaties:

- harde limieten;
- checked arithmetic;
- parservalidatie vóór allocatie;
- begrensde decompressie;
- fuzzing;
- dependency-audits;
- corruptie- en crash-tests;
- expliciete foutafhandeling.

## 20. Niet-doelen voor de vroege releases

De volgende zaken zijn geen doel voor v0.3.x:

- Een definitieve v1.0-wire-formatgarantie.
- Een volledig geïmplementeerde persistente B+tree.
- Een volledig geïmplementeerde ruimtelijke index.
- Volledig herstel van append-only generaties.
- Volledig streaminggedrag van iedere codec.
- GPU-versnelling voor alle meteorologische berekeningen.
- Een eigen vervanging voor NumPy, xarray, Dask of MetPy.
- Stilzwijgende conversie van alle externe bestandsformaten zonder semantische verschillen.

Deze functies kunnen later worden toegevoegd volgens de roadmap en ADR-procedure.

---

## 21. Beslissingen die vóór v1.0 formeel moeten worden vastgelegd

1. Definitieve header- en footer-layout.
2. Exacte CBOR-canonicalisatievariant.
3. Definitieve chunk-ID-scope en chunkdescriptor.
4. CRC32C- en BLAKE3-bytebereiken en domeinscheiding.
5. Append-only generatie- en herstelprotocol.
6. Persistente directory- en B+tree-layout.
7. Indexversies en indexherstel.
8. Wetenschappelijke datatype- en missing-value-semantiek.
9. Tijdstandaarden en kalenderrepresentatie.
10. CRS-, grid- en asvolgordeconventies.
11. Parameteridentiteit en providercontext.
12. Compatibiliteitsmatrix en feature-onderhandeling.
13. Security- en resource-limieten.
14. Conformance test suite en onafhankelijke implementatie-eisen.

## 22. Relatie met de roadmap

- **v0.3.8:** duurzame directory, commitgeneraties, herstel en chunkhashes.
- **v0.3.9:** experimentele persistente copy-on-write B+tree-basis; generieke ordinal-key lookups, nog WSL-verificatie nodig.
- **v0.4.0:** formeel multidimensionaal wetenschappelijk datamodel.
- **v0.4.1:** volledig begrensde codec-pijplijn.
- **v0.5.0:** Python API en wetenschappelijke integraties.
- **v0.5.x:** parameterregisters en meteorologische semantiek.
- **v0.6.x:** parallelle verwerking en optionele GPU-backends.
- **v1.0.0:** stabiele specificatie, conformance tests en productiegeschiktheid.

---

## 23. Onderhoud van dit document

Bij iedere architectuurwijziging:

1. Controleer of een bestaande ADR van toepassing is.
2. Maak een nieuwe ADR bij een nieuwe keuze of wijziging van een bestaande keuze.
3. Werk dit document bij als de overkoepelende architectuur verandert.
4. Werk de wire-format-specificatie bij als bytes, velden of validatieregels veranderen.
5. Houd statuslabels eerlijk: gekozen, gedeeltelijk geïmplementeerd of gepland.
6. Vermeld migratie- en compatibiliteitsgevolgen.

---

## Addendum — v0.3.8 duurzame directory en generaties (2026-10-30)

Dit addendum vult de architectuurbasis aan; eerdere ontwerpbeslissingen blijven gelden. De experimentele v0.3.8-implementatie voegt directory schema v2 met 72-byte descriptors en BLAKE3 over exact de opgeslagen chunkbytes toe, naast de bestaande CRC32C. Append-only generaties worden gecommit met `WX1G`-footers; de footer bevat de root/directory-offset en de hash van uitsluitend de eigen generatie. Voor dit profiel blijft `header.directory_offset` nul.

De directory blijft de gezaghebbende bron; dit addendum introduceert geen persistente index of B+tree en wijzigt ADR 0003 niet. De v1-directory en legacy `WX1C`-lezer blijven behouden. Normaal openen en inspecteren zijn read-only; expliciete repair mag alleen bytes na de nieuwste volledig gevalideerde generatie afkappen. Een generatie-snapshot blijft op de geopende footer gepind.

De v0.3.8-file-API ondersteunt één schrijver per bestand tegelijk; callers moeten append- en repair-acties extern serialiseren. De gebruiker heeft v0.3.8-fix1 in WSL bevestigd: 70 workspace-tests, fmt, Clippy met `-D warnings`, CLI-help en de aangeleverde generatie-/herstel-/corruptietests slaagden. Het formaat is nog experimenteel en niet interoperabiliteitsvast. Zie `docs/spec/WX1_Durable_Directory_and_Generations_Draft_v0.3.8.md` en ADR 0014.


## Addendum — v0.3.9 persistente copy-on-write B+tree-indexen (2026-10-30)

De experimentele v0.3.9-bron behoudt de 72-byte directory-v2 descriptor en voegt header minor 5 met `WX1G` footer v2 van 80 bytes toe. Footer v2 verwijst naar de laatste 4096-byte `WX1B` indexcataloguspagina. Indexpagina’s hebben een afzonderlijke BLAKE3-digest; de generatiehash omvat de nieuwe chunks, directory, nieuwe indexpagina’s en catalogus, exclusief de footer. De mutable header blijft `directory_offset = 0` houden.

De verplichte chunk-ID-B+tree moet exact overeenkomen met de volledige zichtbare primaire directory. Secundaire variabele-, tijd-, verticale-, spatial-tile- en model/provider-indexen gebruiken caller-provided fixed-width keys. `ScientificChunkKey` zet alleen expliciete ordinal identifiers om; v0.3.9 kent nog geen timestamp-/kalender-, fysieke niveau-/eenheden- of CRS-/geometriesemantiek. Exact- en inclusieve byte-range-lookups leveren chunk-ID’s die opnieuw tegen de pinned directory worden gecontroleerd.

Indexpagina’s zijn immutable. Append schrijft alleen nieuwe chunkdata, directory, gewijzigde COW-paden en catalogus; ongewijzigde roots mogen naar eerdere gecommitte pagina’s wijzen. Footer v2 commit de nieuwe catalogus. v0.3.8 minor-4/footer-v1 bestanden en legacy v1/`WX1C` blijven leesbaar, maar de v0.3.9-writer append niet op v0.3.8-bestanden. Verwijderen, compaction, vacuum, index-repair, Python, GRIB, NetCDF, Zarr en GPU blijven buiten scope. Zie `docs/spec/WX1_Persistent_BTree_Indexes_Draft_v0.3.9.md` en ADR 0015.

De v0.3.9-specificatie, ADR, tests, implementatie, CLI en documentatie zijn opgenomen. De gebruiker heeft v0.3.9-fix3 in WSL bevestigd: fmt, 87 workspace-tests, Clippy met `-D warnings`, CLI-help en handmatige index-/append-/fallback-/corruptie-/repairtests slaagden. Daarna is v0.4.0-fix1 eveneens door de gebruiker in WSL bevestigd met 97 workspace-tests, fmt, Clippy, profielregressies, CLI-help en generatie-/index-/appendcontroles.

### v0.3.9-fix1 — range traversal guard

Fix1 bindt het `level`-veld van iedere interne B+tree-pagina tijdens range-traversal en vergelijkt dit met het verwachte niveau dat van root/parent komt. Een leaf wordt alleen op niveau 0 geaccepteerd; een interne pagina moet een positief niveau hebben dat exact overeenkomt met het verwachte niveau. Dit sluit een inconsistent child-level af, ook wanneer de gewijzigde pagina een opnieuw berekende geldige pagina-BLAKE3 heeft. De wijziging raakt geen wire-bytes. De gebruiker onderbrak de eerste v0.3.9-verificatie na compilerfout E0027; de latere handmatige testregels zijn door die afbreking niet uitgevoerd. Fix1 wacht nog op volledige WSL-verificatie.


### v0.3.9-fix3 — testfixture en Clippy-correcties

Fix3 verplaatst de synthetische v0.3.8-generatiefixture naar de gedeelde v0.3.8-testmodule zodat de v0.3.9-testmodule haar via de parent module kan hergebruiken. De productie-API en wire-bytes veranderen hierdoor niet. De argumenten van index-subtreevalidatie, range-traversal, copy-on-write-insertie en append-recordschrijven zijn gegroepeerd in expliciete context-/planstructuren. De range-traversal gebruikt het gevalideerde verwachte B+tree-niveau als recursiegrens en houdt de bestaande depth-, visited-page- en page-count-limieten aan. De overige fix3-wijzigingen vervangen `iter().any()` door `contains()`, gebruiken slices waar geen vectorgroei nodig is, gebruiken `BTreeMap::entry()` en vouwen een geneste validatievoorwaarde samen.

De gebruiker rapporteerde voor fix2 dat fmt en fmt-check slaagden, maar de testbuild stopte bij `E0425` en Clippy stopte met die compilefout plus 11 lintdiagnostics. Fix3 corrigeerde de gemelde problemen. De gebruiker heeft daarna fmt, 87 workspace-tests, Clippy met `-D warnings`, CLI-help en de handmatige index-/append-/fallback-/corruptie-/repairtests in WSL bevestigd.


## Addendum — v0.4.0 wetenschappelijk datamodel (2026-10-30)

De door de gebruiker in WSL geverifieerde v0.4.0-fix1-bron voegt een deterministic-CBOR metadata schema v2 toe. Metadata zonder wetenschappelijk model blijft schema v1 byte-voor-byte gebruiken. Schema v2 beschrijft groepen, dimensies/rollen, variabelen, numerieke datatypes, expliciete payload-byte-order, fill-/missing-values, chunkvormen, coördinaten (inclusief tijd- en verticale semantiek), CRS/grid mappings, attributen en provenance. Het model verwijst uitsluitend naar bestaande stabiele chunk-ID's; de 72-byte fysieke descriptor wordt niet uitgebreid.

Arraychunks vormen in v0.4.0 een dichte, complete chunkgrid met row-major volgorde. Scalar arrays hebben één rank-zero chunk; variabelen met een zero-length-dimensie hebben geen chunks. De reader valideert de chunk-map tegen de primaire directory en de gedeclareerde datatypebreedte. `ScientificSnapshot::read_array_slice` haalt uitsluitend intersecterende chunks op, valideert de bestaande checksums/codecs en kopieert het gevraagde deelgebied naar een aaneengesloten resultaat. De uitvoerlimiet is 64 MiB; een geselecteerde gecomprimeerde chunk wordt nog volledig gedecodeerd.

Nieuwe generatiebestanden gebruiken header minor 6 met ongewijzigde footer v2, indexpagina's en descriptor. De reader accepteert minor 4 (v0.3.8) en minor 5 (v0.3.9). Omdat het wetenschappelijke model in de immutable preamble staat, weigert v0.4.0 append op bestanden met schema-v2 metadata. Calendar arithmetic, fysieke eenhedenconversie, CRS-transformaties, sparse arrays, manifestupdates, adapters, Python en GPU zijn niet geïmplementeerd. Zie de v0.4.0-specificatie en ADR 0016. v0.4.0-fix1 is door de gebruiker volledig in WSL geverifieerd met 97 workspace-tests, fmt en Clippy; zie `docs/CHANGELOG.md`.

## Addendum — v0.4.1 begrensde generatie-codecpijplijn en wetenschappelijke CLI (2026-10-30)

De v0.4.1-kandidaat voegt additive reader-based generation writer API's toe. `ChunkReaderInput` levert een chunk-ID, een `Read`-bron, de gedeclareerde raw-lengte en een bestaande lossless codec. `ChunkWriteOptions` begrenst de hergebruikte kopieerbuffer op 1 byte tot 8 MiB (standaard 256 KiB). Raw en Zstandard schrijven met deze buffer; LZ4 behoudt het bestaande block-formaat en materialiseert maximaal één raw chunk van 64 MiB. Er is geen LZ4 wire-formatwijziging en geen claim van sub-chunk streaming.

Tijdens schrijven worden CRC32C en BLAKE3 over exact de opgeslagen bytes berekend. De bestaande generation digest, footer v2, indexcatalogus, B+tree pagina's en 72-byte descriptor blijven ongewijzigd. Wanneer schema-v2 metadata aanwezig is, valideert de writer de wetenschappelijke chunkreferenties en verwachte raw-lengtes voor de eerste write; Bool8-bytes worden tijdens streaming beperkt tot 0 of 1. De atomic file API schrijft naar een sibling temporary file, sync't vóór en na de footercommit en publiceert met rename; bij fouten wordt de tijdelijke file verwijderd. De bestaande single-writer-beperking blijft gelden.

De CLI krijgt `create-scientific-demo`, `validate-scientific` en `read-array-slice`. De demo produceert een synthetisch 4×4 F32-temperatuurveld met vier 2×2 chunks, latitude-/longitude-coördinaten en EPSG:4326-gridmapping. `validate-scientific` controleert de wetenschappelijke map en decodeert alle chunks. `read-array-slice` schrijft een row-major selectie als onbewerkte payloadbytes naar een nieuw bestand; datatype en payload-byte-order worden gemeld maar niet geconverteerd. De uitvoerlimiet blijft 64 MiB en een geselecteerde gecomprimeerde chunk wordt volledig gedecodeerd.

Dit is een eerste single-worker bounded ingestiepad. Parallelle codec-workers/queues/backpressure, LZ4 frame/sub-chunk streaming, streaming selectie-output, benchmarks, manifest-import, GRIB/ecCodes, NetCDF/Zarr, Python en GPU zijn niet onderdeel van v0.4.1. De eerste WSL-build van de oorspronkelijke v0.4.1-kandidaat rapporteerde een ontbrekende `std::io::Write`-traitimport en ongebruikte lokale `Read`/`Seek`-imports in de reader-based writer. v0.4.1-fix1 corrigeert uitsluitend die imports; de gebruiker heeft fix1 daarna in WSL bevestigd met 105 workspace-tests, fmt, Clippy, CLI-help en handmatige regressies. Zie `docs/spec/WX1_Bounded_Codec_Pipeline_and_Scientific_CLI_Draft_v0.4.1.md` en ADR 0017.


## Addendum — v0.4.2 experimentele wetenschappelijke leesplanning en hersteltests (2026-10-30)

v0.4.2 introduceert `ArrayReadPlan` en `ScientificSnapshot::plan_array_slice` als experimentele, additieve Rust-API. De planner valideert variabele-rang, selectiegrenzen en de bestaande 64 MiB uitvoerlimiet en rapporteert de datatype-/payload-byte-order-/shape-informatie plus de chunk-ID's die de selectie raken. Hij leest geen chunkbytes en geeft geen fysieke offsets vrij. `read_array_slice` gebruikt dezelfde planner en decodeert alleen die geselecteerde chunk-ID's; de bestaande row-major output en datatype-/byte-ordersemantiek blijven gelijk.

De kandidaat voegt regressies toe voor een 64×64 U16-array met 256 4×4 chunks, opgeslagen-bytecorruptie en expliciet herstel van een torn tail. Dit is geen performance- of durabilitybewijs. De planner loopt momenteel de chunkreferenties van de variabele lineair af; benchmarks en een geïndexeerde planner voor zeer grote chunk maps blijven noodzakelijk. Tijd-, fysieke verticale en CRS-gebaseerde ruimtelijke selecties worden uitgesteld totdat kalender-, eenheden-, asvolgorde-, longitude-wrapping- en CRS-semantiek expliciet zijn vastgelegd. Zie de v0.4.2-specificatiedraft en ADR 0018.


## v0.4.3 performance measurement addendum

v0.4.3 adds a dependency-free Cargo benchmark target around the existing writer, generation validator, array planner and array reader. The codec comparison uses three separate processes for Raw, Zstandard and LZ4 on a deterministic synthetic 2048×2048 F32 array with 64 chunks. A separate 4096-chunk workload measures the current linear `plan_array_slice` path. The harness reports median elapsed times, throughput, full-file size ratio and Linux process high-water RSS where `/proc/self/status` is available.

These measurements are observational: write timing excludes `fsync`, complete-file ratio includes metadata/index/footer overhead, the field is synthetic, and process high-water RSS is not per-function allocation accounting. No planner acceleration or performance claim is introduced before the user supplies WSL measurements. The v0.4.x wire format and all existing reader/writer semantics remain unchanged. See `docs/spec/WX1_Performance_Benchmarking_Draft_v0.4.3.md` and ADR 0019.


## Current NWP and benchmark roadmap (2026-11-01)

The current target architecture and phase gates are maintained in `WX1_ROADMAP_CURRENT.md` and `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md`. The current verified implementation status is in `WX1_CURRENT_STATUS.md`. These roadmap documents describe future requirements and must not be read as claims that the features already exist. In particular, PyO3/maturin, NumPy/xarray/Dask, external format adapters, appendable scientific schema generations, lazy open, lossy codecs, multi-writer/async I/O, non-regular grid topology, C ABI and GPU integration remain unimplemented unless a later status document records verified code and tests.

The current v0.4.x generation descriptor is 72 bytes; the older experimental raw-directory descriptor is 40 bytes. Both are distinct existing layouts. Any change requires a spec-draft, compatibility analysis, golden fixtures, tests and ADR before implementation.
