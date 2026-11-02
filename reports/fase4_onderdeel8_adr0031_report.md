# WX-1 Fase 4, Onderdeel 8: ADR 0031 — Fuzzing, Golden Test Vectors, Power-Loss Resistentie, Beveiligingsanalyse en Cross-Platform Validatie voor 1.0

**Datum**: 2026-11-02 17:15:00 CET  
**Status**: Geaccepteerd & Geïmplementeerd (ADR 0031)  
**Filesystem**: WSL2 Native ext4 & Virtual Mount  
**Compiler / Toolchain**: Rust 1.99.1 | LLVM 23.1.1 | Python 3.12 (Conda ECMWF) | PyO3 0.24.2  
**Codebase Integriteit**: `#![forbid(unsafe_code)]` actief op workspace-niveau (100% veilige Rust)  

---

## 1. Doel & Scope van ADR 0031
Als voorbereiding op de formele v1.0-bevriezing van WX-1 moet de container- en parsingengine aantoonbaar robuust zijn tegen vijandige invoer, stroomuitval / SIGKILL tijdens schrijven, corrupte data, decompressiebommen en platformverschillen. 

In deze fase zijn vijf kerncomponenten gerealiseerd en geverifieerd:
1. **Fuzzing & Robuustheid**: Automatische fuzz-harnesses voor container-headers, footers, chunk directories, bit-flip mutaties en byte-truncatie op alle offsets.
2. **Onafhankelijke Golden Conformance Vectors**: 11 deterministische binaire testvectoren voor zowel geldige containerprofielen als opzettelijke corruptie- en aanvalspatronen.
3. **Power-Loss Resistentie & Crash Consistency**: Twee-fasen commit-garanties, simulatie van stroomuitval bij incomplete writes, en atomaire herstelverificatie via `repair_generation_file`.
4. **Beveiligingsanalyse & Grensbewaking**: Bescherming tegen Denial-of-Service (memory exhaustion, decompressiebommen), directory traversal injection (`../`), en formele unsafe-code audit.
5. **Cross-Platform & Endianness**: Bit-exact behoud van speciale IEEE-754 float32-waarden (NaN, $\pm\infty$, $-0.0$, subnormals) en expliciete byte-order onafhankelijkheid.

---

## 2. Fuzzing & Robuustheidsanalyse

De geïmplementeerde fuzz-tests bevinden zich in [`crates/wx1-format/tests/robustness_fuzz_security.rs`](file:///mnt/d/AI/weer-ai/Tempest/WX-1/crates/wx1-format/tests/robustness_fuzz_security.rs):

| Fuzzing Doel | Aantal Iteraties | Testmethode | Resultaat | Panics / Crashes |
| :--- | :---: | :--- | :---: | :---: |
| **`ContainerHeader::decode`** | 10.000 | Volledig pseudo-random 64-byte invoerbuffers (LCG deterministisch) | 10.000 afgewezen / parsed | **0 panics (100% safe)** |
| **Commit Footers & Directories** | 5.000 | Willekeurige 48-byte footers en corrupte directory-indices | 5.000 afgewezen via `ChunkIoError` | **0 panics (100% safe)** |
| **Bit-Flip Mutaties op Container** | 1.000+ | Systematische bit-flips (bits 0x01, 0x08, 0x80) over alle byteposities | CRC32C / BLAKE3 vangen alle corrupties af | **0 panics (100% safe)** |
| **Truncatie Fuzzing** | Elke byte $0 \dots N-1$ | Truncatie van geldige container op elke opeenvolgende byte | 100% afgewezen met `UnexpectedEof` / `ChunkIoError` | **0 panics (100% safe)** |

*Conclusie:* De parser crasht nooit op corrupte of gemuteerde binaire data. Alle fouten worden netjes gerapporteerd via `Result<T, Wx1Error>`.

---

## 3. Onafhankelijke Golden Conformance Vectors

In [`tests/fixtures/golden_vectors/`](file:///mnt/d/AI/weer-ai/Tempest/WX-1/tests/fixtures/golden_vectors/) zijn 11 onafhankelijke binaire testvectoren gegenereerd en getest via zowel Rust ([`crates/wx1-format/tests/golden_vectors.rs`](file:///mnt/d/AI/weer-ai/Tempest/WX-1/crates/wx1-format/tests/golden_vectors.rs)) als Python ([`python/tests/test_golden_vectors.py`](file:///mnt/d/AI/weer-ai/Tempest/WX-1/python/tests/test_golden_vectors.py)):

| Vector ID | Bestandsnaam | Doel & Inhoud | Grootte | Verwachte Respons | Rust Status | Python Status |
| :--- | :--- | :--- | :---: | :--- | :---: | :---: |
| **`GV-01`** | `gv01_valid_scalar_2d_zstd.wx1` | 2D oppervlaktetemperatuur ($16 \times 16$) met Zstandard en CF metadata | 9.0 KB | Geldige lezing, $E_{\max} = 0.0$ | ✅ Slaagt | ✅ Slaagt |
| **`GV-02`** | `gv02_valid_volume_3d_lz4.wx1` | 3D geopotentiaal volume ($4 \times 8 \times 8$) met LZ4 compressie | 9.3 KB | Geldige lezing, 1024 bytes unpacked | ✅ Slaagt | ✅ Slaagt |
| **`GV-03`** | `gv03_valid_multigen_append.wx1` | 3 generaties append met groeiende tijd-as ($[3, 2]$) | 33 KB | Generatie 3 leest over alle generaties | ✅ Slaagt | ✅ Slaagt |
| **`GV-04`** | `gv04_valid_special_floats.wx1` | IEEE-754: NaN, $+\infty$, $-\infty$, $-0.0$, $+0.0$, $1.0$ | 8.9 KB | Bit-exact behoud van alle float-patronen | ✅ Slaagt | ✅ Slaagt |
| **`GV-05`** | `gv05_corrupt_magic.wx1` | Header magic gewijzigd van `WX1\x00` naar `BAD\x00` | 9.0 KB | Schone afwijzing `InvalidMagic` | ✅ Slaagt | ✅ Slaagt |
| **`GV-06`** | `gv06_corrupt_footer_crc.wx1` | Gemuteerde CRC32C in de commit footer | 9.0 KB | Schone afwijzing `CommitFooterInvalid` | ✅ Slaagt | ✅ Slaagt |
| **`GV-07`** | `gv07_corrupt_chunk_blake3.wx1` | 1 bit geflipt in payload van chunk 1 | 9.0 KB | Afwijzing via `ChecksumMismatch(1)` | ✅ Slaagt | ✅ Slaagt |
| **`GV-08`** | `gv08_corrupt_truncated_payload.wx1`| Container afgekapt met 50 bytes halverwege payload | 8.9 KB | Schone afwijzing `UnexpectedEof` | ✅ Slaagt | ✅ Slaagt |
| **`GV-09`** | `gv09_corrupt_unsupported_codec.wx1`| Codec-ID in descriptor gezet op onbekende `0x99` | 9.0 KB | Schone afwijzing `UnsupportedCodec(0x99)`| ✅ Slaagt | ✅ Slaagt |
| **`GV-10`** | `gv10_malicious_path_traversal.wx1`| Variabelenaam met directory traversal (`../`)| 9.0 KB | Afgewezen bij modelvalidatie | ✅ Slaagt | ✅ Slaagt |
| **`GV-11`** | `gv11_malicious_decompression_bomb.wx1` | Chunk descriptor declareert 100 MiB (> 64 MiB limiet) | 9.0 KB | Afgewezen met `ChunkTooLarge` | ✅ Slaagt | ✅ Slaagt |

Alle 11 vectoren zijn fysiek aanwezig op schijf in `tests/fixtures/golden_vectors/` en gedocumenteerd in `provenance.json` met bijbehorende SHA-256 hashes.

---

## 4. Power-Loss Resistentie & Crash Consistency

Het commit-model is strikt ontworpen als een **Atomic Three-Phase Generation Commit**:
1. **Fase 1**: Payload-chunks worden sequentieel naar opslag geschreven.
2. **Fase 2**: Index-pagina's (B-Tree v2) en metadata-manifest worden geschreven.
3. **Fase 3**: De generatie commit footer (met generatienummer, BLAKE3 manifest digest en CRC32C) wordt duurzaam weggeschreven en gesynchroniseerd via `fsync` (`durable_commit`).

In de testsuite zijn plotselinge stroomuitval / crashes gesimuleerd en geverifieerd via zowel in-memory als actieve subprocessen:

1. **Crash Scenario A (Mid-Chunk Write)**:
   - Een proces schrijft Generation 1 succesvol (grootte: $S_1$).
   - Tijdens het schrijven van Generation 2 valt de stroom uit nadat 150 bytes willekeurige chunkpayload naar schijf zijn geschreven, zónder commit footer.
   - **Gedrag van Readers**: `open_generation_snapshot` scant backwards, detecteert dat Generation 1 een geldige commit footer heeft, en maakt Generation 1 volledig leesbaar voor clients.
   - **Gedrag van Writers**: `append_generation` detecteert dat de bestandsgrootte groter is dan de geldige staart van Generation 1 en weigert verdere appends met `Err(ChunkIoError::UnrepairedTail)`.
   - **Herstel**: Aanroep van `repair_generation_file(path)` detecteert de afgebroken staart, kapt het bestand exact af op $S_1$ via `file.set_len(retained_len)` en `file.sync_all()`.
2. **Crash Scenario B (Torn Commit Footer)**:
   - Slechts 32 van de 48/64 footerbytes zijn naar schijf geschreven voordat de stroom uitviel.
   - `repair_generation_file` herstelt direct en deterministisch terug naar Generation 1.
3. **Commit na Herstel (Scenario C)**:
   - Direct na `repair_generation_file` slaagt het schrijven van een nieuwe Generation 2 vlekkeloos en zijn zowel Generation 1 als de nieuwe Generation 2 intact beschikbaar.
4. **Crash Scenario D (`SIGKILL` op Actief Schrijfproces)**:
   - In test `test_power_loss_subprocess_sigkill_recovery` spawnt de test een actief OS-subproces dat data appendt naar de container en abrupt wordt geëxecuteerd met een OS `SIGKILL` (signal 9).
   - De container blijft 100% consistent: Generation 1 blijft onbeschadigd leesbaar, `repair_generation_file` detecteert en kapt de ongecommitteerde bytes exact af, en de container is direct gereed voor nieuwe generaties.

---

## 5. Beveiligingsanalyse (Security Hardening)

### 5.1 Memory Safety & Geheugenallocatielimieten
- **Geen Onveilige Rust**: In `Cargo.toml` is `unsafe_code = "forbid"` geconfigureerd. Clippy en rustc garanderen dat er 0 regels `unsafe` code aanwezig zijn in de opslagkern en CLI.
- **Decompressiebommen & Geheugenbescherming**:
  - `MAX_CHUNK_BYTES` is begrensd op **64 MiB**.
  - `decode_chunk` weigert elke chunk die een `raw_len > MAX_CHUNK_BYTES` declareert vóórdat er geheugen wordt gereserveerd.
  - Zstandard en LZ4 decompressie gebruiken een `take(limit)` stream-wrapper met een pre-allocatie cap (`expected.min(16 MiB)`), waardoor chunks nooit oneindig kunnen expanderen.
  - Test `test_security_decompression_bomb_and_length_guards` en Golden Vector `GV-11` bevestigen dat kwaadaardige chunks met onbegrensde decompressieverhoudingen direct worden geneutraliseerd.
- **Formele Grenswaarden (Boundary Tests)**:
  - **Maximaal Aantal Dimensies / Array Rank**: Begrensd op **64** (`MAX_SCIENTIFIC_RANK`). Test `test_security_max_dimensions_and_rank_boundary` toont aan dat rank 64 correct wordt geaccepteerd, terwijl rank 65 pertinent wordt geweigerd met `ScientificModelError::RankMismatch`.
  - **Maximaal Aantal Named Dimensions**: Begrensd op **4.096** (`MAX_SCIENTIFIC_DIMENSIONS`).
  - **Maximaal Aantal Chunk Referenties**: Begrensd op **1.048.576** (`MAX_CHUNKS`). Test `test_security_max_chunk_refs_boundary` verifieert dat zowel individuele variabelen als cumulatieve variabelensets die deze grens overschrijden onmiddellijk worden geweigerd met `ChunkCoverage` of `TooManyChunkReferences`.
  - **Non-recursieve CBOR Schema-structuur**: Het CBOR-metadataschema is strikt plat en tabulair. De structurele diepte is inherent begrensd tot maximaal 3 niveaus (Model $\to$ Variable $\to$ ChunkRef). Stack overflow aanvallen via diep geneste CBOR-structuren zijn hierdoor by-design uitgesloten.
  - **Metadata Omvang**: Maximaal 32 MiB (`MAX_METADATA_BYTES`). Maximaal 100.000 variabelen en 4.096 groepen.
  - Maximaal aantal variabelen: 100.000.
  - Maximaal aantal groepen: 4.096.
  - Metadata CBOR-omvang: maximaal 32 MiB (`MAX_METADATA_BYTES`).

### 5.2 Directory Traversal & Injectiepreventie
Variabelen- en groepspaden worden gecontroleerd via `valid_identifier` en `valid_group_path`:
- Paden zoals `../../etc/passwd`, `foo/../bar`, `\windows\system32`, `var\0null` en spaties worden pertinent geweigerd met `ScientificModelError::InvalidVariable` of `InvalidGroup`.
- Alleen canonieke Unix-hiërarchieën (`/groep/subgroep/variabele`) met alfanumerieke tekens, liggende streepjes (`_`), koppeltekens (`-`) en punten (`.`) zijn toegestaan.

---

## 6. Cross-Platform & Endianness Validatie

- **IEEE-754 Speciale Drijvende-Kommagegevens**:
  - Getest met `f32::NAN`, `f32::INFINITY`, `f32::NEG_INFINITY`, `-0.0`, `+0.0` en de kleinste positieve subnormale float (`0x00000001`).
  - Bitpatronen (`to_bits()`) blijven na compressie, opslag en reconstructie 100% bit-exact identiek.
- **Byte Order**:
  - `PayloadByteOrder::LittleEndian` en `PayloadByteOrder::BigEndian` worden expliciet opgeslagen in de variabele-metadata.
  - Conversies gebeuren transparant zonder platformafhankelijkheid.

---

## 7. Toelichting op de 3 Geskipte Python Tests

In `python/tests/test_external_adapters.py` worden 3 tests geskipt:
1. `test_zarr_adapter_reports_missing_optional_dependency`: Wordt geskipt omdat `zarr` (v3.4.2) daadwerkelijk geïnstalleerd is in de actieve Conda-omgeving. De adapter-test vereist in dat geval een live fixture in plaats van de missing-dependency error.
2. `test_grib2_adapter_reports_missing_optional_dependency`: Wordt geskipt omdat `cfgrib` daadwerkelijk geïnstalleerd is in de actieve Conda-omgeving.
3. `test_grib2_local_fixture_when_path_is_supplied`: Wordt geskipt omdat de optionele omgevingsvariabele `WX1_GRIB2_FIXTURE` niet is gezet. (De GRIB2 roundtrip functionaliteit is elders dekkend getest in `test_interoperability_adr0030.py`).

Dit is het verwachte en gedocumenteerde gedrag van de pytest test-suite.

---

## 8. Verificatie- & Testresultaten

De formele verificatietaken zijn uitgevoerd met de volgende uitkomsten:

1. **`cargo fmt --all -- --check`**:  
   Resultaat: **Geslaagd (0 diffs, 100% conform rustfmt)**.
2. **`cargo clippy --workspace --all-targets -- -D warnings`**:  
   Resultaat: **Geslaagd (0 warnings, 0 fouten over alle crates en native bindings)**.
3. **`cargo test --workspace`**:  
   Resultaat: **159 geslaagd, 0 mislukt, 0 genegeerd** (inclusief `robustness_fuzz_security.rs`, `golden_vectors.rs`, `scientific_arrays.rs`, `scientific_streaming.rs`, `wx1-cli`, `wx1-registry` en `wx1-python-native`).
4. **`pytest python/tests/`**:  
   Resultaat: **45 geslaagd, 3 geskipt, 0 mislukt** (inclusief `test_golden_vectors.py`, alle 11 vectoren gevalideerd).
5. **Cross-Platform Matrix & CI**:  
   Gedefinieerd in [`.github/workflows/ci.yml`](file:///mnt/d/AI/weer-ai/Tempest/WX-1/.github/workflows/ci.yml) over Linux x86_64, Linux aarch64, macOS ARM64 en Windows x86_64.

---

## 9. Status Conclusie voor Fase 4, Onderdeel 8
Fase 4, Onderdeel 8 (ADR 0031) is **volledig afgerond, geharmoniseerd en geverifieerd**. Alle 11 golden vectors bestaan fysiek op schijf met vastgelegde SHA-256 hashes, grenswaarden zijn bit-exact getest, proces-crashes met SIGKILL zijn aantoonbaar herstelbaar, en de v1.0-specificatie is consistent en audit-proof.
