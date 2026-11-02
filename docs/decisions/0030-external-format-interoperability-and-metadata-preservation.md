# ADR 0030 — External Format Interoperability (GRIB2, NetCDF-4, Zarr) and Lossless Metadata Preservation

## Status
Accepted (Fase 4, Onderdeel 7)

## Context
Operational weather prediction centers, climate researchers, and meteorological service providers rely on diverse file formats:
1. **WMO GRIB2**: Used by ECMWF, NCEP, DWD, and KNMI for operational model dissemination. GRIB2 encodes parameters using numerical tables (Discipline, Category, Number, Level Type, Forecast Time).
2. **NetCDF-4 / HDF5**: Used for climate models, satellite data (e.g. EUMETSAT MTG), and observational datasets, adhering to Climate and Forecast (CF) Metadata Conventions.
3. **Zarr (v2 / v3)**: Used for cloud-native geospatial and NWP data stores with chunked, compressed arrays.

To facilitate seamless adoption and pipeline integration, WX-1 must provide robust bidirectional import and export:
- Data payloads must be faithfully preserved, achieving bit-exact fidelity for lossless fields.
- Metadata must not be discarded, approximated, or guessed. WMO code tables, CF standard names, physical units, coordinate systems, fill values, and format-specific attributes must be preserved across conversions.
- Prior to ADR 0030, WX-1 provided read-only adapter stubs (`open_dataset`) that lacked export routines, full attribute mapping, and roundtrip verification.

## Decision
1. **Bidirectional Adapter Architecture**:
   Implement full bidirectional import and export in `python/wx1/adapters/` and expose canonical top-level functions in `wx1`:
   - `wx1.from_grib(input_path, output_wx1_path, ...)` & `wx1.to_grib(input_wx1_path, output_grib_path, ...)`
   - `wx1.from_netcdf(input_path, output_wx1_path, ...)` & `wx1.to_netcdf(input_wx1_path, output_nc_path, ...)`
   - `wx1.from_zarr(input_path, output_wx1_path, ...)` & `wx1.to_zarr(input_wx1_path, output_zarr_path, ...)`

2. **Systematic Metadata Namespacing & Preservation**:
   All metadata from external formats is translated into WX-1 container attributes using standardized namespace prefixes:
   - **GRIB2 Metadata (`grib:`)**:
     - `grib:discipline`, `grib:parameterCategory`, `grib:parameterNumber`
     - `grib:shortName`, `grib:typeOfLevel`, `grib:level`
     - `grib:dataDate`, `grib:dataTime`, `grib:forecastTime`, `grib:stepUnits`
     - `grib:gridType`, `grib:shapeOfTheEarth`
   - **NetCDF / CF Conventions (`cf:` / standard attributes)**:
     - `cf:standard_name`, `cf:long_name`, `units`
     - `_FillValue`, `missing_value`, `scale_factor`, `add_offset`
     - `cf:coordinates`, `cf:grid_mapping_name`
   - **Zarr Metadata (`zarr:`)**:
     - `zarr:chunk_shape`, `zarr:order`, `zarr:dimension_names`
   - **Provenance & Source Tracking (`wx1:`)**:
     - `wx1:source_format` (`"grib2"`, `"netcdf4"`, `"zarr"`)
     - `wx1:source_file`, `wx1:converted_at`, `wx1:profile`
   - **Unknown Attributes**:
     - Any unrecognized or user-defined attribute is preserved verbatim in the attribute map without guessing or discarding.

3. **Bit-Exact Fidelity & Special Values**:
   - Lossless floating-point and integer arrays must roundtrip with zero error ($E_{\max} = 0.0$).
   - IEEE 754 NaN values, infinities, and designated fill values must retain their exact bit patterns or canonical fill semantics.
   - When lossy compression is explicitly configured, error bounds are strictly enforced and documented in the metadata attributes.

4. **Multi-Format Command-Line Interface (`wx1 convert`)**:
   Implement a CLI conversion command that automatically detects input and output format extensions (`.wx1`, `.grib2`, `.nc`, `.zarr`) and executes the corresponding adapter pipeline with streaming or chunked memory bounds.

## Consequences
- WX-1 can ingest and export operational ECMWF and KNMI GRIB2 products, EUMETSAT NetCDF satellite data, and cloud Zarr stores without information loss.
- Scientific metadata is preserved end-to-end, allowing data to be exported back to GRIB2 or NetCDF with valid WMO headers.
- Safe CPU-only execution without requiring complex native C compile-time dependencies in the base Rust crate.
