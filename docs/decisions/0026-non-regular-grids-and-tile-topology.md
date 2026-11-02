# ADR 0026 — Non-Regular Grids, Cubed-Sphere Topology, Halos, Arakawa C Staggering, and Domain Nesting

- **Status:** Accepted and implemented (Fase 4)
- **Date:** 2026-11-02
- **Specification Draft:** `docs/spec/WX1_Non_Regular_Grids_And_Topology_Draft_v0.1.md`

## Context

Global and regional NWP models require storage representations that preserve native grid topology without forcing lossy or expensive regridding to regular latitude-longitude arrays during model execution or checkpointing (`restart` and `output`). Specifically:
1. Global spectral/grid-point models (such as ECMWF IFS) use octahedral reduced Gaussian grids (`O1280`), while finite-volume global models (such as FV3 and future Tempest/JAX-NWP runs) use 6-panel cubed-sphere grids (`C96`, `C192`, `C384`) with explicit panel orientations, edge connectivity, and halo exchange zones.
2. Regional NWP models (such as KNMI HARMONIE-AROME CY46) and dynamical cores use Arakawa C-grid velocity staggering ($u$ on zonal cell faces, $v$ on meridional cell faces, $\zeta$ on cell corners, $w$ on vertical half-levels) and nested child domains.
3. Users and NWP dynamical cores need to read a single cubed-sphere tile (`R9`) or strip halo cells (`compute_interior_selection`) directly using validated metadata, without changing the underlying WX-1 container or CBOR schema v1 binary layout.

## Decision

1. **Dedicated Topology Module (`crates/wx1-format/src/topology.rs`):**
   - Introduce strongly-typed Rust structures:
     - `GridFamily` (`RegularLatLon`, `Gaussian`, `ReducedGaussian`, `LambertConformal`, `CubedSphere`, `YinYang`, `Icosahedral`)
     - `PanelEdge` (`West`, `East`, `South`, `North`), `EdgeNeighbor`, `PanelConnectivity`, and `CubedSphereTopology` (with standard 6-tile cubed-sphere connectivity constructor `CubedSphereTopology::standard(c_resolution, gnomonic_type)` and mathematical involution check `validate_connectivity()`).
     - `ReducedGaussianTopology` (`gaussian_n`, `is_octahedral`, `ring_point_counts: Vec<u32>`), including an octahedral constructor `ReducedGaussianTopology::octahedral(n)` where ring $i \in [1, N]$ from the pole has $4i + 16$ points.
     - `HaloSpec` (`south`, `north`, `west`, `east`) and `StaggerLocation` (`HorizontalStagger::{Center, EdgeU, EdgeV, Corner}`, `VerticalStagger::{FullLevel, HalfLevel}`).
     - `NestingDescriptor` (`domain_id`, `parent_domain_id`, `refinement_ratio_y`, `refinement_ratio_x`, `parent_start_y`, `parent_start_x`, `two_way_feedback`).
2. **Additive Canonical Encoding into `GridMapping` and `Variable`:**
   - `GridTopologyDescriptor::to_grid_mapping(name)` and `GridTopologyDescriptor::from_grid_mapping(&GridMapping)` serialize and validate all topology fields deterministically via `GridMapping::attributes`.
   - `VariableGridLayout::apply_to_variable(&mut Variable)` and `VariableGridLayout::from_variable(&Variable)` encode per-variable Arakawa staggering, halo widths, and tile axis binding into `Variable::attributes`.
   - Existing `ScientificDataset` CBOR schema v1 remains 100% wire-compatible.
3. **Topology-Aware Read & Validation Helpers:**
   - `ScientificDataset::validate_grid_topologies()` verifies that all variables referencing a cubed-sphere, Yin-Yang, or Arakawa-staggered `GridMapping` have matching tile dimensions, valid halo widths (strictly smaller than axis lengths), and consistent Arakawa C-grid dimensions ($N_x + 1$ for `EdgeU`, $N_y + 1$ for `EdgeV`, $N_z + 1$ for `HalfLevel` after accounting for halos).
   - `ScientificDataset::compute_interior_selection(variable_path)` and `ScientificDataset::plan_tile_slice(variable_path, tile_index, include_halos)` (plus `ScientificSnapshot::read_tile_slice`) enable direct $O(k)$ planning and reading of individual cubed-sphere tiles (`R9`) with or without halo zones.

## Consequences

- **Positive:**
  - Full support for `C96`, `C192`, `C384` cubed-sphere models, Arakawa C-grids, halos, octahedral reduced Gaussian grids (`O96 = 40320`, `O1280 = 6599680`), and nested domains with zero wire-format breakage.
  - Native tile-level planning and reading (`R9`) leverages the $O(k)$ indexed array planner (ADR 0028) and parallel chunk decompression: on `C192 L60` (`56.45 MB` raw noisy float32 field from real ECMWF 0.1 NH PL patches + continuous NWP turbulence), reading 1 cubed-sphere tile (`39.66 ms` interior / `39.75 ms` with halos) is **6.86x faster** than reading all 6 tiles (`272.13 ms`), and faster than `NETCDF4_ZLIB4_SHUFFLE` (`58.11 ms` 1-tile, `57.15 ms` open vs `8.00 ms` WX-1 open) while matching compressed size (`30.54 MB` / `54.09%` of raw vs `30.57 MB` Zarr 3 Blosc+shuffle and `30.38 MB` NetCDF4 zlib4+shuffle).
  - Asynchronous per-timestep cubed-sphere appends (`W5`, 6 steps of `C192 L60` = `56.45 MB/step` with a fixed `500.0 ms` stub model compute step and 8 staging workers via ADR 0025 `AsyncScientificWriter`) achieve a median model-thread I/O stall of **`49.15 ms` (`9.83%` of model step)** in `buffered` mode and **`48.60 ms` (`9.72%` of model step)** in `durable` mode (`0.000 ms` queue backpressure wait).
  - Geometric 3D unit-sphere continuity (`gnomonic_xyz`), 24-edge halo index mapping (`map_edge_halo_index`), and post-exchange halo verification (`verify_cubed_sphere_edge_halos` with `HaloConsistencyState`) are verified in automated tests.
  - Older readers that do not inspect `"wx1:topology_capability_version"` (`1`) continue to read the raw multidimensional tensor `[tile, lev, y, x]` without breaking wire compatibility.
- **Negative / Trade-offs:**
  - Regridding between cubed-sphere and regular lat/lon is intentionally not performed implicitly inside `read_array_slice`; any coordinate transformation remains an explicit external step.
