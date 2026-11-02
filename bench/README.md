# WX-1 cross-format benchmark suite — gepland

**Status:** ontwerp/roadmap; de volledige suite is nog niet geïmplementeerd.

## Twee gescheiden benchmarksystemen

1. **Interne Rust-microbenchmark — ADR 0019.** `crates/wx1-format/benches/wx1_performance.rs` meet interne WX-1-codepaden (Raw/Zstandard/LZ4, open/validate, array-read en de huidige lineaire planner). Dit is geen vergelijking met andere formaten.
2. **Cross-format-suite — ADR 0021.** Deze `bench/`-boom vergelijkt WX-1, GRIB2/ecCodes, NetCDF4 en Zarr 3 via adapters, gedeelde fixtures en gelijke workload-/precisie-/foutgrenssemantiek. Deze suite is nog niet gebouwd.

Beide blijven bestaan en worden afzonderlijk gerapporteerd. De interne microbenchmark kan instellingen en interne bottlenecks helpen onderzoeken, maar vervangt de cross-format-suite niet.

## Fasegate

Begin niet aan formele formatvergelijkingen voordat:

1. Python/CPU-only, NumPy, xarray, Dask en functionele lokale adapters uit Fase 1 geverifieerd zijn;
2. lokale fixtures en adapter-roundtrips beschikbaar zijn;
3. lossless/lossy vergelijkingsprofielen en exacte foutgrenzen vastliggen;
4. `config/acceptance.yaml` is vastgezet vóór de eerste formele vergelijking;
5. versie, hardware, filesystem, toolchain, compilerflags en meetmethode reproduceerbaar vastgelegd zijn.

## Workloads en meetmodi

- Reads: R1–R9, koud en warm afzonderlijk.
- Writes: W1–W6; W2 append en W4 restart rapporteren buffered en durable writes afzonderlijk.
- W6 vergelijkt compute-only, synchrone WX-1, asynchrone WX-1, asynchrone Zarr 3 en asynchrone NetCDF4. CPU eerst; GPU pas na de CPU/API-gate.
- C1 procescrash (`kill -9`) is geen fysieke power-loss-test.
- De v0.4.3 Cargo-microbenchmark sluit `fsync` uit en is dus geen duurzame schrijfmeting.

## Bevroren acceptatiedrempels

`config/acceptance.yaml` bevat de door de gebruiker goedgekeurde doelen: R1 100-GB/1-GB open-tijd-ratio ≤2,0; R2 minstens 2× sneller dan zowel GRIB2 als Zarr 3 (koud én warm); bestandsgrootte kleiner dan GRIB2 CCSDS bij gelijke foutgrens; W5 I/O-stall ≤5% van compute-only modelstaptijd; W6 async overhead ≤5%; D4 async overhead ≤5% en niet slechter dan Zarr 3. Dit zijn acceptatiedoelen, geen behaalde resultaten. Wijzig ze niet na inzage van benchmarkresultaten.

## Beoogde reproduceerbare interface

`make bench-data`, `make bench-run`, `make bench-report` zijn doelcommando's. Ze bestaan nog niet zolang de `Makefile` en targets niet zijn gebouwd en getest.

Datasets worden niet in git opgeslagen. Downloadscript, licentie-/bronmetadata en SHA256-manifest worden wel opgeslagen. Tests en benchmarkruns gebruiken lokale fixtures; netwerk is niet nodig tijdens een offline run.
