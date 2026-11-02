//! Non-regular grid topologies, cubed-sphere connectivity, Arakawa C-grid staggering,
//! halo zones, octahedral reduced Gaussian rings, and domain nesting (ADR 0026).
//!
//! All structures serialize deterministically into `GridMapping::attributes` and
//! `Variable::attributes` without altering the underlying WX-1 container or CBOR schema v1 wire format.

use std::collections::BTreeMap;
use std::io::{Read, Seek};

use crate::scientific::{
    ArrayReadPlan, ArraySelection, AttributeValue, GridMapping, ScientificArray, ScientificDataset,
    ScientificModelError, ScientificSnapshot, Variable,
};
use crate::ChunkIoError;

/// Supported horizontal grid families in WX-1 (ADR 0026).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridFamily {
    RegularLatLon,
    Gaussian,
    ReducedGaussian,
    LambertConformal,
    CubedSphere,
    YinYang,
    Icosahedral,
}

impl GridFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RegularLatLon => "regular_latlon",
            Self::Gaussian => "gaussian",
            Self::ReducedGaussian => "reduced_gaussian",
            Self::LambertConformal => "lambert_conformal",
            Self::CubedSphere => "cubed_sphere",
            Self::YinYang => "yin_yang",
            Self::Icosahedral => "icosahedral",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ScientificModelError> {
        match value {
            "regular_latlon" => Ok(Self::RegularLatLon),
            "gaussian" => Ok(Self::Gaussian),
            "reduced_gaussian" => Ok(Self::ReducedGaussian),
            "lambert_conformal" => Ok(Self::LambertConformal),
            "cubed_sphere" => Ok(Self::CubedSphere),
            "yin_yang" => Ok(Self::YinYang),
            "icosahedral" => Ok(Self::Icosahedral),
            _ => Err(ScientificModelError::InvalidGridMapping),
        }
    }
}

/// Gnomonic projection variant on each cubed-sphere panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GnomonicProjection {
    Equiangular,
    Equidistant,
}

impl GnomonicProjection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Equiangular => "equiangular",
            Self::Equidistant => "equidistant",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ScientificModelError> {
        match value {
            "equiangular" => Ok(Self::Equiangular),
            "equidistant" => Ok(Self::Equidistant),
            _ => Err(ScientificModelError::InvalidGridMapping),
        }
    }
}

/// Directed boundary edge of a 2D panel (`West`, `East`, `South`, `North`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PanelEdge {
    West = 0,
    East = 1,
    South = 2,
    North = 3,
}

impl PanelEdge {
    pub fn from_code(code: u8) -> Result<Self, ScientificModelError> {
        match code {
            0 => Ok(Self::West),
            1 => Ok(Self::East),
            2 => Ok(Self::South),
            3 => Ok(Self::North),
            _ => Err(ScientificModelError::InvalidGridMapping),
        }
    }
}

/// Connection from one panel edge to a neighboring panel edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeNeighbor {
    /// Target panel/tile index (`0..6` for cubed-sphere).
    pub target_tile: u8,
    /// Target boundary edge on `target_tile`.
    pub target_edge: PanelEdge,
    /// True if the tangential coordinate along the shared edge runs in opposite index order.
    pub reverse_index: bool,
}

/// Four-edge neighbor specification for one cubed-sphere panel (`tile = 0..5`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelConnectivity {
    pub west: EdgeNeighbor,
    pub east: EdgeNeighbor,
    pub south: EdgeNeighbor,
    pub north: EdgeNeighbor,
}

impl PanelConnectivity {
    pub fn edge(&self, edge: PanelEdge) -> EdgeNeighbor {
        match edge {
            PanelEdge::West => self.west,
            PanelEdge::East => self.east,
            PanelEdge::South => self.south,
            PanelEdge::North => self.north,
        }
    }
}

/// Complete 6-panel cubed-sphere topology (`C{N}`, e.g. `C96`, `C192`, `C384`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CubedSphereTopology {
    /// Number of interior grid cells per panel edge ($N \ge 1$, e.g. `96`, `192`, `384`).
    pub c_resolution: u64,
    /// Gnomonic panel projection (`Equiangular` or `Equidistant`).
    pub projection: GnomonicProjection,
    /// Explicit 4-edge neighbor connectivity for each of the 6 panels (`0..6`).
    pub panels: [PanelConnectivity; 6],
}

impl CubedSphereTopology {
    /// Constructs a standard FV3/Tempest 6-tile cubed-sphere topology with resolution `C{c_resolution}`.
    ///
    /// Panels `0, 1, 2, 3` form the equatorial belt; panel `4` is the North Pole cap and panel `5` is the South Pole cap.
    /// Every directed edge connection is an exact involution: if panel `A` edge `E1` connects to
    /// panel `B` edge `E2` with `reverse_index = r`, then panel `B` edge `E2` connects back to
    /// panel `A` edge `E1` with the same `reverse_index = r`.
    pub fn standard(
        c_resolution: u64,
        projection: GnomonicProjection,
    ) -> Result<Self, ScientificModelError> {
        if c_resolution == 0 || c_resolution > 1_000_000 {
            return Err(ScientificModelError::InvalidGridMapping);
        }
        use PanelEdge::{East, North, South, West};
        let panels = [
            // Panel 0 (Equatorial 0)
            PanelConnectivity {
                west: EdgeNeighbor {
                    target_tile: 3,
                    target_edge: East,
                    reverse_index: false,
                },
                east: EdgeNeighbor {
                    target_tile: 1,
                    target_edge: West,
                    reverse_index: false,
                },
                north: EdgeNeighbor {
                    target_tile: 4,
                    target_edge: South,
                    reverse_index: false,
                },
                south: EdgeNeighbor {
                    target_tile: 5,
                    target_edge: North,
                    reverse_index: false,
                },
            },
            // Panel 1 (Equatorial 90E)
            PanelConnectivity {
                west: EdgeNeighbor {
                    target_tile: 0,
                    target_edge: East,
                    reverse_index: false,
                },
                east: EdgeNeighbor {
                    target_tile: 2,
                    target_edge: West,
                    reverse_index: false,
                },
                north: EdgeNeighbor {
                    target_tile: 4,
                    target_edge: East,
                    reverse_index: false,
                },
                south: EdgeNeighbor {
                    target_tile: 5,
                    target_edge: East,
                    reverse_index: true,
                },
            },
            // Panel 2 (Equatorial 180)
            PanelConnectivity {
                west: EdgeNeighbor {
                    target_tile: 1,
                    target_edge: East,
                    reverse_index: false,
                },
                east: EdgeNeighbor {
                    target_tile: 3,
                    target_edge: West,
                    reverse_index: false,
                },
                north: EdgeNeighbor {
                    target_tile: 4,
                    target_edge: North,
                    reverse_index: true,
                },
                south: EdgeNeighbor {
                    target_tile: 5,
                    target_edge: South,
                    reverse_index: true,
                },
            },
            // Panel 3 (Equatorial 90W)
            PanelConnectivity {
                west: EdgeNeighbor {
                    target_tile: 2,
                    target_edge: East,
                    reverse_index: false,
                },
                east: EdgeNeighbor {
                    target_tile: 0,
                    target_edge: West,
                    reverse_index: false,
                },
                north: EdgeNeighbor {
                    target_tile: 4,
                    target_edge: West,
                    reverse_index: true,
                },
                south: EdgeNeighbor {
                    target_tile: 5,
                    target_edge: West,
                    reverse_index: false,
                },
            },
            // Panel 4 (North polar panel)
            PanelConnectivity {
                south: EdgeNeighbor {
                    target_tile: 0,
                    target_edge: North,
                    reverse_index: false,
                },
                east: EdgeNeighbor {
                    target_tile: 1,
                    target_edge: North,
                    reverse_index: false,
                },
                north: EdgeNeighbor {
                    target_tile: 2,
                    target_edge: North,
                    reverse_index: true,
                },
                west: EdgeNeighbor {
                    target_tile: 3,
                    target_edge: North,
                    reverse_index: true,
                },
            },
            // Panel 5 (South polar panel)
            PanelConnectivity {
                north: EdgeNeighbor {
                    target_tile: 0,
                    target_edge: South,
                    reverse_index: false,
                },
                east: EdgeNeighbor {
                    target_tile: 1,
                    target_edge: South,
                    reverse_index: true,
                },
                south: EdgeNeighbor {
                    target_tile: 2,
                    target_edge: South,
                    reverse_index: true,
                },
                west: EdgeNeighbor {
                    target_tile: 3,
                    target_edge: South,
                    reverse_index: false,
                },
            },
        ];
        let topo = Self {
            c_resolution,
            projection,
            panels,
        };
        topo.validate()?;
        Ok(topo)
    }

    /// Validates that `c_resolution >= 1` and that all 24 directed panel edges form a valid,
    /// self-consistent involution with no self-loops or duplicate target edges.
    pub fn validate(&self) -> Result<(), ScientificModelError> {
        if self.c_resolution == 0 || self.c_resolution > 1_000_000 {
            return Err(ScientificModelError::InvalidGridMapping);
        }
        let edges = [
            PanelEdge::West,
            PanelEdge::East,
            PanelEdge::South,
            PanelEdge::North,
        ];
        for (tile_idx, panel) in self.panels.iter().enumerate() {
            for &edge in &edges {
                let nbr = panel.edge(edge);
                if nbr.target_tile >= 6 || nbr.target_tile as usize == tile_idx {
                    return Err(ScientificModelError::InvalidGridMapping);
                }
                let reciprocal = self.panels[nbr.target_tile as usize].edge(nbr.target_edge);
                if reciprocal.target_tile as usize != tile_idx
                    || reciprocal.target_edge != edge
                    || reciprocal.reverse_index != nbr.reverse_index
                {
                    return Err(ScientificModelError::InvalidGridMapping);
                }
            }
        }
        Ok(())
    }

    /// Total number of interior cell-centered grid points per vertical level across all 6 tiles ($6 \times N^2$).
    pub fn total_cells_per_level(&self) -> Result<u64, ScientificModelError> {
        self.c_resolution
            .checked_mul(self.c_resolution)
            .and_then(|n2| n2.checked_mul(6))
            .ok_or(ScientificModelError::DimensionProductOverflow)
    }

    /// Computes the 3D unit-sphere Cartesian coordinate $(X, Y, Z)$ for local equiangular
    /// gnomonic angles $(\alpha, \beta) \in [-\pi/4, \pi/4]^2$ on `tile` (`0..6`).
    ///
    /// Convention matching `CubedSphereTopology::standard`:
    /// - Tile 0 (Equatorial $0^\circ$): $+X$ face, $\xi = \tan\alpha$ East ($+Y$), $\eta = \tan\beta$ North ($+Z$)
    /// - Tile 1 (Equatorial $90^\circ$E): $+Y$ face, $\xi = \tan\alpha$ East ($-X$), $\eta = \tan\beta$ North ($+Z$)
    /// - Tile 2 (Equatorial $180^\circ$): $-X$ face, $\xi = \tan\alpha$ East ($-Y$), $\eta = \tan\beta$ North ($+Z$)
    /// - Tile 3 (Equatorial $90^\circ$W): $-Y$ face, $\xi = \tan\alpha$ East ($+X$), $\eta = \tan\beta$ North ($+Z$)
    /// - Tile 4 (North Pole cap): $+Z$ face, South edge at Tile 0 North, East edge at Tile 1 North
    /// - Tile 5 (South Pole cap): $-Z$ face, North edge at Tile 0 South, East edge at Tile 1 South (reversed)
    pub fn gnomonic_xyz(tile: u8, alpha: f64, beta: f64) -> Result<[f64; 3], ScientificModelError> {
        let xi = alpha.tan();
        let eta = beta.tan();
        let inv_r = 1.0 / (1.0 + xi * xi + eta * eta).sqrt();
        let (x, y, z) = match tile {
            0 => (1.0, xi, eta),
            1 => (-xi, 1.0, eta),
            2 => (-1.0, -xi, eta),
            3 => (xi, -1.0, eta),
            4 => (-eta, xi, 1.0),
            5 => (eta, xi, -1.0),
            _ => return Err(ScientificModelError::InvalidGridMapping),
        };
        Ok([x * inv_r, y * inv_r, z * inv_r])
    }

    /// Maps a halo cell at interior-relative indices `(y_rel, x_rel)` on `tile` across the
    /// appropriate panel edge (`West`, `East`, `South`, `North`) to the corresponding
    /// interior cell `(target_tile, target_y, target_x)` on the neighboring panel (`0..c_resolution`).
    ///
    /// Here `y_rel` and `x_rel` are relative to the interior domain `[0..N, 0..N]`:
    /// - West halo: `x_rel = -d` ($d \in [1..=H]$), `y_rel = t` ($t \in [0..N)$)
    /// - East halo: `x_rel = N - 1 + d` ($d \in [1..=H]$), `y_rel = t` ($t \in [0..N)$)
    /// - South halo: `y_rel = -d` ($d \in [1..=H]$), `x_rel = t` ($t \in [0..N)$)
    /// - North halo: `y_rel = N - 1 + d` ($d \in [1..=H]$), `x_rel = t` ($t \in [0..N)$)
    pub fn map_edge_halo_index(
        &self,
        tile: u8,
        edge: PanelEdge,
        tangential_idx: u64,
        halo_depth: u64,
    ) -> Result<(u8, u64, u64), ScientificModelError> {
        let n = self.c_resolution;
        if tile >= 6 || tangential_idx >= n || halo_depth == 0 || halo_depth > n {
            return Err(ScientificModelError::InvalidSelection);
        }
        let nbr = self.panels[tile as usize].edge(edge);
        let t_mapped = if nbr.reverse_index {
            n - 1 - tangential_idx
        } else {
            tangential_idx
        };
        let d_offset = halo_depth - 1;
        let (target_y, target_x) = match nbr.target_edge {
            PanelEdge::West => (t_mapped, d_offset),
            PanelEdge::East => (t_mapped, n - 1 - d_offset),
            PanelEdge::South => (d_offset, t_mapped),
            PanelEdge::North => (n - 1 - d_offset, t_mapped),
        };
        Ok((nbr.target_tile, target_y, target_x))
    }

    fn encode_connectivity_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(6 * 4 * 3);
        let edges = [
            PanelEdge::West,
            PanelEdge::East,
            PanelEdge::South,
            PanelEdge::North,
        ];
        for panel in &self.panels {
            for &e in &edges {
                let nbr = panel.edge(e);
                out.push(nbr.target_tile);
                out.push(nbr.target_edge as u8);
                out.push(u8::from(nbr.reverse_index));
            }
        }
        out
    }

    fn decode_connectivity_bytes(
        bytes: &[u8],
    ) -> Result<[PanelConnectivity; 6], ScientificModelError> {
        if bytes.len() != 6 * 4 * 3 {
            return Err(ScientificModelError::InvalidGridMapping);
        }
        let dummy_nbr = EdgeNeighbor {
            target_tile: 0,
            target_edge: PanelEdge::West,
            reverse_index: false,
        };
        let mut panels = [PanelConnectivity {
            west: dummy_nbr,
            east: dummy_nbr,
            south: dummy_nbr,
            north: dummy_nbr,
        }; 6];
        for (t, panel) in panels.iter_mut().enumerate() {
            let base = t * 12;
            let parse_nbr = |offset: usize| -> Result<EdgeNeighbor, ScientificModelError> {
                let target_tile = bytes[offset];
                let target_edge = PanelEdge::from_code(bytes[offset + 1])?;
                let reverse_index = match bytes[offset + 2] {
                    0 => false,
                    1 => true,
                    _ => return Err(ScientificModelError::InvalidGridMapping),
                };
                Ok(EdgeNeighbor {
                    target_tile,
                    target_edge,
                    reverse_index,
                })
            };
            panel.west = parse_nbr(base)?;
            panel.east = parse_nbr(base + 3)?;
            panel.south = parse_nbr(base + 6)?;
            panel.north = parse_nbr(base + 9)?;
        }
        Ok(panels)
    }
}

/// Reduced Gaussian grid topology (`O{N}` octahedral or `N{N}` classic reduced Gaussian).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReducedGaussianTopology {
    /// Gaussian number $N$ (number of latitude rings from equator to pole; total rings $= 2N$).
    pub gaussian_n: u64,
    /// True if this grid follows the ECMWF octahedral rule (`O{N}`).
    pub is_octahedral: bool,
    /// Number of longitude points on each of the $2N$ latitude rings (pole-to-pole).
    pub ring_point_counts: Vec<u32>,
}

impl ReducedGaussianTopology {
    /// Constructs an ECMWF octahedral reduced Gaussian topology `O{gaussian_n}` ($2N$ rings,
    /// where ring $i \in [1..=N]$ from either pole has $4i + 16$ points).
    pub fn octahedral(gaussian_n: u64) -> Result<Self, ScientificModelError> {
        if gaussian_n == 0 || gaussian_n > 32_768 {
            return Err(ScientificModelError::InvalidGridMapping);
        }
        let n_usize =
            usize::try_from(gaussian_n).map_err(|_| ScientificModelError::InvalidGridMapping)?;
        let mut counts = Vec::with_capacity(2 * n_usize);
        for i in 1..=n_usize {
            counts.push((4 * i + 16) as u32);
        }
        for i in (1..=n_usize).rev() {
            counts.push((4 * i + 16) as u32);
        }
        Ok(Self {
            gaussian_n,
            is_octahedral: true,
            ring_point_counts: counts,
        })
    }

    /// Total number of horizontal grid points across all $2N$ rings.
    pub fn total_points(&self) -> u64 {
        self.ring_point_counts.iter().map(|&c| u64::from(c)).sum()
    }

    pub fn validate(&self) -> Result<(), ScientificModelError> {
        if self.gaussian_n == 0 || self.gaussian_n > 32_768 {
            return Err(ScientificModelError::InvalidGridMapping);
        }
        if self.ring_point_counts.len() as u64 != 2 * self.gaussian_n {
            return Err(ScientificModelError::InvalidGridMapping);
        }
        if self.ring_point_counts.contains(&0) {
            return Err(ScientificModelError::InvalidGridMapping);
        }
        // Verify North-South symmetry
        let len = self.ring_point_counts.len();
        for i in 0..len / 2 {
            if self.ring_point_counts[i] != self.ring_point_counts[len - 1 - i] {
                return Err(ScientificModelError::InvalidGridMapping);
            }
        }
        Ok(())
    }
}

/// Parent-child domain nesting metadata for regional or telescopic NWP grids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestingDescriptor {
    pub domain_id: String,
    pub parent_domain_id: Option<String>,
    pub refinement_ratio_y: u64,
    pub refinement_ratio_x: u64,
    pub parent_start_y: u64,
    pub parent_start_x: u64,
    pub two_way_feedback: bool,
}

impl NestingDescriptor {
    pub fn validate(&self) -> Result<(), ScientificModelError> {
        if self.domain_id.is_empty() || self.refinement_ratio_y == 0 || self.refinement_ratio_x == 0
        {
            return Err(ScientificModelError::InvalidGridMapping);
        }
        if self
            .parent_domain_id
            .as_ref()
            .is_some_and(|p| p.is_empty() || p == &self.domain_id)
        {
            return Err(ScientificModelError::InvalidGridMapping);
        }
        Ok(())
    }
}

/// High-level grid topology descriptor mapped into a `GridMapping` object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridTopologyDescriptor {
    pub family: GridFamily,
    pub cubed_sphere: Option<CubedSphereTopology>,
    pub reduced_gaussian: Option<ReducedGaussianTopology>,
    pub nesting: Option<NestingDescriptor>,
}

impl GridTopologyDescriptor {
    pub fn validate(&self) -> Result<(), ScientificModelError> {
        match self.family {
            GridFamily::CubedSphere => {
                let cs = self
                    .cubed_sphere
                    .as_ref()
                    .ok_or(ScientificModelError::InvalidGridMapping)?;
                cs.validate()?;
                if self.reduced_gaussian.is_some() {
                    return Err(ScientificModelError::InvalidGridMapping);
                }
            }
            GridFamily::ReducedGaussian => {
                let rg = self
                    .reduced_gaussian
                    .as_ref()
                    .ok_or(ScientificModelError::InvalidGridMapping)?;
                rg.validate()?;
                if self.cubed_sphere.is_some() {
                    return Err(ScientificModelError::InvalidGridMapping);
                }
            }
            _ => {
                if self.cubed_sphere.is_some() || self.reduced_gaussian.is_some() {
                    return Err(ScientificModelError::InvalidGridMapping);
                }
            }
        }
        if let Some(ref nest) = self.nesting {
            nest.validate()?;
        }
        Ok(())
    }

    /// Serializes this topology descriptor into a canonical `GridMapping` record.
    pub fn to_grid_mapping(
        &self,
        name: impl Into<String>,
        axis_order: Vec<String>,
    ) -> Result<GridMapping, ScientificModelError> {
        self.validate()?;
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "wx1:topology_capability_version".to_owned(),
            AttributeValue::Unsigned(1),
        );
        attrs.insert(
            "wx1:grid_family".to_owned(),
            AttributeValue::Text(self.family.as_str().to_owned()),
        );
        if let Some(ref cs) = self.cubed_sphere {
            attrs.insert("wx1:cs_num_tiles".to_owned(), AttributeValue::Unsigned(6));
            attrs.insert(
                "wx1:cs_resolution".to_owned(),
                AttributeValue::Unsigned(cs.c_resolution),
            );
            attrs.insert(
                "wx1:cs_gnomonic_type".to_owned(),
                AttributeValue::Text(cs.projection.as_str().to_owned()),
            );
            attrs.insert(
                "wx1:cs_connectivity".to_owned(),
                AttributeValue::Bytes(cs.encode_connectivity_bytes()),
            );
        }
        if let Some(ref rg) = self.reduced_gaussian {
            attrs.insert(
                "wx1:gaussian_n".to_owned(),
                AttributeValue::Unsigned(rg.gaussian_n),
            );
            attrs.insert(
                "wx1:is_octahedral".to_owned(),
                AttributeValue::Boolean(rg.is_octahedral),
            );
            let mut ring_bytes = Vec::with_capacity(rg.ring_point_counts.len() * 4);
            for &c in &rg.ring_point_counts {
                ring_bytes.extend_from_slice(&c.to_le_bytes());
            }
            attrs.insert(
                "wx1:ring_point_counts".to_owned(),
                AttributeValue::Bytes(ring_bytes),
            );
        }
        if let Some(ref nest) = self.nesting {
            attrs.insert(
                "wx1:domain_id".to_owned(),
                AttributeValue::Text(nest.domain_id.clone()),
            );
            if let Some(ref parent) = nest.parent_domain_id {
                attrs.insert(
                    "wx1:parent_domain_id".to_owned(),
                    AttributeValue::Text(parent.clone()),
                );
            }
            attrs.insert(
                "wx1:nest_ratio_y".to_owned(),
                AttributeValue::Unsigned(nest.refinement_ratio_y),
            );
            attrs.insert(
                "wx1:nest_ratio_x".to_owned(),
                AttributeValue::Unsigned(nest.refinement_ratio_x),
            );
            attrs.insert(
                "wx1:parent_start_y".to_owned(),
                AttributeValue::Unsigned(nest.parent_start_y),
            );
            attrs.insert(
                "wx1:parent_start_x".to_owned(),
                AttributeValue::Unsigned(nest.parent_start_x),
            );
            attrs.insert(
                "wx1:two_way_feedback".to_owned(),
                AttributeValue::Boolean(nest.two_way_feedback),
            );
        }
        Ok(GridMapping {
            name: name.into(),
            mapping_name: self.family.as_str().to_owned(),
            crs_wkt: Some(format!("WX1_GRID:{}", self.family.as_str())),
            epsg_code: None,
            axis_order,
            attributes: attrs,
        })
    }

    /// Parses and validates a `GridTopologyDescriptor` from a `GridMapping` if `"wx1:grid_family"` is present.
    pub fn from_grid_mapping(mapping: &GridMapping) -> Result<Option<Self>, ScientificModelError> {
        let Some(AttributeValue::Text(family_str)) = mapping.attributes.get("wx1:grid_family")
        else {
            return Ok(None);
        };
        let family = GridFamily::parse(family_str)?;
        let cubed_sphere = if family == GridFamily::CubedSphere {
            let num_tiles = match mapping.attributes.get("wx1:cs_num_tiles") {
                Some(AttributeValue::Unsigned(v)) => *v,
                _ => return Err(ScientificModelError::InvalidGridMapping),
            };
            if num_tiles != 6 {
                return Err(ScientificModelError::InvalidGridMapping);
            }
            let c_resolution = match mapping.attributes.get("wx1:cs_resolution") {
                Some(AttributeValue::Unsigned(v)) => *v,
                _ => return Err(ScientificModelError::InvalidGridMapping),
            };
            let proj_str = match mapping.attributes.get("wx1:cs_gnomonic_type") {
                Some(AttributeValue::Text(s)) => s.as_str(),
                _ => return Err(ScientificModelError::InvalidGridMapping),
            };
            let projection = GnomonicProjection::parse(proj_str)?;
            let conn_bytes = match mapping.attributes.get("wx1:cs_connectivity") {
                Some(AttributeValue::Bytes(b)) => b.as_slice(),
                _ => return Err(ScientificModelError::InvalidGridMapping),
            };
            let panels = CubedSphereTopology::decode_connectivity_bytes(conn_bytes)?;
            let cs = CubedSphereTopology {
                c_resolution,
                projection,
                panels,
            };
            cs.validate()?;
            Some(cs)
        } else {
            None
        };

        let reduced_gaussian = if family == GridFamily::ReducedGaussian {
            let gaussian_n = match mapping.attributes.get("wx1:gaussian_n") {
                Some(AttributeValue::Unsigned(v)) => *v,
                _ => return Err(ScientificModelError::InvalidGridMapping),
            };
            let is_octahedral = match mapping.attributes.get("wx1:is_octahedral") {
                Some(AttributeValue::Boolean(b)) => *b,
                _ => return Err(ScientificModelError::InvalidGridMapping),
            };
            let ring_bytes = match mapping.attributes.get("wx1:ring_point_counts") {
                Some(AttributeValue::Bytes(b)) => b.as_slice(),
                _ => return Err(ScientificModelError::InvalidGridMapping),
            };
            if ring_bytes.len() % 4 != 0 {
                return Err(ScientificModelError::InvalidGridMapping);
            }
            let mut counts = Vec::with_capacity(ring_bytes.len() / 4);
            for chunk in ring_bytes.chunks_exact(4) {
                counts.push(u32::from_le_bytes(chunk.try_into().unwrap()));
            }
            let rg = ReducedGaussianTopology {
                gaussian_n,
                is_octahedral,
                ring_point_counts: counts,
            };
            rg.validate()?;
            Some(rg)
        } else {
            None
        };

        let nesting = if let Some(AttributeValue::Text(domain_id)) =
            mapping.attributes.get("wx1:domain_id")
        {
            let parent_domain_id = match mapping.attributes.get("wx1:parent_domain_id") {
                Some(AttributeValue::Text(s)) => Some(s.clone()),
                None => None,
                _ => return Err(ScientificModelError::InvalidGridMapping),
            };
            let get_u64 = |key: &str| -> Result<u64, ScientificModelError> {
                match mapping.attributes.get(key) {
                    Some(AttributeValue::Unsigned(v)) => Ok(*v),
                    _ => Err(ScientificModelError::InvalidGridMapping),
                }
            };
            let refinement_ratio_y = get_u64("wx1:nest_ratio_y")?;
            let refinement_ratio_x = get_u64("wx1:nest_ratio_x")?;
            let parent_start_y = get_u64("wx1:parent_start_y")?;
            let parent_start_x = get_u64("wx1:parent_start_x")?;
            let two_way_feedback = match mapping.attributes.get("wx1:two_way_feedback") {
                Some(AttributeValue::Boolean(b)) => *b,
                _ => false,
            };
            let nest = NestingDescriptor {
                domain_id: domain_id.clone(),
                parent_domain_id,
                refinement_ratio_y,
                refinement_ratio_x,
                parent_start_y,
                parent_start_x,
                two_way_feedback,
            };
            nest.validate()?;
            Some(nest)
        } else {
            None
        };

        let desc = Self {
            family,
            cubed_sphere,
            reduced_gaussian,
            nesting,
        };
        desc.validate()?;
        Ok(Some(desc))
    }
}

/// Horizontal Arakawa C-grid staggering location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizontalStagger {
    /// Mass/thermodynamic point at cell center ($N_y \times N_x$).
    Center,
    /// Zonal/xi velocity point on vertical cell edges ($N_y \times (N_x + 1)$ or $N_y \times N_x$).
    EdgeU,
    /// Meridional/eta velocity point on horizontal cell edges ($(N_y + 1) \times N_x$ or $N_y \times N_x$).
    EdgeV,
    /// Vorticity/circulation point on cell corners ($(N_y + 1) \times (N_x + 1)$).
    Corner,
}

impl HorizontalStagger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Center => "center",
            Self::EdgeU => "edge_u",
            Self::EdgeV => "edge_v",
            Self::Corner => "corner",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ScientificModelError> {
        match value {
            "center" => Ok(Self::Center),
            "edge_u" => Ok(Self::EdgeU),
            "edge_v" => Ok(Self::EdgeV),
            "corner" => Ok(Self::Corner),
            _ => Err(ScientificModelError::InvalidVariable),
        }
    }
}

/// Vertical level staggering (Lorenz / Charney-Phillips vertical grids).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalStagger {
    /// Full model level ($N_z$ layers).
    FullLevel,
    /// Half-level / layer interface ($N_z + 1$ interfaces, e.g. vertical velocity $w$ or geopotential $\Phi$).
    HalfLevel,
}

impl VerticalStagger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FullLevel => "full_level",
            Self::HalfLevel => "half_level",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ScientificModelError> {
        match value {
            "full_level" => Ok(Self::FullLevel),
            "half_level" => Ok(Self::HalfLevel),
            _ => Err(ScientificModelError::InvalidVariable),
        }
    }
}

/// Horizontal halo widths around the compute domain of each tile or regional domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HaloSpec {
    pub south: u64,
    pub north: u64,
    pub west: u64,
    pub east: u64,
}

impl HaloSpec {
    pub const fn symmetric(width: u64) -> Self {
        Self {
            south: width,
            north: width,
            west: width,
            east: width,
        }
    }

    pub const fn is_zero(&self) -> bool {
        self.south == 0 && self.north == 0 && self.west == 0 && self.east == 0
    }
}

/// Synchronization state of horizontal halo regions stored in a variable's chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HaloConsistencyState {
    /// No halo cells are present (`halo_width == 0`), or state is unspecified.
    Unverified,
    /// Halo cells were populated by the NWP model's halo-exchange routine prior to I/O,
    /// and can be verified against neighboring panels via `verify_cubed_sphere_halos`.
    ExchangedConsistent,
    /// Halo cells contain stale/unexchanged buffer values; readers should strip halos
    /// via `compute_interior_selection` or `read_tile_slice(..., include_halos=false)` before cross-panel operations.
    StaleOrUnfilled,
}

impl HaloConsistencyState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::ExchangedConsistent => "exchanged_consistent",
            Self::StaleOrUnfilled => "stale_or_unfilled",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ScientificModelError> {
        match value {
            "unverified" => Ok(Self::Unverified),
            "exchanged_consistent" => Ok(Self::ExchangedConsistent),
            "stale_or_unfilled" => Ok(Self::StaleOrUnfilled),
            _ => Err(ScientificModelError::InvalidVariable),
        }
    }
}

/// Per-variable layout metadata for Arakawa staggering, halo widths, halo consistency, and tile dimension binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableGridLayout {
    pub horizontal_stagger: HorizontalStagger,
    pub vertical_stagger: VerticalStagger,
    pub halo: HaloSpec,
    pub halo_consistency: HaloConsistencyState,
    pub tile_dimension: Option<String>,
}

impl Default for VariableGridLayout {
    fn default() -> Self {
        Self {
            horizontal_stagger: HorizontalStagger::Center,
            vertical_stagger: VerticalStagger::FullLevel,
            halo: HaloSpec::default(),
            halo_consistency: HaloConsistencyState::Unverified,
            tile_dimension: None,
        }
    }
}

impl VariableGridLayout {
    /// Writes the layout attributes into `variable.attributes`.
    pub fn apply_to_variable(&self, variable: &mut Variable) {
        variable.attributes.insert(
            "wx1:topology_capability_version".to_owned(),
            AttributeValue::Unsigned(1),
        );
        variable.attributes.insert(
            "wx1:stagger_horizontal".to_owned(),
            AttributeValue::Text(self.horizontal_stagger.as_str().to_owned()),
        );
        variable.attributes.insert(
            "wx1:stagger_vertical".to_owned(),
            AttributeValue::Text(self.vertical_stagger.as_str().to_owned()),
        );
        variable.attributes.insert(
            "wx1:halo_south".to_owned(),
            AttributeValue::Unsigned(self.halo.south),
        );
        variable.attributes.insert(
            "wx1:halo_north".to_owned(),
            AttributeValue::Unsigned(self.halo.north),
        );
        variable.attributes.insert(
            "wx1:halo_west".to_owned(),
            AttributeValue::Unsigned(self.halo.west),
        );
        variable.attributes.insert(
            "wx1:halo_east".to_owned(),
            AttributeValue::Unsigned(self.halo.east),
        );
        variable.attributes.insert(
            "wx1:halo_consistency".to_owned(),
            AttributeValue::Text(self.halo_consistency.as_str().to_owned()),
        );
        if let Some(ref tile_dim) = self.tile_dimension {
            variable.attributes.insert(
                "wx1:tile_dimension".to_owned(),
                AttributeValue::Text(tile_dim.clone()),
            );
        }
    }

    /// Extracts `VariableGridLayout` from `variable.attributes` if any ADR 0026 attributes are present.
    pub fn from_variable(variable: &Variable) -> Result<Option<Self>, ScientificModelError> {
        let has_any = variable.attributes.contains_key("wx1:stagger_horizontal")
            || variable.attributes.contains_key("wx1:stagger_vertical")
            || variable.attributes.contains_key("wx1:halo_south")
            || variable.attributes.contains_key("wx1:halo_consistency")
            || variable.attributes.contains_key("wx1:tile_dimension");
        if !has_any {
            return Ok(None);
        }
        let horizontal_stagger = match variable.attributes.get("wx1:stagger_horizontal") {
            Some(AttributeValue::Text(s)) => HorizontalStagger::parse(s)?,
            None => HorizontalStagger::Center,
            _ => return Err(ScientificModelError::InvalidVariable),
        };
        let vertical_stagger = match variable.attributes.get("wx1:stagger_vertical") {
            Some(AttributeValue::Text(s)) => VerticalStagger::parse(s)?,
            None => VerticalStagger::FullLevel,
            _ => return Err(ScientificModelError::InvalidVariable),
        };
        let get_u64 = |key: &str| -> Result<u64, ScientificModelError> {
            match variable.attributes.get(key) {
                Some(AttributeValue::Unsigned(v)) => Ok(*v),
                None => Ok(0),
                _ => Err(ScientificModelError::InvalidVariable),
            }
        };
        let halo = HaloSpec {
            south: get_u64("wx1:halo_south")?,
            north: get_u64("wx1:halo_north")?,
            west: get_u64("wx1:halo_west")?,
            east: get_u64("wx1:halo_east")?,
        };
        let halo_consistency = match variable.attributes.get("wx1:halo_consistency") {
            Some(AttributeValue::Text(s)) => HaloConsistencyState::parse(s)?,
            None => HaloConsistencyState::Unverified,
            _ => return Err(ScientificModelError::InvalidVariable),
        };
        let tile_dimension = match variable.attributes.get("wx1:tile_dimension") {
            Some(AttributeValue::Text(s)) => Some(s.clone()),
            None => None,
            _ => return Err(ScientificModelError::InvalidVariable),
        };
        Ok(Some(Self {
            horizontal_stagger,
            vertical_stagger,
            halo,
            halo_consistency,
            tile_dimension,
        }))
    }
}

impl ScientificDataset {
    /// Validates all ADR 0026 grid topologies, cubed-sphere tile dimensions, and halo bounds.
    pub fn validate_grid_topologies(&self) -> Result<(), ScientificModelError> {
        let mut topologies = BTreeMap::new();
        for mapping in &self.grid_mappings {
            if let Some(desc) = GridTopologyDescriptor::from_grid_mapping(mapping)? {
                topologies.insert(mapping.name.as_str(), desc);
            }
        }
        let dim_map: BTreeMap<&str, u64> = self
            .dimensions
            .iter()
            .map(|d| (d.name.as_str(), d.length))
            .collect();

        for variable in &self.variables {
            let layout = VariableGridLayout::from_variable(variable)?;
            if let Some(ref l) = layout {
                if variable.dimensions.len() < 2 && !l.halo.is_zero() {
                    return Err(ScientificModelError::InvalidVariable);
                }
                if variable.dimensions.len() >= 2 {
                    let y_dim = &variable.dimensions[variable.dimensions.len() - 2];
                    let x_dim = &variable.dimensions[variable.dimensions.len() - 1];
                    let y_len = *dim_map
                        .get(y_dim.as_str())
                        .ok_or(ScientificModelError::UnknownDimension)?;
                    let x_len = *dim_map
                        .get(x_dim.as_str())
                        .ok_or(ScientificModelError::UnknownDimension)?;
                    let halo_y = l
                        .halo
                        .south
                        .checked_add(l.halo.north)
                        .ok_or(ScientificModelError::DimensionProductOverflow)?;
                    let halo_x = l
                        .halo
                        .west
                        .checked_add(l.halo.east)
                        .ok_or(ScientificModelError::DimensionProductOverflow)?;
                    if halo_y >= y_len || halo_x >= x_len {
                        return Err(ScientificModelError::InvalidVariable);
                    }
                }
                if let Some(ref tile_dim) = l.tile_dimension {
                    if !variable.dimensions.contains(tile_dim) {
                        return Err(ScientificModelError::UnknownDimension);
                    }
                }
            }

            if let Some(ref gm_name) = variable.grid_mapping {
                if let Some(topo) = topologies.get(gm_name.as_str()) {
                    if topo.family == GridFamily::CubedSphere {
                        let cs = topo.cubed_sphere.as_ref().unwrap();
                        let l = layout
                            .as_ref()
                            .ok_or(ScientificModelError::InvalidVariable)?;
                        let tile_dim_name = l
                            .tile_dimension
                            .as_ref()
                            .ok_or(ScientificModelError::InvalidVariable)?;
                        let tile_len = *dim_map
                            .get(tile_dim_name.as_str())
                            .ok_or(ScientificModelError::UnknownDimension)?;
                        if tile_len != 6 || variable.dimensions.len() < 3 {
                            return Err(ScientificModelError::InvalidVariable);
                        }
                        let y_dim = &variable.dimensions[variable.dimensions.len() - 2];
                        let x_dim = &variable.dimensions[variable.dimensions.len() - 1];
                        let y_len = *dim_map.get(y_dim.as_str()).unwrap();
                        let x_len = *dim_map.get(x_dim.as_str()).unwrap();
                        let interior_y = y_len - (l.halo.south + l.halo.north);
                        let interior_x = x_len - (l.halo.west + l.halo.east);
                        let expected_y = match l.horizontal_stagger {
                            HorizontalStagger::Center | HorizontalStagger::EdgeU => cs.c_resolution,
                            HorizontalStagger::EdgeV | HorizontalStagger::Corner => {
                                cs.c_resolution + 1
                            }
                        };
                        let expected_x = match l.horizontal_stagger {
                            HorizontalStagger::Center | HorizontalStagger::EdgeV => cs.c_resolution,
                            HorizontalStagger::EdgeU | HorizontalStagger::Corner => {
                                cs.c_resolution + 1
                            }
                        };
                        if interior_y != expected_y || interior_x != expected_x {
                            return Err(ScientificModelError::InvalidVariable);
                        }
                    } else if topo.family == GridFamily::YinYang {
                        let l = layout
                            .as_ref()
                            .ok_or(ScientificModelError::InvalidVariable)?;
                        let tile_dim_name = l
                            .tile_dimension
                            .as_ref()
                            .ok_or(ScientificModelError::InvalidVariable)?;
                        let tile_len = *dim_map
                            .get(tile_dim_name.as_str())
                            .ok_or(ScientificModelError::UnknownDimension)?;
                        if tile_len != 2 {
                            return Err(ScientificModelError::InvalidVariable);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Computes the interior (halo-stripped) `ArraySelection` for `variable_path`.
    pub fn compute_interior_selection(
        &self,
        variable_path: &str,
    ) -> Result<ArraySelection, ScientificModelError> {
        let variable = self
            .variable(variable_path)
            .ok_or(ScientificModelError::InvalidVariable)?;
        let rank = variable.dimensions.len();
        let mut start = vec![0u64; rank];
        let mut count = Vec::with_capacity(rank);
        for dname in &variable.dimensions {
            let d = self
                .dimensions
                .iter()
                .find(|dim| &dim.name == dname)
                .ok_or(ScientificModelError::UnknownDimension)?;
            count.push(d.length);
        }
        if let Some(layout) = VariableGridLayout::from_variable(variable)? {
            if rank >= 2 && !layout.halo.is_zero() {
                let y_axis = rank - 2;
                let x_axis = rank - 1;
                let halo_y = layout
                    .halo
                    .south
                    .checked_add(layout.halo.north)
                    .ok_or(ScientificModelError::DimensionProductOverflow)?;
                let halo_x = layout
                    .halo
                    .west
                    .checked_add(layout.halo.east)
                    .ok_or(ScientificModelError::DimensionProductOverflow)?;
                if halo_y >= count[y_axis] || halo_x >= count[x_axis] {
                    return Err(ScientificModelError::InvalidSelection);
                }
                start[y_axis] = layout.halo.south;
                count[y_axis] -= halo_y;
                start[x_axis] = layout.halo.west;
                count[x_axis] -= halo_x;
            }
        }
        Ok(ArraySelection { start, count })
    }

    /// Computes an `ArraySelection` for a single tile (`tile_index`) of a multi-tile variable
    /// (`CubedSphere` or `YinYang`), optionally including or stripping horizontal halo cells.
    pub fn tile_selection(
        &self,
        variable_path: &str,
        tile_index: u64,
        include_halos: bool,
    ) -> Result<ArraySelection, ScientificModelError> {
        let variable = self
            .variable(variable_path)
            .ok_or(ScientificModelError::InvalidVariable)?;
        let layout = VariableGridLayout::from_variable(variable)?
            .ok_or(ScientificModelError::InvalidVariable)?;
        let tile_dim_name = layout
            .tile_dimension
            .as_ref()
            .ok_or(ScientificModelError::InvalidVariable)?;
        let tile_axis = variable
            .dimensions
            .iter()
            .position(|d| d == tile_dim_name)
            .ok_or(ScientificModelError::UnknownDimension)?;
        let mut sel = if include_halos {
            let mut count = Vec::with_capacity(variable.dimensions.len());
            for dname in &variable.dimensions {
                let d = self
                    .dimensions
                    .iter()
                    .find(|dim| &dim.name == dname)
                    .ok_or(ScientificModelError::UnknownDimension)?;
                count.push(d.length);
            }
            ArraySelection {
                start: vec![0u64; variable.dimensions.len()],
                count,
            }
        } else {
            self.compute_interior_selection(variable_path)?
        };
        if tile_index >= sel.count[tile_axis] {
            return Err(ScientificModelError::InvalidSelection);
        }
        sel.start[tile_axis] = tile_index;
        sel.count[tile_axis] = 1;
        Ok(sel)
    }

    /// Plans a read for a single tile (`R9` workload) with or without halo zones.
    pub fn plan_tile_slice(
        &self,
        variable_path: &str,
        tile_index: u64,
        include_halos: bool,
    ) -> Result<ArrayReadPlan, ScientificModelError> {
        let sel = self.tile_selection(variable_path, tile_index, include_halos)?;
        self.plan_array_slice(variable_path, &sel)
    }
}

impl ScientificSnapshot {
    /// Plans a read for a single tile (`R9` workload) with or without halo zones.
    pub fn plan_tile_slice(
        &self,
        variable_path: &str,
        tile_index: u64,
        include_halos: bool,
    ) -> Result<ArrayReadPlan, ChunkIoError> {
        self.dataset()
            .plan_tile_slice(variable_path, tile_index, include_halos)
            .map_err(ChunkIoError::ScientificModel)
    }

    /// Reads a single tile (`R9` workload) with or without halo zones.
    pub fn read_tile_slice<R: Read + Seek>(
        &self,
        reader: &mut R,
        variable_path: &str,
        tile_index: u64,
        include_halos: bool,
    ) -> Result<ScientificArray, ChunkIoError> {
        let sel = self
            .dataset()
            .tile_selection(variable_path, tile_index, include_halos)
            .map_err(ChunkIoError::ScientificModel)?;
        self.read_array_slice(reader, variable_path, &sel)
    }

    /// Verifies that the edge halo zones of a 3D `(tile=6, y=N+2H, x=N+2H)` cell-centered
    /// cubed-sphere variable are bit-for-bit consistent with the corresponding interior cells
    /// of the neighboring panels across all 24 directed panel edges.
    pub fn verify_cubed_sphere_edge_halos<R: Read + Seek>(
        &self,
        reader: &mut R,
        variable_path: &str,
    ) -> Result<bool, ChunkIoError> {
        let variable =
            self.dataset()
                .variable(variable_path)
                .ok_or(ChunkIoError::ScientificModel(
                    ScientificModelError::InvalidVariable,
                ))?;
        let layout = VariableGridLayout::from_variable(variable)
            .map_err(ChunkIoError::ScientificModel)?
            .ok_or(ChunkIoError::ScientificModel(
                ScientificModelError::InvalidVariable,
            ))?;
        if layout.horizontal_stagger != HorizontalStagger::Center || layout.halo.is_zero() {
            return Err(ChunkIoError::ScientificModel(
                ScientificModelError::InvalidVariable,
            ));
        }
        let h = layout.halo.south;
        if layout.halo.north != h || layout.halo.west != h || layout.halo.east != h {
            return Err(ChunkIoError::ScientificModel(
                ScientificModelError::InvalidVariable,
            ));
        }
        let gm_name = variable
            .grid_mapping
            .as_ref()
            .ok_or(ChunkIoError::ScientificModel(
                ScientificModelError::UnknownGridMapping,
            ))?;
        let gm = self
            .dataset()
            .grid_mappings
            .iter()
            .find(|g| &g.name == gm_name)
            .ok_or(ChunkIoError::ScientificModel(
                ScientificModelError::UnknownGridMapping,
            ))?;
        let topo = GridTopologyDescriptor::from_grid_mapping(gm)
            .map_err(ChunkIoError::ScientificModel)?
            .ok_or(ChunkIoError::ScientificModel(
                ScientificModelError::InvalidGridMapping,
            ))?;
        let cs = topo.cubed_sphere.ok_or(ChunkIoError::ScientificModel(
            ScientificModelError::InvalidGridMapping,
        ))?;
        let n = cs.c_resolution;
        let ny = n + 2 * h;
        let nx = n + 2 * h;
        let elem_size = variable.data_type.element_size() as usize;

        let mut tiles = Vec::with_capacity(6);
        for t in 0..6u64 {
            let arr = self.read_tile_slice(reader, variable_path, t, true)?;
            if arr.shape != vec![1, ny, nx] {
                return Err(ChunkIoError::ScientificModel(
                    ScientificModelError::InvalidSelection,
                ));
            }
            tiles.push(arr.data);
        }

        let get_cell = |t: usize, y: u64, x: u64| -> &[u8] {
            let idx = ((y * nx + x) as usize) * elem_size;
            &tiles[t][idx..idx + elem_size]
        };

        let edges = [
            PanelEdge::West,
            PanelEdge::East,
            PanelEdge::South,
            PanelEdge::North,
        ];
        for tile in 0..6u8 {
            for &edge in &edges {
                for t_idx in 0..n {
                    for d in 1..=h {
                        let (halo_y, halo_x) = match edge {
                            PanelEdge::West => (h + t_idx, h - d),
                            PanelEdge::East => (h + t_idx, h + n - 1 + d),
                            PanelEdge::South => (h - d, h + t_idx),
                            PanelEdge::North => (h + n - 1 + d, h + t_idx),
                        };
                        let (nbr_tile, nbr_int_y, nbr_int_x) = cs
                            .map_edge_halo_index(tile, edge, t_idx, d)
                            .map_err(ChunkIoError::ScientificModel)?;
                        let expected = get_cell(nbr_tile as usize, h + nbr_int_y, h + nbr_int_x);
                        let actual = get_cell(tile as usize, halo_y, halo_x);
                        if actual != expected {
                            return Ok(false);
                        }
                    }
                }
            }
        }
        Ok(true)
    }
}
