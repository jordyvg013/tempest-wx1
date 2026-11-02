# ADR 0032 — Stable C ABI, cbindgen, and Foreign Function Interface (wx1-ffi)

## Status
Accepted (Fase 4, Onderdeel 9)

## Context
WX-1 is written in Rust, with an official high-level Python interface via PyO3. However, operational numerical weather prediction (NWP) infrastructure (e.g., ECMWF IFS, DWD ICON, KNMI Harmonie, NOAA GFS) and scientific models (such as Tempest or Fortran/C-based physics engines) require direct interoperability without a Python runtime or JVM.

To make WX-1 universally accessible across C, C++, Fortran (via `iso_c_binding`), Julia, and external workflow runners:
1. WX-1 must expose a stable, platform-independent, versioned C Foreign Function Interface (C-ABI) compiled as a shared library (`libwx1_ffi.so`, `libwx1_ffi.dylib`, `wx1_ffi.dll`) and static library (`libwx1_ffi.a`).
2. Memory management, ownership, and buffer lifecycles must be unambiguous to prevent leaks and double-frees across the FFI boundary.
3. Panics in Rust must never unwind across the FFI boundary (which is undefined behavior in C). All API functions must catch unwinding and map results to explicit C integer status codes (`wx1_error_t`).
4. Array reads must support caller-allocated buffers to enable zero-copy reads directly into existing application arrays (e.g., Fortran multidimensional arrays or C heap blocks).
5. The C header file (`include/wx1.h`) must be maintained as an authoritative golden contract, verified against the Rust implementation.

## Decision

### 1. Dedicated C-ABI Crate (`crates/wx1-ffi`)
We establish a dedicated crate `crates/wx1-ffi` in the workspace:
- `crate-type = ["cdylib", "staticlib", "rlib"]`
- Depends on `wx1-format` using safe internal APIs.
- While the core workspace enforces `#![forbid(unsafe_code)]`, `wx1-ffi` selectively allows `unsafe` strictly at the raw pointer boundary (`*const c_char`, `*mut wx1_file_t`). Every raw pointer is validated for nullness, alignment, and wrapped in `std::panic::catch_unwind`.

### 2. Opaque Handle Architecture
C callers interact exclusively through opaque pointer types:
- `typedef struct wx1_file wx1_file_t;`
- Internal state encapsulates the open file descriptor and the scientific snapshot model.
- Pointers are allocated via `Box::into_raw` on open, and deallocated via `Box::from_raw` only in `wx1_close_file`. Passing a null or previously freed pointer returns `WX1_ERROR_NULL_POINTER` / `WX1_ERROR_INVALID_ARGUMENT`.

### 3. Error Model & Thread-Local Diagnostics
All functions returning operations yield `wx1_error_t`:
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
When an error occurs, an explanatory diagnostic message is stored in thread-local storage and queryable via:
```c
const char* wx1_last_error_message(void);
```

### 4. Direct Buffer Slicing (Caller-Allocated)
For scientific array reads, the caller allocates the destination buffer and passes its capacity:
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
- If `buffer_len` is smaller than the required slice byte length, the function sets `*out_bytes_written` to the required length and returns `WX1_ERROR_BUFFER_TOO_SMALL` without modifying `out_buffer`.
- If `buffer_len` is sufficient, data is decoded and copied directly into the caller's buffer in canonical C row-major layout.

### 5. Golden Header & ABI Invariance
- Canonical header: `include/wx1.h`.
- cbindgen configuration: `cbindgen.toml`.
- An integration test (`tests/c_integration/`) compiles a standalone C application using `gcc` against `include/wx1.h` and verifies reading golden vector `gv01_valid_scalar_2d_zstd.wx1`.

## Consequences
- **Positive**:
  - Unlocks native integration for C, C++, Fortran, Julia, Tempest, and embedded meteorological edge systems.
  - Zero Python dependencies required for native deployments.
  - Callers can read directly into their own pre-allocated arrays, avoiding intermediate allocations.
- **Negative / Trade-offs**:
  - Introduces an FFI boundary that requires rigorous null-checking and panic guards.
  - Any future change to exported C functions or structs must maintain backward compatibility or bump the ABI major version.
