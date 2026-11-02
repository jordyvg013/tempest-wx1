# ADR 0027 — Parameter Registries, CF/WMO Mappings, Provenance, and Safe Units

- **Status:** Accepted
- **Date:** 2026-11-02
- **Specification Draft:** `docs/spec/WX1_Parameter_Registry_And_Units_Draft_v0.1.md`

## Context

Meteorological models and downstream forecasting systems rely on strict, verifiable physical parameters:
1. **WMO GRIB2 and Local Authority Tables:** Variables originate from GRIB2 codes (Discipline, Category, Number) or local center tables (e.g. ECMWF, KNMI CY46), which must map unambiguously to canonical parameter identifiers.
2. **Climate and Forecast (CF) Metadata:** Standard names and units must follow consistent conventions without ambiguity.
3. **Safe Physical Units and Dimensional Checks:** Physical operations and table overrides must not silently change physical dimensions (e.g. pressure into temperature or energy flux into velocity). Safe linear conversions (e.g. Celsius to Kelvin, hPa to Pa, km/h to m/s) should be mathematically exact and auditable.
4. **Dataset Provenance:** Forecast reference times, data providers, software versions, and lineage history must be retained across file generations and operations.

Previously, `wx1-registry` existed as a standalone crate with CSV-based table import/export, and `wx1-format` had basic text-based provenance. A unified, tested bridge connecting `wx1-format` variables to parameter registries, CF standard names, safe unit dimensions, and provenance lineage was missing.

## Decision

1. **Extend `wx1-registry` with Expanded Meteorological Unit Dimensions & Conversions:**
   - Expand the built-in dimensional vocabulary to cover `temperature`, `pressure`, `length`, `time`, `velocity`, `mass_fraction`, `specific_energy`, `mass_per_area`, `energy_flux`, `dimensionless`, `angular_velocity`, `density`, and `precipitation_rate`.
   - Provide `unit_dimension(unit: &str) -> Option<&'static str>` and `unit_conversion_factor(from: &str, to: &str) -> Result<(f64, f64), RegistryError>` that returns scale and offset for safe linear conversion ($y = x \cdot \text{scale} + \text{offset}$).
   - Reject conversions across different physical dimensions with `RegistryError::IncompatibleUnits`.

2. **Standard Built-in WMO and CF Registry Profiles:**
   - Provide built-in standard tables in `wx1-registry`:
     - `wmo.grib2` Table 4.2 standard parameters (e.g. `air_temperature`, `dew_point_temperature`, `surface_pressure`, `geopotential_height`, `u_component_of_wind`, `v_component_of_wind`, `total_precipitation`).
     - CF standard name mapping helper: associate canonical parameters with CF `standard_name` and standard canonical units.

3. **Bridge Between `wx1-format` and `wx1-registry`:**
   - Introduce helper methods in `Variable` and `ScientificDataset` to attach and validate registry parameter attributes:
     - `Variable::with_registry_parameter(param: &Parameter)`
     - `Variable::canonical_id(&self) -> Option<&str>`
     - `Variable::standard_name(&self) -> Option<&str>`
     - `Variable::physical_dimension(&self) -> Option<&'static str>`
   - Enhance `Provenance::append_history(&mut self, entry: &str)` to support ISO 8601 audit trailing.

4. **Python Bindings and Convenience API:**
   - Expose parameter registry querying, CF parameter resolution, unit compatibility checking, and provenance inspection in `wx1` Python bindings.

## Consequences

- **Positive:**
  - Prevents physical unit bugs and accidental unit mismatch during model input/output.
  - Full traceability of GRIB2 codes, CF standard names, and processing lineage.
  - 100% wire-compatible with existing CBOR schema v1/v2; no breaking changes to on-disk layouts.
- **Negative / Trade-offs:**
  - The built-in unit converter is deliberately conservative (linear scale + offset) and does not attempt full logarithmic or non-linear unit calculus (e.g. decibels or non-linear psychrometric units remain explicitly declared).
