# WX-1 — gebruikersinstructies voor CLI-testpakketten

Deze instructie is verplicht voor iedere releasehandoff met terminaltests. Alle commando's worden in één chatbericht geleverd. Wacht niet na elk blok op de uitvoer van de gebruiker.

## Verplichte volgorde en vorm

1. **Blok 1:** uitsluitend het `cd`-commando naar de exacte nieuwe releasemap.
2. **Blok 2:** combineer exact deze vier standaard Rust-controles:
   - `cargo fmt --all`
   - `cargo fmt --all -- --check`
   - `cargo test --workspace`
   - `cargo clippy --workspace --all-targets -- -D warnings`
3. **Blok 3:** actuele CLI-help als afzonderlijk blok. Gebruik het geverifieerde workspace-commando, bijvoorbeeld `cargo run --quiet -p wx1-cli -- --help`; controleer eerst de actuele workspace en CLI-help. Verzin geen commando of pakketnaam.
4. Daarna alle gerichte CLI-, integratie-, corruptie-, herstel-, compatibiliteits-, Python-, wheel- en benchcontroles in afzonderlijke, korte codeblokken.
5. Ieder logisch onderdeel krijgt een eigen codeblok. Houd scripts bij voorkeur op drie tot vijf controles per blok, met tijdelijke directories en cleanup waar passend.
6. Zet direct onder ieder blok de verwachte uitvoer en succescriteria. Verwachte resultaten zijn voorspellingen, geen gerapporteerde testresultaten.

## Correctheidsregels

- Inspecteer eerst de echte broncode, Cargo-/Python-manifests, tests en actuele CLI-help.
- Verzin nooit CLI-commando's, argumenten, parameters, paden, API's of verwachte numerieke resultaten.
- Rapporteer geen test, wheel-build of benchmark als uitgevoerd als dit niet werkelijk is uitgevoerd.
- Markeer niet uitgevoerde onderdelen als **niet getest** of **niet gemeten**.
- Vraag pas na het leveren van alle testblokken om de volledige terminaluitvoer.
- Analyseer de volledige aangeleverde uitvoer. Onderscheid echte fouten van handmatig afgebroken opdrachten (`^C`).
- Maak alleen een `-fixN`-release wanneer de uitvoer een concrete fout aantoont. Behoud de eerder geverifieerde release als basis.
- Een benchmark heeft zijn eigen rapportage: configuratie, machine, filesystem, toolchain, meetmethode, runs en beperkingen. Een succesvol benchmarkcommando is geen prestatieacceptatie.

## Rapportage per release

Rapporteer afzonderlijk:

- Rust-formatting, tests en Clippy;
- CLI-/handmatige regressietests;
- Python-unittests en interoperabiliteitstests;
- maturin/wheel-build en CPU-only installatie;
- benchmarkresultaten en meetbeperkingen;
- niet uitgevoerde tests en resterende beperkingen.

ZIP bevat één projectroot, zonder `target/`, `.git/`, caches of lokale datasets.


## Benchmark- en duurzaamheidstests

- Rapporteer de interne Cargo/Rust-microbenchmark (ADR 0019) los van de cross-format-suite (ADR 0021).
- Rapporteer buffered writes en durable writes als afzonderlijke meetmodi. De term durable mag alleen worden gebruikt wanneer de bedoelde file-/directory-sync daadwerkelijk is uitgevoerd en de garantie is getest.
- `kill -9` is een procescrashtest, geen fysieke power-loss-test.
- Meet de huidige lineaire planner eerst tegen de formele nulmeting; een geïndexeerde planner wordt pas daarna gebouwd en opnieuw vergeleken.
- Voeg W6 end-to-end checkpointmetingen toe; CPU-first, GPU alleen na de GPU/API-gate.
- Gebruik de bevroren drempels in `bench/config/acceptance.yaml`; pas ze niet aan na het bekijken van resultaten.

## Numerieke rapportagenotatie

Gebruik altijd een punt als decimaalteken en geen duizendtalscheiding in test- en benchmarkrapportages. Voorbeelden: `6228.0 ns`, `6.228 us`, `4096 chunks`. Noteer niet-gemeten waarden expliciet als `niet gemeten`; verzin geen nulwaarden.

## Fase-1 Python-controles (naast Rust-blokken)

Run vanuit de projectroot na de standaard Rust-blokken. Gebruik een Rust-enabled Python build environment. De onderstaande verwachte uitkomsten zijn geen reeds behaalde resultaten.

### 1. Build CPU-only wheel

```bash
rm -rf /tmp/wx1-wheelhouse /tmp/wx1-cpu-only
mkdir -p /tmp/wx1-wheelhouse
python3 -m pip wheel --no-deps ./python --wheel-dir /tmp/wx1-wheelhouse
```

Verwacht: maturin bouwt een wheel van het minimale CPU-only package. De basisdependencies blijven leeg; er zijn geen CUDA-, ROCm-, JAX- of GPU-runtimepakketten vereist.

### 2. Installeer en importeer in schone CPU-only omgeving

```bash
python3 -m venv /tmp/wx1-cpu-only
/tmp/wx1-cpu-only/bin/python -m pip install --no-deps /tmp/wx1-wheelhouse/wx1-*.whl
/tmp/wx1-cpu-only/bin/python -c "import wx1; print(wx1.__file__)"
```

Verwacht: de wheel installeert en `wx1` importeert zonder NumPy, xarray, Dask of GPU-runtime. Een import is nog geen native data-roundtrip.

### 3. Installeer NumPy/xarray extra's en native ontwikkelbuild

```bash
python3 -m pip install -e "./python[numpy,xarray]"
```

Verwacht: maturin bouwt de native extension en pip installeert wx1 met NumPy/xarray extras.

### 4. Volledige Python-suite

```bash
PYTHONPATH=python python3 -m pytest -q -rs python/tests
```

Verwacht: wrapper-, xarray- en beschikbare adaptertests slagen. Na native build mogen de native NumPy-tests niet worden overgeslagen. Noteer elk skip van een optionele dependency of ontbrekende lokale fixture afzonderlijk.

### 5. Native NumPy roundtrips

```bash
PYTHONPATH=python python3 -m pytest -q python/tests/test_native_roundtrip.py
```

Verwacht: little-endian F32, big-endian F32, U16, F64 en Bool8 roundtrips slagen; de tests vergelijken dtype, shape en waarden/bitpatronen.

### 6. Dask native WX-1 → xarray → Dask

```bash
python3 -m pip install -e "./python[dask]"
PYTHONPATH=python python3 -m pytest -q python/tests/test_native_roundtrip.py::test_native_xarray_dask_chunked_read_and_reduction
```

Verwacht: native WX-1-fixture wordt als Dask array geopend en een chunked mean levert dezelfde waarde als de NumPy-bron.

### 7. NetCDF4 en Zarr lokale fixtures

```bash
python3 -m pip install -e "./python[netcdf,zarr]"
PYTHONPATH=python python3 -m pytest -q python/tests/test_external_adapters.py::test_netcdf4_local_fixture_when_engine_is_installed python/tests/test_external_adapters.py::test_zarr_local_roundtrip_when_dependency_is_installed
```

Verwacht: beide adapters lezen lokaal gemaakte fixtures en behouden geteste waarden, coördinaten en attributen. Zarr-fixture gebruikt expliciet Zarr v2 wanneer de geïnstalleerde stack dat ondersteunt.

### 8. GRIB2/ecCodes lokale fixture

```bash
: "${WX1_GRIB2_FIXTURE:?Zet WX1_GRIB2_FIXTURE naar een bestaand lokaal GRIB2-fixturebestand}"
python3 -m pip install -e "./python[grib2]"
PYTHONPATH=python python3 -m pytest -q python/tests/test_external_adapters.py::test_grib2_local_fixture_when_path_is_supplied
```

Verwacht: cfgrib/ecCodes opent het lokaal aangeleverde bestand en levert ten minste één data-variable. Zonder fixture wordt dit onderdeel `niet getest`; er is geen netwerkdownload.
