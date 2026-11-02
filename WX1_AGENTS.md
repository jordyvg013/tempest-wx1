# AGENTS.md — WX-1 ontwikkelregels

Dit bestand bevat de verplichte werkinstructies voor iedereen die aan WX-1 werkt, inclusief AI-codeassistenten en geautomatiseerde agents.

## 1. Algemene regels

## 1.1 Verplichte actuele documenten

Lees voor een nieuwe ontwikkeltaak naast dit bestand en `AGENTS.md` ook `WX1_CURRENT_STATUS.md`, `WX1_ROADMAP_CURRENT.md`, `WX1_PLAN.md`, `WX1_USER_CLI_TEST_INSTRUCTIONS.md` en `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md`. Als een van deze documenten ontbreekt, meld dit vóór je code wijzigt en herstel de documentatie. De actuele status is leidend boven een verouderde roadmappassage; documenteer een conflict in plaats van stilzwijgend een versie te kiezen.


1. Lees dit bestand volledig voordat je code wijzigt.
2. Bekijk daarna de bestaande projectstructuur, `Cargo.toml`-bestanden, broncode, tests, documentatie, ADR's en changelog.
3. Behoud de bestaande architectuur tenzij er een concrete technische reden is om die te wijzigen.
4. Voer geen grote, ongerelateerde refactors uit tijdens een release-implementatie.
5. Verzin geen bestaande functies, testresultaten, bestanden of API's.
6. Claim nooit dat code is gebouwd of getest als dat niet daadwerkelijk is uitgevoerd.
7. Maak onduidelijkheden en ontbrekende requirements expliciet; verberg ze niet met aannames.
8. Houd de documentatie in overeenstemming met het werkelijk geïmplementeerde gedrag.

## 2. Projectdoel en technische uitgangspunten

WX-1 is een open, platformonafhankelijk wetenschappelijk opslagformaat en opslag-engine. De eerste toepassing is meteorologische en klimatologische data; het ontwerp moet breder inzetbaar blijven voor multidimensionale wetenschappelijke datasets.

Vaste uitgangspunten:

- Rust is de primaire implementatietaal voor de opslagkern.
- Python is een officiële gebruikersinterface via PyO3/maturin.
- Little-endian is de standaard; big-endian moet correct worden ondersteund.
- Metadata gebruikt deterministische CBOR waar de specificatie dat voorschrijft.
- Zstandard is de standaard lossless codec; Raw en LZ4 worden ondersteund waar geïmplementeerd.
- CRC32C en BLAKE3 hebben expliciet gedefinieerde rollen.
- RAM-, buffer-, metadata-, chunk- en decompressielimieten zijn verplicht.
- Bestanden zijn onbetrouwbare input: valideer alle velden en grenzen.
- CPU-functionaliteit mag nooit afhankelijk zijn van een GPU.
- GPU-ondersteuning is optioneel en capability-based.
- Wetenschappelijke metadata en provenance mogen niet stilzwijgend verloren gaan.

## 3. Verplichte werkwijze per taak

Voer bij iedere taak de volgende stappen uit:

1. **Inspectie**
   - Lees relevante broncode en tests.
   - Controleer de actuele API en bestaande conventies.
   - Zoek naar gerelateerde ADR's en specificatietekst.

2. **Plan**
   - Benoem welke bestanden waarschijnlijk wijzigen.
   - Beschrijf de kleinste veilige implementatiestap.
   - Benoem risico's voor bestandscompatibiliteit en bestaande functionaliteit.

3. **Implementatie**
   - Maak kleine, gerichte wijzigingen.
   - Voeg tests toe bij nieuwe functionaliteit.
   - Behoud compatibiliteit met bestaande fixtures, tenzij een expliciete formatwijziging gepland is.
   - Voeg geen ongebruikte dependencies toe.

4. **Verificatie**
   - Voer relevante tests uit.
   - Voer formatting en Clippy uit.
   - Controleer CLI-gedrag wanneer de wijziging de CLI raakt.
   - Controleer foutpaden en corrupte input, niet alleen het happy path.

5. **Documentatie**
   - Werk de formele of experimentele specificatie bij.
   - Voeg een ADR toe voor belangrijke architectuurkeuzes.
   - Werk changelog/release notes bij.
   - Vermeld expliciet wat nog niet is geïmplementeerd.

6. **Oplevering**
   - Volg `WX1_USER_CLI_TEST_INSTRUCTIONS.md` voor de volgorde en vorm van het volledige testbericht.
   - Rapporteer gewijzigde bestanden.
   - Rapporteer exacte uitgevoerde commando's en resultaten.
   - Benoem niet-uitgevoerde tests.
   - Benoem resterende beperkingen en risico's.

## 4. Veiligheid van bestand-parsers

Behandel ieder WX-1-bestand als potentieel kwaadaardige of beschadigde input.

- Vertrouw nooit op offsets, lengtes, aantallen, dimensies of allocatiegroottes uit het bestand.
- Controleer integer-overflow vóór optellen, vermenigvuldigen, casten en alloceren.
- Gebruik checked arithmetic (`checked_add`, `checked_mul`, passende `try_from`-conversies).
- Valideer dat alle bereiken binnen de werkelijke bestandsgrootte vallen.
- Valideer dat verboden overlap niet voorkomt.
- Valideer unieke sleutels en chunk-ID's.
- Stel harde limieten in vóór allocatie of decompressie.
- Wijs onbekende verplichte flags/features af.
- Behoud onbekende optionele metadata als de specificatie dat toestaat.
- Gebruik geen `unwrap()` of `expect()` op waarden die door bestandsinput kunnen worden beïnvloed.
- Geef specifieke fouttypen terug; gebruik geen generieke fout die een CRC-, hash-, footer- of directoryfout maskeert.
- Laat parserfouten niet leiden tot panics, out-of-bounds toegang of onbegrensde geheugengroei.

## 5. Integriteit en commits

- Definieer exact welke bytes CRC32C en BLAKE3 beschermen.
- Verander de betekenis van een bestaande checksum of hash niet stilzwijgend.
- Controleer integriteit vóór data als vertrouwd aan de gebruiker door te geven.
- Houd chunkintegriteit en bestand-/commitintegriteit als afzonderlijke controles.
- Een commit is pas geldig als alle bijbehorende onderdelen volledig en correct gevalideerd zijn.
- Een onvolledige nieuwe generatie mag een eerdere geldige generatie niet vernietigen.
- Schrijf bij crash-safe publicatie eerst naar een tijdelijk bestand en publiceer pas na succesvolle flush/sync en validatie.
- Documenteer platformafhankelijke beperkingen van `fsync` en atomische rename.
- Herstel nooit stilzwijgend door data te verwijderen of te herschrijven.
- Maak herstelgedrag reproduceerbaar en testbaar.

## 6. Streaming en geheugenbeheer

- Laad nooit standaard een volledige dataset in RAM.
- Gebruik streaming of range reads waar de codec en indeling dat toelaten.
- Stel expliciete maximale buffer- en chunkgroottes in.
- Gebruik begrensde queues voor parallelle verwerking.
- Pas backpressure toe zodat producers niet onbeperkt data kunnen opstapelen.
- Houd rekening met tijdelijke buffers van compressie- en decompressielibraries.
- Documenteer wanneer een codec of bewerking toch een volledige chunk materialiseert.
- Claim geen volledig streaminggedrag zolang een pad een volledige chunk in geheugen decodeert.
- Test ook limietoverschrijdingen en extreem kleine/grote invoer.

## 7. Byte-order en wire-format

- Gebruik expliciete byte-orderfuncties; vertrouw niet op native host-endianness.
- Test little-endian én big-endian.
- Leg veldbreedtes, offsets, alignment, reserved bytes en versiegedrag vast.
- Gebruik geen impliciete struct-layout als wire-format.
- Voeg geen velden toe of wijzig bestaande veldbetekenissen zonder specificatie- en compatibiliteitsanalyse.
- Reserved bytes moeten volgens de specificatie worden gevalideerd.
- Onbekende verplichte features worden geweigerd; optionele extensies worden alleen genegeerd of behouden als de specificatie dat toestaat.

## 8. Indexen

- De primaire directory blijft de gezaghebbende bron voor chunklocaties en chunkmetadata.
- Indexen zijn hulpmiddelen om gegevens te vinden; ze mogen nooit onjuiste resultaten opleveren.
- Valideer indexverwijzingen tegen de primaire directory.
- Definieer versie, checksums, grenzen en herstelgedrag voor iedere persistente index.
- Een beschadigde index mag niet leiden tot lezen van willekeurige bestandsoffsets.
- Gebruik een B+tree als generieke basis waar passend.
- Kies een gespecialiseerde ruimtelijke index alleen met onderbouwde eisen en tests.
- Leg tijdkalender, CRS, asvolgorde, longitude-wrapping en verticale niveau-semantiek expliciet vast.

## 9. Wetenschappelijke metadata en parameterregisters

- Bewaar oorspronkelijke parametercodes en hun volledige context.
- Een numerieke code alleen is niet altijd uniek; behoud discipline, tabel, versie, provider en productcontext.
- Maak onderscheid tussen onbekende, ontbrekende en niet-ondersteunde parameters.
- Verzin geen betekenis voor onbekende codes.
- Overschrijf officiële tabellen nooit stilzwijgend.
- Overrides moeten expliciet, versieerbaar en voorzien van provenance zijn.
- Eenheden mogen alleen worden geconverteerd als dimensiecompatibiliteit aantoonbaar is.
- Bewaar bron, auteur, licentie, reden en tabelversie waar beschikbaar.
- Houd meteorologische berekeningen zoals MetPy in de Python/research-laag; maak ze geen verplichte afhankelijkheid van de opslagkern.

## 10. Dependencies en externe code

- Voeg alleen dependencies toe die noodzakelijk zijn.
- Controleer licentie, onderhoudsstatus, platformondersteuning en security-impact.
- Pin of beperk versies volgens de bestaande projectconventies.
- Vermijd dubbele implementaties van bestaande, betrouwbare functionaliteit.
- Gebruik geen netwerktoegang tijdens tests tenzij een test expliciet als integratietest is gemarkeerd.
- Houd de core zo onafhankelijk mogelijk van Python, GPU-runtimes en provider-specifieke bibliotheken.

## 11. Tests

Minimaal uit te voeren bij een normale Rust-release:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p wx1-cli -- --help
```

Voeg gerichte tests toe voor iedere wijziging.

Verplichte testonderwerpen naarmate de functionaliteit beschikbaar komt:

- roundtrip van metadata en payload;
- little-endian en big-endian;
- truncated en malformed bestanden;
- ongeldige offsets, lengtes en integer-overflow;
- CRC32C- en BLAKE3-corruptie;
- foutieve codecs en decompressielengtes;
- directory- en indexcorruptie;
- onderbroken writes en herstel naar vorige commit;
- dubbele ID's en overlappende bereiken;
- geheugenlimieten en bufferlimieten;
- compatibiliteit met oudere fixtures;
- Python-/NumPy-/xarray-interoperabiliteit;
- CPU/GPU-resultaatvergelijking waar GPU-code bestaat.

Gebruik deterministische fixtures waar mogelijk. Tests mogen niet afhankelijk zijn van actuele externe datasets of netwerkdiensten.

## 12. Fuzzing en robustness

Zodra de parserstructuur dit toelaat:

- Voeg fuzz targets toe voor header, metadata, directory, indexen, commitrecords en codec-ingangen.
- Gebruik corpusbestanden voor geldige en corrupte voorbeelden.
- Controleer dat willekeurige input geen panic, hang of onbegrensde allocatie veroorzaakt.
- Voeg gevonden regressies als vaste tests toe.
- Houd fuzzing los van de normale snelle unit-testset, maar documenteer hoe deze wordt uitgevoerd.

## 13. CLI-regels

- CLI-output moet voorspelbaar en scriptbaar zijn.
- Gebruik duidelijke foutmeldingen en een niet-nul exitcode bij fouten.
- Schrijf geen succesmelding voordat de operatie daadwerkelijk is afgerond.
- Maak onderscheid tussen inspectie, validatie en reparatie.
- Een inspectiecommando mag een bestand niet wijzigen.
- Reparatie moet expliciet door de gebruiker worden aangevraagd.
- Documenteer alle commando's en opties in `--help` en gebruikersdocumentatie.

## 14. Python- en GPU-regels

### Python

- Rust blijft de bron van waarheid voor opslagsemantiek.
- Python bindings mogen geen afwijkende interpretatie van het bestandsformaat introduceren.
- Houd grote arrays lazy/chunked waar mogelijk.
- Vermijd onnodige kopieën tussen Rust en NumPy.
- Test lifetime-, ownership- en foutafhandelingsgedrag.

### GPU

- GPU is optioneel; CPU blijft volledig bruikbaar.
- Detecteer hardware- en backendmogelijkheden vóór uitvoering.
- Maak geheugenlimieten en batchgroottes configureerbaar.
- Ondersteun CPU-only, GPU-only waar haalbaar en hybride verwerking.
- Geef expliciet aan wanneer een bewerking niet op de GPU ondersteund wordt.
- Verberg geen fallback.
- Ontwerp met beperkte VRAM, met 6 GB als praktisch testdoel.
- Claim geen GPU-versnelling zonder correcte benchmarks en numerieke validatie.

## 15. Documentatie en ADR's

- Architectuurkeuzes met langdurige gevolgen krijgen een ADR.
- ADR's bevatten context, beslissing, alternatieven en gevolgen.
- Specificaties beschrijven bytes en validatieregels exact; ze zijn geen globale architectuurbeschrijvingen.
- Markeer experimentele onderdelen expliciet.
- Houd voorbeelden synchroon met de CLI en API.
- Noteer breaking changes en migratiegevolgen.
- Verwijder geen eerdere ontwerpbeslissingen zonder de reden en vervangende beslissing vast te leggen.

## 16. Release- en versiebeheer

- Werk aan één afgebakende release-scope tegelijk.
- Houd de release-scope klein genoeg om volledig te testen.
- Gebruik duidelijke versienummers en mapnamen.
- ZIP-bestanden bevatten één projectroot met de juiste versienaam.
- Sluit `target/`, tijdelijke bestanden, lokale caches, secrets en persoonlijke data uit.
- Controleer de inhoud van het ZIP-archief vóór oplevering.
- Voeg release notes, testresultaten en bekende beperkingen toe.
- Maak geen stabiele API- of formaatcompatibiliteitsclaims vóórdat die formeel zijn vastgesteld.

## 17. Verboden werkwijzen

Niet doen:

- Tests overslaan en toch melden dat ze geslaagd zijn.
- Code genereren op basis van aannames over niet-geïnspecteerde bestanden.
- Fouten verbergen met brede `unwrap`, `expect`, `catch_unwind` of generieke foutmapping.
- Bestandsgestuurde groottes zonder limiet alloceren.
- Onbekende parametercodes een betekenis toekennen.
- Officiële parameterdefinities stilzwijgend overschrijven.
- Een volledige dataset laden terwijl een chunked oplossing vereist is.
- Een GPU-runtime verplicht maken voor normale CPU-functionaliteit.
- Bestaande functionaliteit verwijderen zonder migratiepad en tests.
- Documentatie laten claimen dat iets bestaat terwijl alleen een ontwerp, stub of TODO aanwezig is.

## 18. Opleverrapport

Sluit iedere implementatietaak af met:

1. **Resultaat:** wat daadwerkelijk is geïmplementeerd.
2. **Bestanden:** welke bestanden zijn toegevoegd of gewijzigd.
3. **Tests:** exacte commando's en echte resultaten.
4. **Niet getest:** wat niet kon worden uitgevoerd en waarom.
5. **Beperkingen:** wat nog experimenteel, gedeeltelijk of niet geïmplementeerd is.
6. **Compatibiliteit:** impact op bestaande bestanden en API's.
7. **Release:** exacte versie en locatie van eventuele artifacts.

Wees feitelijk en terughoudend. Een groene unit-testset bewijst niet automatisch crash safety, interoperabiliteit, performance of productiegeschiktheid.

[PONYTAIL RULESET]
Je bent een senior developer die code-bloat haat. Lever altijd de kleinst mogelijke, correcte code-wijziging. Voordat je code schrijft of aanpast, doorloop je verplicht deze 7 stappen:
1. YAGNI: Is dit écht nodig voor de specifieke prompt?
2. Hergebruik: Kan bestaande logica in het project worden hergebruikt?
3. Standard Library: Kan de ingebouwde bibliotheek van de taal dit oplossen?
4. Native Platform: Biedt het OS of platform dit al aan?
5. Installed Dependency: Bestaat er al een geïnstalleerd pakket dat dit doet?
6. One Expression: Kan het in één expressie/regel?
7. Minimum Implementation: Schrijf uitsluitend de absolute basis.

[CAVEMAN RULESET]
Schrijf code alsof abstractie een gevaar is. Geen slimme trucjes, geen diepe overerving, geen complexe design-patterns. Volg verplicht deze regels:
1. No Clever Code: Geen ingewikkelde one-liners of vage bitwise operaties. Schrijf simpele, uitgeschreven if/else-statements die direct leesbaar zijn.
2. Inline over Abstractie: Kopieer liever drie regels code als dat een extra, onnodige functie of module-import voorkomt.
3. Beaten Path: Gebruik alleen basisfuncties en patronen die al 20 jaar onveranderd werken in de programmeertaal.
4. No Fancy Dependencies: Introduceer geen nieuwe bibliotheken voor problemen die je zelf met een simpele loop of array kunt oplossen.
5. Explicit Over Implicit: Maak variabelen, data-structuren en stappen zo expliciet mogelijk. Magie (zoals reflection of complexe macro's) is verboden.

