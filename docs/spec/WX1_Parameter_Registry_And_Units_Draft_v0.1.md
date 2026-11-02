# WX-1 Parameter Registries, Provenance, and Safe Units — Draft v0.1

- **Status:** Draft specification
- **Date:** 2026-11-02
- **Author:** Tempest / WX-1 Core Team
- **Tracking ADR:** ADR 0027

---

## 1. Context and Problem Statement

Scientific datasets and numerical weather prediction (NWP) models (e.g. ECMWF IFS, KNMI HARMONIE-AROME, NOAA GFS, and Tempest NWP) store physical variables that map to external international and meteorological authority standards:
1. **WMO GRIB2 Code Tables:** e.g. Table 4.2 (`discipline`, `parameterCategory`, `parameterNumber`), plus local centre/subcentre expansions (e.g. ECMWF centre 98, KNMI centre 99).
2. **Climate and Forecast (CF) Metadata Conventions:** `standard_name`, canonical long names, and cell methods.
3. **Canonical Identifiers & Physical Units:** UDUNITS-compatible unit strings (e.g. `K`, `degC`, `m s-1`, `hPa`), unit dimensions (temperature, pressure, velocity, flux), and safe unit conversions.
4. **Provenance & Auditability:** Exact model runs, source observation sets, software versions, processing lineage, reference forecast times, and immutable attribution.

Previously, `wx1-registry` provided an isolated, CSV-based parameter registry and overlay mechanism, while `wx1-format` contained a basic `Provenance` struct and untyped text attributes. There was no direct, verified integration between `wx1-format` scientific variables, `wx1-registry` parameter resolution, unit compatibility checking, and Python convenience APIs.

---

## 2. Parameter Identity and Namespace Conventions

A parameter identity in WX-1 consists of:
- **`canonical_id`**: A normalized, unique ASCII snake_case string (e.g. `air_temperature`, `surface_air_pressure`, `eastward_wind`).
- **`standard_name` (CF convention)**: The exact CF Standard Name string if defined (e.g. `air_temperature`).
- **`units`**: Physical unit string conforming to canonical dimensional representations.
- **`external_mappings`**: Zero or more mappings to meteorological authority codes:
  - `wmo_grib2`: `(discipline, category, number, centre, subcentre)`
  - `ecmwf_param_id`: e.g. `130` for `t`, `167` for `2t`, `129` for `z`.
  - `knmi_harmonie`: local cy46 parameter keys.

### Namespace Resolution Rule
In WX-1 dataset variables, the canonical identity and registry resolution are declared via canonical variable attributes or typed registry bindings:
- `"wx1:canonical_id"`: Text (e.g. `"air_temperature"`)
- `"wx1:standard_name"`: Text (e.g. `"air_temperature"`)
- `"wx1:units"`: Text (e.g. `"K"`)
- `"wx1:registry_namespace"`: Text (e.g. `"wmo.grib2"`)
- `"wx1:registry_table"`: Text (e.g. `"4.2"`)
- `"wx1:registry_version"`: Text (e.g. `"2025"`)
- `"wx1:grib2_code"`: Text (e.g. `"0:0:0"` for discipline 0, category 0, number 0) or structured attributes.

---

## 3. Physical Unit Dimensions and Safe Dimensional Compatibility

WX-1 enforces that physical unit transformations or overrides must preserve physical dimensions. The dimension vocabulary covers all core meteorological and NWP variables:

| Physical Dimension | Standard Canonical Unit | Accepted Equivalent Representations |
| :--- | :---: | :--- |
| `temperature` | `K` | `kelvin`, `degC`, `degree_Celsius`, `°C` |
| `pressure` | `Pa` | `pascal`, `hPa`, `mbar`, `bar` |
| `length` | `m` | `meter`, `metre`, `km`, `mm` |
| `time` | `s` | `second`, `min`, `minute`, `h`, `hour`, `d`, `day` |
| `velocity` | `m/s` | `m s-1`, `m s^-1`, `km/h`, `knots` |
| `mass_fraction` | `kg/kg` | `kg kg-1`, `g/kg` |
| `specific_energy` | `m2/s2` | `m^2 s^-2`, `J/kg`, `J kg-1` |
| `mass_per_area` | `kg/m2` | `kg m-2`, `mm` (water equivalent) |
| `energy_flux` | `W/m2` | `W m-2` |
| `dimensionless` | `1` | `dimensionless`, `%`, `percent`, `fraction` |
| `angular_velocity` | `s-1` | `s^-1` (vorticity, divergence) |
| `density` | `kg/m3` | `kg m-3` |
| `precipitation_rate` | `kg m-2 s-1` | `mm/s`, `mm/h` |

### Conversion Semantics
When safe linear unit conversion is requested between compatible units $(u_1, u_2)$ within the same physical dimension:
$$y = (x \cdot \text{scale}) + \text{offset}$$
Examples:
- `degC` $\to$ `K`: $\text{scale} = 1.0$, $\text{offset} = 273.15$
- `hPa` $\to$ `Pa`: $\text{scale} = 100.0$, $\text{offset} = 0.0$
- `km/h` $\to$ `m/s`: $\text{scale} = 1 / 3.6$, $\text{offset} = 0.0$

If dimensions do not match (e.g. attempting to convert `Pa` to `K`), the operation fails immediately with `RegistryError::IncompatibleUnits`.

---

## 4. Provenance and Processing Lineage

The `Provenance` record in `ScientificDataset` tracks the origin and processing chain:
- `source`: Dataset originator (e.g. `"ECMWF IFS Cycle 48r1"`, `"KNMI HARMONIE-AROME cy46"`).
- `provider`: Data center (e.g. `"ECMWF"`, `"KNMI"`, `"NOAA"`).
- `product`: Product name (e.g. `"oper"`, `"enfo"`, `"reanalysis"`).
- `institution`: Responsible institute.
- `license`: Open data or usage license (e.g. `"CC-BY-4.0"`, `"ECMWF Terms of Use"`).
- `history`: ISO 8601 timestamped processing log (e.g. `"2026-11-02T10:00:00Z: WX-1 lossy quantization applied (epsilon=0.01)"`).
- `reference_time`: Forecast reference / initialization time (ISO 8601 UTC).

When datasets are subset, merged, quantized, or converted, the history field must append lineage entries deterministically without destroying existing audit trails.

---

## 5. Wire Format Compatibility

- All metadata fields for parameters, units, and provenance are represented within existing CBOR schema v1/v2 definitions (`ScientificDataset.provenance`, `Variable.attributes`, and `Registry` tables).
- No byte-level wire format change or container header change is required.
