# WX-1 Non-Regular Grids, Cubed-Sphere Topology, Halos, Arakawa C Staggering, and Domain Nesting — Draft v0.1

**Status:** Draft v0.1 (Fase 4 — ADR 0026)  
**Target crate:** `wx1-format` (`crates/wx1-format/src/topology.rs`, `crates/wx1-format/src/scientific.rs`) & `wx1-python`  
**Wire compatibility:** 100% additive. No changes to container headers, footers, chunk descriptors, or CBOR schema v1/v2 array arity. Structured grid topology, halo metadata, Arakawa staggering, and nesting descriptors are encoded into deterministic `GridMapping` and `Variable` attribute maps (`wx1:grid_type`, `wx1:cs_*`, `wx1:halo_*`, `wx1:stagger_*`, `wx1:nest_*`) and validated via `GridTopologyDescriptor` and `VariableGridLayout`.

---

## 1. Motivation and Requirements

Modern global and regional Numerical Weather Prediction (NWP) models—including spectral/grid-point global models (IFS reduced Gaussian), finite-volume cubed-sphere models (FV3 / Tempest / JAX-NWP `C96`, `C192`, `C384`), icosahedral models (ICON), Yin-Yang grids, and regional mesoscale models (HARMONIE-AROME CY46 on Lambert Conformal with Arakawa C-grid staggering)—do not store variables exclusively on simple un-staggered regular latitude-longitude grids.

Per `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md` §4.5, WX-1 must natively describe and validate:
1. **Grid families (`GridFamily`):**
   - `RegularLatLon`
   - `Gaussian` (regular Gaussian latitude rings)
   - `ReducedGaussian` (octahedral `O...` or classic `N...` reduced Gaussian with per-latitude-ring `pl` point counts)
   - `LambertConformal` / projected regional grids (e.g., HARMONIE-AROME)
   - `CubedSphere` (6 tiles/panels, `C{N}` resolution, tile orientation, edge/corner connectivity across all 6 faces)
   - `YinYang` (2 overlapping overset panels: `Yin` and `Yang`)
   - `Icosahedral` (unstructured or hierarchical triangular/hexagonal cells)
2. **Cubed-Sphere Topology (`CubedSphereTopology`):**
   - Explicitly 6 tiles (`tile = 0..5`), resolution `c_resolution` ($N \ge 1$, e.g. `96`, `192`, `384`).
   - Explicit 4-edge neighbor connectivity per tile (`west`, `east`, `south`, `north` neighbor tile index `0..5`, target edge, and index orientation flip/rotation flag) so halo exchanges and continuous cross-panel operators can be verified directly from file metadata.
   - Explicit tile memory layout so a single tile (`read_tile`) or all 6 tiles can be read directly into the NWP model's memory layout.
3. **Halo Regions (`HaloSpec`):**
   - Per-spatial-axis halo widths (`[south, north]` and `[west, east]`, or symmetric `halo_width`).
   - Explicit distinction between the **compute domain** (`[halo_start .. length - halo_end]`) and the **allocated array domain** (interior + halo).
   - Helper methods to compute interior (halo-stripped) `ArraySelection` slices or full tile-with-halo slices without copying or guessing offsets.
4. **Arakawa C-Grid Staggering (`StaggerLocation`):**
   - Explicit location of each variable on an Arakawa staggered cell:
     - `CellCenter` (`T`, `q`, `p`, mass points: $N_y \times N_x$)
     - `EdgeU` (zonal/xi normal velocity $u$: $N_y \times (N_x + 1)$ or $N_y \times N_x$)
     - `EdgeV` (meridional/eta normal velocity $v$: $(N_y + 1) \times N_x$ or $N_y \times N_x$)
     - `CornerVorticity` (cell corners $\zeta$: $(N_y + 1) \times (N_x + 1)$)
     - `VerticalFace` (vertical velocity $w$ on $N_z + 1$ half-levels / interfaces)
     - `VerticalCenter` (full model levels $N_z$)
5. **Domain Nesting (`NestingDescriptor`):**
   - Parent-child domain linkage (`domain_id`, `parent_domain_id`, integer refinement ratios `refinement_ratio_y`, `refinement_ratio_x`, parent start indices `parent_start_y`, `parent_start_x`, and `two_way_feedback` flag).
6. **No Silent Regridding:**
   - Original topology and coordinates are preserved bit-for-bit. Regridding (e.g., cubed-sphere to regular lat/lon) is never performed implicitly during read/write.

---

## 2. Canonical Attribute Mapping on `GridMapping` and `Variable`

To preserve 100% wire compatibility with existing `ScientificDataset` CBOR schema v1, `GridTopologyDescriptor` serializes deterministically into `GridMapping::attributes` (and `VariableGridLayout` into `Variable::attributes`):

### 2.1 `GridMapping` Attributes
- `"wx1:grid_family"`: `"regular_latlon" | "gaussian" | "reduced_gaussian" | "lambert_conformal" | "cubed_sphere" | "yin_yang" | "icosahedral"`
- For `CubedSphere`:
  - `"wx1:cs_num_tiles"`: `AttributeValue::Unsigned(6)`
  - `"wx1:cs_resolution"`: `AttributeValue::Unsigned(c_resolution)` (e.g. `96`, `192`, `384`)
  - `"wx1:cs_gnomonic_type"`: `"equiangular" | "equidistant"`
  - `"wx1:cs_connectivity"`: Canonical CBOR or compact text table of the 6×4 panel edge connections (validated to be a symmetric involution on the 24 directed panel edges).
- For `ReducedGaussian`:
  - `"wx1:gaussian_n"`: `AttributeValue::Unsigned(n)` (e.g., `1280` for `O1280`)
  - `"wx1:is_octahedral"`: `AttributeValue::Boolean(true | false)`
  - `"wx1:ring_point_counts"`: `AttributeValue::Bytes(...)` (little-endian `u32` array of length $2N$ giving the exact number of longitude points per latitude ring, validated to be positive and north-south symmetric).
- For `NestingDescriptor`:
  - `"wx1:domain_id"`: `AttributeValue::Text(...)`
  - `"wx1:parent_domain_id"`: `AttributeValue::Text(...)`
  - `"wx1:nest_ratio_y"`, `"wx1:nest_ratio_x"`: `AttributeValue::Unsigned(...)` ($\ge 1$)
  - `"wx1:parent_start_y"`, `"wx1:parent_start_x"`: `AttributeValue::Unsigned(...)`

### 2.2 `Variable` Attributes (`VariableGridLayout`) & Capability Flags
- `"wx1:topology_capability_version"`: `AttributeValue::Unsigned(1)` (recorded on both `GridMapping` and `Variable`).
- `"wx1:stagger_horizontal"`: `"center" | "edge_u" | "edge_v" | "corner"`
- `"wx1:stagger_vertical"`: `"full_level" | "half_level"`
- `"wx1:halo_south"`, `"wx1:halo_north"`, `"wx1:halo_west"`, `"wx1:halo_east"`: `AttributeValue::Unsigned(...)`
- `"wx1:halo_consistency"`: `"unverified" | "exchanged_consistent" | "stale_or_unfilled"`
- `"wx1:tile_dimension"`: `AttributeValue::Text("tile")` (optional name of the panel/tile axis, length 6 for `CubedSphere` or 2 for `YinYang`)

### 2.3 Halo Consistency Verification and Backward Compatibility for Older Readers
1. **Halo Consistency (`HaloConsistencyState` & `verify_cubed_sphere_edge_halos`):**
   - Writers explicitly declare whether stored halo cells are synchronized via `"wx1:halo_consistency"`:
     - `"exchanged_consistent"`: Halo zones were populated by the NWP model's halo-exchange routine prior to I/O and match the interior boundary cells of neighboring panels.
     - `"stale_or_unfilled"`: Halo zones contain unexchanged scratch/sentinel values; readers must strip halos via `read_tile_slice(..., include_halos=false)` or `compute_interior_selection` before cross-panel operations.
     - `"unverified"`: Default when halo state has not been explicitly asserted.
   - `ScientificSnapshot::verify_cubed_sphere_edge_halos(reader, variable_path)` verifies bit-for-bit across all 24 directed panel edges (`map_edge_halo_index`) whether the stored halo cells match the adjacent interior cells of the neighboring tiles.
2. **Capability Flag & Behavior of Older Readers:**
   - Because topology and halo descriptors are encoded in `GridMapping::attributes` and `Variable::attributes` (with `"wx1:topology_capability_version" = 1`) rather than altering the container header `flags` or CBOR schema v1 array arity, **older schema-v1 readers can still open the container and read the full allocated chunk arrays** (`N + 2H`) without crashing, while preserving all `"wx1:*"` attributes intact per the unknown-attribute preservation rule.
   - Topology-aware readers inspect `"wx1:topology_capability_version"` and `"wx1:halo_*"` to automatically strip halos (`include_halos=false`) or verify cross-panel halo consistency.

---

## 3. Tile & Interior Selection Helpers (`R9` & `W5` Workloads)

`ScientificSnapshot` and `ScientificDataset` gain dedicated topology-aware planning and reading methods:
- `plan_tile_slice(variable_path, tile_index, include_halos)` -> `Result<ArrayReadPlan, ScientificModelError>`
- `read_tile_slice(reader, variable_path, tile_index, include_halos)` -> `Result<ScientificArray, ChunkIoError>`
- `compute_interior_selection(variable_path)` -> `Result<ArraySelection, ScientificModelError>`: strips any declared halo zones (`halo_south`, `halo_north`, `halo_west`, `halo_east`) along the horizontal axes and returns the exact interior compute domain selection.
- `validate_arakawa_c_consistency(&self, center_var, u_var, v_var, w_var)`: verifies that staggered variables on the same grid obey exact Arakawa C-grid dimension relationships (accounting for each variable's declared halo widths).
