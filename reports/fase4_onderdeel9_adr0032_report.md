# WX-1 Fase 4, Onderdeel 9: ADR 0032 — Stabiele C-ABI, cbindgen en Golden Files (`wx1-ffi`)

**Datum**: 2026-11-02 19:45:00 CET  
**Status**: Geaccepteerd & Geïmplementeerd (ADR 0032)  
**Compiler / Toolchain**: Rust 1.99.1 | GCC 13.3.0 | Linux x86_64 WSL2  
**Codebase Integriteit**: `#![forbid(unsafe_code)]` op alle kern-crates (`wx1-format`, `wx1-registry`, `wx1-cli`); strikt begrensde en gecontroleerde `unsafe` uitsluitend op de raw pointer boundary van `wx1-ffi` met null-checks en `catch_unwind`.

---

## 1. Doel & Scope van ADR 0032

Voor operationele weermodellen (zoals ECMWF IFS, KNMI Harmonie, DWD ICON) en wetenschappelijke HPC-omgevingen in C, C++, Fortran (via `iso_c_binding`), Julia en Tempest is een betrouwbare, runtime-onafhankelijke C-API vereist.

In deze fase is een native C Foreign Function Interface (`crates/wx1-ffi`) opgeleverd die gecompileerd wordt naar:
* `target/release/libwx1_ffi.so` (dynamische bibliotheek, 1.2 MB);
* `target/release/libwx1_ffi.a` (statische bibliotheek, 26 MB);
* `include/wx1.h` (canonieke golden C header).

---

## 2. Architectuur & Veiligheidsgaranties

### 2.1 Opaque Handle & Memory Ownership
Aanroepers beheren container-instanties uitsluitend via een opaque pointer:
```c
typedef struct wx1_file_t wx1_file_t;

wx1_error_t wx1_open_file(const char* path, wx1_file_t** out_file);
wx1_error_t wx1_close_file(wx1_file_t* file);
```
* Het geheugen wordt gealloceerd in Rust (`Box::into_raw`) en uitsluitend vrijgegeven in Rust (`Box::from_raw` binnen `wx1_close_file`).
* Null-pointers of dubbele frees worden afgevangen vóór dereferencing.

### 2.2 Foutafhandeling & Panic Safety
Panics in Rust mogen **nooit** de FFI-grens overschrijden (dit is undefined behavior in C). Alle C-functies zijn ingepakt in `std::panic::catch_unwind`:
```c
typedef enum wx1_error {
    WX1_OK = 0,
    WX1_ERROR_NULL_POINTER = 1,
    WX1_ERROR_INVALID_ARGUMENT = 2,
    WX1_ERROR_IO = 3,
    WX1_ERROR_CORRUPT_DATA = 4,
    WX1_ERROR_VARIABLE_NOT_FOUND = 5,
    WX1_ERROR_SLICE_OUT_OF_BOUNDS = 6,
    WX1_ERROR_BUFFER_TOO_SMALL = 7,
    WX1_ERROR_PANIC = 8
} wx1_error_t;
```
Bij een fout bevat thread-local storage een gedetailleerde diagnostische foutmelding die direct queryable is via:
```c
const char* wx1_last_error_message(void);
```

### 2.3 Direct Caller-Allocated Slicing
In plaats van buffers in de FFI-laag te alloceren en te kopiëren, declareert de C/Fortran aanroeper zijn eigen doelgeheugen:
```c
wx1_error_t wx1_read_array_slice(
    wx1_file_t* file,
    const char* var_name,
    const uint64_t* start,
    const uint64_t* count,
    size_t rank,
    uint8_t* out_buffer,
    size_t buffer_len,
    size_t* out_bytes_written
);
```
* **Buffer-Te-Klein Beveiliging**: Als `buffer_len` ontoereikend is, retourneert de functie `WX1_ERROR_BUFFER_TOO_SMALL`, vult `*out_bytes_written` met de exact vereiste grootte, en laat het doelgeheugen intact.
* **Directe Data-overdracht**: Data wordt direct in C row-major layout weggeschreven, ideaal voor directe binding aan Fortran arrays (`dimension(16, 16)`) of C arrays.

---

## 3. C Integratietest & GCC Verificatie

In [`tests/c_integration/test_c_abi.c`](file:///mnt/d/AI/weer-ai/Tempest/WX-1/tests/c_integration/test_c_abi.c) is een end-to-end integratieprogramma gerealiseerd en gecompileerd via GCC (`gcc -Wall -Wextra -Werror`):
1. **Versiecontrole**: Valideert `wx1_version_string()`.
2. **Foutdetectie**: Opent niet-bestaand bestand; controleert `WX1_ERROR_IO` en de inhoud van `wx1_last_error_message()`.
3. **Golden Vector Validatie**: Opent [`gv01_valid_scalar_2d_zstd.wx1`](file:///mnt/d/AI/weer-ai/Tempest/WX-1/tests/fixtures/golden_vectors/gv01_valid_scalar_2d_zstd.wx1).
4. **Metadata & Generatie**: Valideert `dataset_id = "gv01"` en `generation = 1`.
5. **Inspectie**: Valideert variabele `t2m`, rank 2, shape `[16, 16]`, en datatype `WX1_DTYPE_F32`.
6. **Capaciteitsgrenzen**: Test buffer te klein (`small_buf[100]`) $\to$ retourneert netjes `WX1_ERROR_BUFFER_TOO_SMALL` en rapporteert 1024 benodigde bytes.
7. **Bit-Exact Array Lezing**: Leest de $16 \times 16$ array uit in een lokale `float data[256]` array. Waarden zijn bit-exact identiek ($273.15\text{ K} \dots 298.65\text{ K}$).
8. **Schone Afsluiting**: `wx1_close_file` zonder geheugenlekken.

---

## 4. Verificatiestatus

De volledige workspace is gecontroleerd:
* **`cargo fmt --all -- --check`**: Geslaagd (0 diffs).
* **`cargo clippy --workspace --all-targets -- -D warnings`**: Geslaagd (0 warnings over alle 5 crates: `wx1-format`, `wx1-registry`, `wx1-cli`, `wx1-ffi`, `wx1-python-native`).
* **`cargo test --workspace`**: **163 geslaagd, 0 mislukt**.
  * `wx1-ffi`: 4 tests (inclusief automatische compilatie en uitvoering van het externe C binary testprogramma via `gcc`).
* **`pytest python/tests/`**: **45 geslaagd, 3 geskipt, 0 mislukt**.

---

## 5. Conclusie Fase 4, Onderdeel 9
Fase 4, Onderdeel 9 (ADR 0032) is **volledig afgerond en geverifieerd**. WX-1 beschikt nu over een geharde, geteste C-ABI en golden header voor directe integratie in C-, C++-, Fortran- en Julia-ecosystemen.
