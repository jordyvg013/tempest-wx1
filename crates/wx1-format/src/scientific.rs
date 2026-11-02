//! Experimental WX-1 scientific metadata model and multidimensional array access.
//!
//! The model is stored as a deterministic CBOR byte string in metadata schema v2.
//! Array payloads remain ordinary WX-1 chunks; this module only describes their
//! scientific meaning and maps them to multidimensional regions.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{Read, Seek};

use super::{
    canonical_text_cmp, cbor_head, CborDecoder, ChunkDescriptor, ChunkIoError, ChunkReaderInput,
    EncodedChunkInput, GenerationSnapshot, MAX_CHUNKS, MAX_CHUNK_BYTES, MAX_METADATA_BYTES,
};

pub const SCIENTIFIC_SCHEMA_VERSION: u64 = 1;
pub const MAX_SCIENTIFIC_RANK: usize = 64;
pub const MAX_SCIENTIFIC_GROUPS: usize = 4_096;
pub const MAX_SCIENTIFIC_DIMENSIONS: usize = 4_096;
pub const MAX_SCIENTIFIC_VARIABLES: usize = 100_000;
pub const MAX_SCIENTIFIC_COORDINATES: usize = 100_000;
pub const MAX_SCIENTIFIC_GRID_MAPPINGS: usize = 4_096;
pub const MAX_SCIENTIFIC_PROVENANCE: usize = 4_096;
pub const MAX_ARRAY_READ_BYTES: u64 = MAX_CHUNK_BYTES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F32,
    F64,
    Bool8,
}
impl DataType {
    pub const fn element_size(self) -> u64 {
        match self {
            Self::U8 | Self::I8 | Self::Bool8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 | Self::F32 => 4,
            Self::U64 | Self::I64 | Self::F64 => 8,
        }
    }
    fn code(self) -> u64 {
        match self {
            Self::U8 => 0,
            Self::I8 => 1,
            Self::U16 => 2,
            Self::I16 => 3,
            Self::U32 => 4,
            Self::I32 => 5,
            Self::U64 => 6,
            Self::I64 => 7,
            Self::F32 => 8,
            Self::F64 => 9,
            Self::Bool8 => 10,
        }
    }
    fn from_code(code: u64) -> Result<Self, ScientificModelError> {
        match code {
            0 => Ok(Self::U8),
            1 => Ok(Self::I8),
            2 => Ok(Self::U16),
            3 => Ok(Self::I16),
            4 => Ok(Self::U32),
            5 => Ok(Self::I32),
            6 => Ok(Self::U64),
            7 => Ok(Self::I64),
            8 => Ok(Self::F32),
            9 => Ok(Self::F64),
            10 => Ok(Self::Bool8),
            _ => Err(ScientificModelError::InvalidEncoding),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadByteOrder {
    LittleEndian,
    BigEndian,
}
impl PayloadByteOrder {
    fn code(self) -> u64 {
        match self {
            Self::LittleEndian => 0,
            Self::BigEndian => 1,
        }
    }
    fn from_code(code: u64) -> Result<Self, ScientificModelError> {
        match code {
            0 => Ok(Self::LittleEndian),
            1 => Ok(Self::BigEndian),
            _ => Err(ScientificModelError::InvalidEncoding),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimensionRole {
    Other,
    Time,
    ForecastReferenceTime,
    Ensemble,
    Vertical,
    Latitude,
    Longitude,
    Height,
    SoilLayer,
}
impl DimensionRole {
    fn code(self) -> u64 {
        match self {
            Self::Other => 0,
            Self::Time => 1,
            Self::ForecastReferenceTime => 2,
            Self::Ensemble => 3,
            Self::Vertical => 4,
            Self::Latitude => 5,
            Self::Longitude => 6,
            Self::Height => 7,
            Self::SoilLayer => 8,
        }
    }
    fn from_code(code: u64) -> Result<Self, ScientificModelError> {
        match code {
            0 => Ok(Self::Other),
            1 => Ok(Self::Time),
            2 => Ok(Self::ForecastReferenceTime),
            3 => Ok(Self::Ensemble),
            4 => Ok(Self::Vertical),
            5 => Ok(Self::Latitude),
            6 => Ok(Self::Longitude),
            7 => Ok(Self::Height),
            8 => Ok(Self::SoilLayer),
            _ => Err(ScientificModelError::InvalidEncoding),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeValue {
    Text(String),
    Integer(i64),
    Unsigned(u64),
    Float64Bits(u64),
    Boolean(bool),
    Bytes(Vec<u8>),
}

/// Exact typed scalar representation; floating-point variants retain their original IEEE bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarValue {
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    U64(u64),
    I64(i64),
    F32Bits(u32),
    F64Bits(u64),
    Bool(bool),
}
impl ScalarValue {
    fn matches(self, data_type: DataType) -> bool {
        matches!(
            (self, data_type),
            (Self::U8(_), DataType::U8)
                | (Self::I8(_), DataType::I8)
                | (Self::U16(_), DataType::U16)
                | (Self::I16(_), DataType::I16)
                | (Self::U32(_), DataType::U32)
                | (Self::I32(_), DataType::I32)
                | (Self::U64(_), DataType::U64)
                | (Self::I64(_), DataType::I64)
                | (Self::F32Bits(_), DataType::F32)
                | (Self::F64Bits(_), DataType::F64)
                | (Self::Bool(_), DataType::Bool8)
        )
    }
    fn code(self) -> u64 {
        match self {
            Self::U8(_) => 0,
            Self::I8(_) => 1,
            Self::U16(_) => 2,
            Self::I16(_) => 3,
            Self::U32(_) => 4,
            Self::I32(_) => 5,
            Self::U64(_) => 6,
            Self::I64(_) => 7,
            Self::F32Bits(_) => 8,
            Self::F64Bits(_) => 9,
            Self::Bool(_) => 10,
        }
    }
    fn payload(self) -> u64 {
        match self {
            Self::U8(v) => v as u64,
            Self::I8(v) => v as i64 as u64,
            Self::U16(v) => v as u64,
            Self::I16(v) => v as i64 as u64,
            Self::U32(v) => v as u64,
            Self::I32(v) => v as i64 as u64,
            Self::U64(v) => v,
            Self::I64(v) => v as u64,
            Self::F32Bits(v) => v as u64,
            Self::F64Bits(v) => v,
            Self::Bool(v) => {
                if v {
                    1
                } else {
                    0
                }
            }
        }
    }
    fn from_parts(code: u64, value: u64) -> Result<Self, ScientificModelError> {
        Ok(match code {
            0 => {
                Self::U8(u8::try_from(value).map_err(|_| ScientificModelError::InvalidScalarType)?)
            }
            1 => Self::I8(
                i8::try_from(value as i64).map_err(|_| ScientificModelError::InvalidScalarType)?,
            ),
            2 => Self::U16(
                u16::try_from(value).map_err(|_| ScientificModelError::InvalidScalarType)?,
            ),
            3 => Self::I16(
                i16::try_from(value as i64).map_err(|_| ScientificModelError::InvalidScalarType)?,
            ),
            4 => Self::U32(
                u32::try_from(value).map_err(|_| ScientificModelError::InvalidScalarType)?,
            ),
            5 => Self::I32(
                i32::try_from(value as i64).map_err(|_| ScientificModelError::InvalidScalarType)?,
            ),
            6 => Self::U64(value),
            7 => Self::I64(value as i64),
            8 => Self::F32Bits(
                u32::try_from(value).map_err(|_| ScientificModelError::InvalidScalarType)?,
            ),
            9 => Self::F64Bits(value),
            10 => Self::Bool(match value {
                0 => false,
                1 => true,
                _ => return Err(ScientificModelError::InvalidScalarType),
            }),
            _ => return Err(ScientificModelError::InvalidScalarType),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub path: String,
    pub attributes: BTreeMap<String, AttributeValue>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dimension {
    pub name: String,
    pub length: u64,
    pub role: DimensionRole,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayChunkRef {
    pub chunk_id: u64,
    pub origin: Vec<u64>,
    pub shape: Vec<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    pub name: String,
    /// Absolute group path (`/` for the root group).
    pub group: String,
    pub data_type: DataType,
    pub byte_order: PayloadByteOrder,
    pub dimensions: Vec<String>,
    /// Nominal chunk length for each dimension; edge chunks are clipped to the dimension length.
    pub chunk_shape: Vec<u64>,
    pub fill_value: Option<ScalarValue>,
    pub missing_value: Option<ScalarValue>,
    /// Includes conventional text keys such as `units`, `standard_name` and `long_name`.
    pub attributes: BTreeMap<String, AttributeValue>,
    pub chunks: Vec<ArrayChunkRef>,
    pub grid_mapping: Option<String>,
}
impl Variable {
    pub fn path(&self) -> String {
        if self.group == "/" {
            format!("/{}", self.name)
        } else {
            format!("{}/{}", self.group, self.name)
        }
    }

    /// Returns the variable's declared physical unit string, if present in attributes.
    pub fn units(&self) -> Option<&str> {
        self.attributes.get("units").and_then(|attr| match attr {
            AttributeValue::Text(t) => Some(t.as_str()),
            _ => None,
        })
    }

    /// Returns the CF `standard_name` if present in attributes.
    pub fn standard_name(&self) -> Option<&str> {
        self.attributes
            .get("standard_name")
            .and_then(|attr| match attr {
                AttributeValue::Text(t) => Some(t.as_str()),
                _ => None,
            })
    }

    /// Returns the canonical parameter ID (`wx1:canonical_id`), or falls back to name.
    pub fn canonical_id(&self) -> &str {
        if let Some(AttributeValue::Text(t)) = self.attributes.get("wx1:canonical_id") {
            t.as_str()
        } else if let Some(sn) = self.standard_name() {
            sn
        } else {
            self.name.as_str()
        }
    }

    /// Sets canonical registry parameter attributes on this variable.
    pub fn set_registry_parameter(
        &mut self,
        canonical_id: &str,
        standard_name: Option<&str>,
        units: &str,
        table_identity: Option<(&str, &str, &str)>,
    ) {
        self.attributes.insert(
            "wx1:canonical_id".to_owned(),
            AttributeValue::Text(canonical_id.to_owned()),
        );
        self.attributes
            .insert("units".to_owned(), AttributeValue::Text(units.to_owned()));
        if let Some(sn) = standard_name {
            self.attributes.insert(
                "standard_name".to_owned(),
                AttributeValue::Text(sn.to_owned()),
            );
        }
        if let Some((ns, tbl, ver)) = table_identity {
            self.attributes.insert(
                "wx1:registry_namespace".to_owned(),
                AttributeValue::Text(ns.to_owned()),
            );
            self.attributes.insert(
                "wx1:registry_table".to_owned(),
                AttributeValue::Text(tbl.to_owned()),
            );
            self.attributes.insert(
                "wx1:registry_version".to_owned(),
                AttributeValue::Text(ver.to_owned()),
            );
        }
    }
    pub fn element_count(&self, dataset: &ScientificDataset) -> Result<u64, ScientificModelError> {
        let dimensions: Vec<_> = self
            .dimensions
            .iter()
            .map(|name| {
                dataset
                    .dimensions
                    .iter()
                    .find(|dimension| dimension.name == *name)
                    .map(|dimension| dimension.length)
                    .ok_or(ScientificModelError::UnknownDimension)
            })
            .collect::<Result<_, _>>()?;
        if dimensions.contains(&0) {
            return Ok(0);
        }
        dimensions.into_iter().try_fold(1u64, |count, length| {
            count
                .checked_mul(length)
                .ok_or(ScientificModelError::DimensionProductOverflow)
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateAxis {
    Other,
    Time,
    ForecastReferenceTime,
    Ensemble,
    Vertical,
    Latitude,
    Longitude,
    Height,
}
impl CoordinateAxis {
    fn code(self) -> u64 {
        match self {
            Self::Other => 0,
            Self::Time => 1,
            Self::ForecastReferenceTime => 2,
            Self::Ensemble => 3,
            Self::Vertical => 4,
            Self::Latitude => 5,
            Self::Longitude => 6,
            Self::Height => 7,
        }
    }
    fn from_code(v: u64) -> Result<Self, ScientificModelError> {
        match v {
            0 => Ok(Self::Other),
            1 => Ok(Self::Time),
            2 => Ok(Self::ForecastReferenceTime),
            3 => Ok(Self::Ensemble),
            4 => Ok(Self::Vertical),
            5 => Ok(Self::Latitude),
            6 => Ok(Self::Longitude),
            7 => Ok(Self::Height),
            _ => Err(ScientificModelError::InvalidEncoding),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeCalendar {
    Gregorian,
    ProlepticGregorian,
    Julian,
    NoLeap,
    AllLeap,
    Day360,
    Utc,
}
impl TimeCalendar {
    fn code(self) -> u64 {
        match self {
            Self::Gregorian => 0,
            Self::ProlepticGregorian => 1,
            Self::Julian => 2,
            Self::NoLeap => 3,
            Self::AllLeap => 4,
            Self::Day360 => 5,
            Self::Utc => 6,
        }
    }
    fn from_code(v: u64) -> Result<Self, ScientificModelError> {
        match v {
            0 => Ok(Self::Gregorian),
            1 => Ok(Self::ProlepticGregorian),
            2 => Ok(Self::Julian),
            3 => Ok(Self::NoLeap),
            4 => Ok(Self::AllLeap),
            5 => Ok(Self::Day360),
            6 => Ok(Self::Utc),
            _ => Err(ScientificModelError::InvalidEncoding),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimePrecision {
    Day,
    Hour,
    Minute,
    Second,
    Millisecond,
    Microsecond,
    Nanosecond,
}
impl TimePrecision {
    fn code(self) -> u64 {
        match self {
            Self::Day => 0,
            Self::Hour => 1,
            Self::Minute => 2,
            Self::Second => 3,
            Self::Millisecond => 4,
            Self::Microsecond => 5,
            Self::Nanosecond => 6,
        }
    }
    fn from_code(v: u64) -> Result<Self, ScientificModelError> {
        match v {
            0 => Ok(Self::Day),
            1 => Ok(Self::Hour),
            2 => Ok(Self::Minute),
            3 => Ok(Self::Second),
            4 => Ok(Self::Millisecond),
            5 => Ok(Self::Microsecond),
            6 => Ok(Self::Nanosecond),
            _ => Err(ScientificModelError::InvalidEncoding),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeCoordinate {
    pub calendar: TimeCalendar,
    pub units: String,
    pub epoch: String,
    pub timezone: String,
    pub precision: TimePrecision,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalKind {
    Pressure,
    GeometricHeight,
    GeopotentialHeight,
    Altitude,
    Depth,
    Hybrid,
    SoilLayer,
    Other,
}
impl VerticalKind {
    fn code(self) -> u64 {
        match self {
            Self::Pressure => 0,
            Self::GeometricHeight => 1,
            Self::GeopotentialHeight => 2,
            Self::Altitude => 3,
            Self::Depth => 4,
            Self::Hybrid => 5,
            Self::SoilLayer => 6,
            Self::Other => 7,
        }
    }
    fn from_code(v: u64) -> Result<Self, ScientificModelError> {
        match v {
            0 => Ok(Self::Pressure),
            1 => Ok(Self::GeometricHeight),
            2 => Ok(Self::GeopotentialHeight),
            3 => Ok(Self::Altitude),
            4 => Ok(Self::Depth),
            5 => Ok(Self::Hybrid),
            6 => Ok(Self::SoilLayer),
            7 => Ok(Self::Other),
            _ => Err(ScientificModelError::InvalidEncoding),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalDirection {
    Up,
    Down,
}
impl VerticalDirection {
    fn code(self) -> u64 {
        match self {
            Self::Up => 0,
            Self::Down => 1,
        }
    }
    fn from_code(v: u64) -> Result<Self, ScientificModelError> {
        match v {
            0 => Ok(Self::Up),
            1 => Ok(Self::Down),
            _ => Err(ScientificModelError::InvalidEncoding),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerticalCoordinate {
    pub kind: VerticalKind,
    pub units: String,
    pub positive: VerticalDirection,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LongitudeConvention {
    Minus180To180,
    ZeroTo360,
    Unwrapped,
}
impl LongitudeConvention {
    fn code(self) -> u64 {
        match self {
            Self::Minus180To180 => 0,
            Self::ZeroTo360 => 1,
            Self::Unwrapped => 2,
        }
    }
    fn from_code(v: u64) -> Result<Self, ScientificModelError> {
        match v {
            0 => Ok(Self::Minus180To180),
            1 => Ok(Self::ZeroTo360),
            2 => Ok(Self::Unwrapped),
            _ => Err(ScientificModelError::InvalidEncoding),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coordinate {
    pub name: String,
    pub variable: String,
    pub axis: CoordinateAxis,
    pub time: Option<TimeCoordinate>,
    pub vertical: Option<VerticalCoordinate>,
    pub longitude_convention: Option<LongitudeConvention>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridMapping {
    pub name: String,
    pub mapping_name: String,
    pub crs_wkt: Option<String>,
    pub epsg_code: Option<String>,
    pub axis_order: Vec<String>,
    pub attributes: BTreeMap<String, AttributeValue>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub source: String,
    pub provider: Option<String>,
    pub product: Option<String>,
    pub institution: Option<String>,
    pub license: Option<String>,
    pub history: Option<String>,
    pub reference_time: Option<String>,
}

impl Provenance {
    /// Appends a new timestamped lineage entry to the provenance history.
    pub fn append_history(&mut self, entry: &str) {
        let entry = entry.trim();
        if entry.is_empty() {
            return;
        }
        match self.history.as_mut() {
            Some(h) if !h.is_empty() => {
                h.push('\n');
                h.push_str(entry);
            }
            _ => {
                self.history = Some(entry.to_owned());
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScientificDataset {
    pub groups: Vec<Group>,
    pub dimensions: Vec<Dimension>,
    pub variables: Vec<Variable>,
    pub coordinates: Vec<Coordinate>,
    pub grid_mappings: Vec<GridMapping>,
    pub provenance: Vec<Provenance>,
}
impl ScientificDataset {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn variable(&self, path: &str) -> Option<&Variable> {
        self.variables
            .iter()
            .find(|variable| variable.path() == path)
    }

    /// Canonical CBOR representation. Identity-bearing arrays are sorted by their names/paths;
    /// provenance retains caller order because it represents an ordered history.
    pub fn to_cbor(&self) -> Result<Vec<u8>, ScientificModelError> {
        self.validate()?;
        let mut model = self.clone();
        model.groups.sort_by(|a, b| a.path.cmp(&b.path));
        model.dimensions.sort_by(|a, b| a.name.cmp(&b.name));
        model.variables.sort_by_key(|variable| variable.path());
        model.coordinates.sort_by(|a, b| a.name.cmp(&b.name));
        model.grid_mappings.sort_by(|a, b| a.name.cmp(&b.name));
        for variable in &mut model.variables {
            variable.chunks.sort_by_key(|chunk| chunk.chunk_id);
        }
        let mut out = ModelEncoder::new();
        out.array(7)?;
        out.uint(SCIENTIFIC_SCHEMA_VERSION)?;
        out.array(model.groups.len())?;
        for group in &model.groups {
            out.array(2)?;
            out.text(&group.path)?;
            out.attributes(&group.attributes)?;
        }
        out.array(model.dimensions.len())?;
        for dimension in &model.dimensions {
            out.array(3)?;
            out.text(&dimension.name)?;
            out.uint(dimension.length)?;
            out.uint(dimension.role.code())?;
        }
        out.array(model.variables.len())?;
        for variable in &model.variables {
            out.array(11)?;
            out.text(&variable.name)?;
            out.text(&variable.group)?;
            out.uint(variable.data_type.code())?;
            out.uint(variable.byte_order.code())?;
            out.text_array(&variable.dimensions)?;
            out.uint_array(&variable.chunk_shape)?;
            out.scalar(variable.fill_value)?;
            out.scalar(variable.missing_value)?;
            out.attributes(&variable.attributes)?;
            out.array(variable.chunks.len())?;
            for chunk in &variable.chunks {
                out.array(3)?;
                out.uint(chunk.chunk_id)?;
                out.uint_array(&chunk.origin)?;
                out.uint_array(&chunk.shape)?;
            }
            out.optional_text(variable.grid_mapping.as_deref())?;
        }
        out.array(model.coordinates.len())?;
        for coordinate in &model.coordinates {
            out.array(6)?;
            out.text(&coordinate.name)?;
            out.text(&coordinate.variable)?;
            out.uint(coordinate.axis.code())?;
            out.time(coordinate.time.as_ref())?;
            out.vertical(coordinate.vertical.as_ref())?;
            match coordinate.longitude_convention {
                Some(value) => out.uint(value.code())?,
                None => out.null()?,
            }
        }
        out.array(model.grid_mappings.len())?;
        for mapping in &model.grid_mappings {
            out.array(6)?;
            out.text(&mapping.name)?;
            out.text(&mapping.mapping_name)?;
            out.optional_text(mapping.crs_wkt.as_deref())?;
            out.optional_text(mapping.epsg_code.as_deref())?;
            out.text_array(&mapping.axis_order)?;
            out.attributes(&mapping.attributes)?;
        }
        out.array(model.provenance.len())?;
        for provenance in &model.provenance {
            out.array(7)?;
            out.text(&provenance.source)?;
            out.optional_text(provenance.provider.as_deref())?;
            out.optional_text(provenance.product.as_deref())?;
            out.optional_text(provenance.institution.as_deref())?;
            out.optional_text(provenance.license.as_deref())?;
            out.optional_text(provenance.history.as_deref())?;
            out.optional_text(provenance.reference_time.as_deref())?;
        }
        Ok(out.finish())
    }

    /// Resolves the intersecting chunk indices for a bounded selection.
    /// Uses an O(k) coordinate grid index for regular arrays, falling back to an O(N) scan for irregular grids.
    pub(crate) fn plan_intersecting_chunk_indices(
        &self,
        variable_path: &str,
        selection: &ArraySelection,
    ) -> Result<Vec<usize>, ScientificModelError> {
        let variable = self
            .variable(variable_path)
            .ok_or(ScientificModelError::InvalidVariable)?;
        let rank = variable.dimensions.len();
        if selection.start.len() != rank || selection.count.len() != rank {
            return Err(ScientificModelError::InvalidSelection);
        }
        let mut dimension_lengths = Vec::with_capacity(rank);
        for name in &variable.dimensions {
            let dimension = self
                .dimensions
                .iter()
                .find(|dimension| dimension.name == *name)
                .ok_or(ScientificModelError::UnknownDimension)?;
            dimension_lengths.push(dimension.length);
        }
        for ((start, count), dimension_length) in selection
            .start
            .iter()
            .zip(&selection.count)
            .zip(&dimension_lengths)
        {
            let end = start
                .checked_add(*count)
                .ok_or(ScientificModelError::InvalidSelection)?;
            if end > *dimension_length {
                return Err(ScientificModelError::InvalidSelection);
            }
        }
        let output_bytes =
            checked_array_output_bytes(&selection.count, variable.data_type.element_size())?;
        if output_bytes == 0 || variable.chunks.is_empty() {
            return Ok(Vec::new());
        }

        // Fast path: O(k) coordinate grid indexing for regular arrays
        if let Some(indices) = try_regular_grid_indices(variable, &dimension_lengths, selection) {
            return Ok(indices);
        }

        // Fallback: linear scan over variable chunks for ragged/irregular layouts
        let mut chunk_indices = Vec::new();
        for (idx, chunk) in variable.chunks.iter().enumerate() {
            let mut intersects = true;
            for ((start, count), (origin, shape)) in selection
                .start
                .iter()
                .zip(&selection.count)
                .zip(chunk.origin.iter().zip(&chunk.shape))
            {
                let selection_end = start
                    .checked_add(*count)
                    .ok_or(ScientificModelError::InvalidSelection)?;
                let chunk_end = origin
                    .checked_add(*shape)
                    .ok_or(ScientificModelError::DimensionProductOverflow)?;
                if *start >= chunk_end || *origin >= selection_end {
                    intersects = false;
                    break;
                }
            }
            if intersects {
                chunk_indices.push(idx);
            }
        }
        Ok(chunk_indices)
    }

    /// Plans a bounded row-major selection across variable chunks without requiring snapshot or disk context.
    pub fn plan_array_slice(
        &self,
        variable_path: &str,
        selection: &ArraySelection,
    ) -> Result<ArrayReadPlan, ScientificModelError> {
        let variable = self
            .variable(variable_path)
            .ok_or(ScientificModelError::InvalidVariable)?;
        let chunk_indices = self.plan_intersecting_chunk_indices(variable_path, selection)?;
        let output_bytes =
            checked_array_output_bytes(&selection.count, variable.data_type.element_size())?;
        let chunk_ids = chunk_indices
            .iter()
            .map(|&idx| variable.chunks[idx].chunk_id)
            .collect();
        Ok(ArrayReadPlan {
            variable_path: variable_path.to_owned(),
            data_type: variable.data_type,
            byte_order: variable.byte_order,
            start: selection.start.clone(),
            shape: selection.count.clone(),
            output_bytes,
            chunk_ids,
        })
    }

    pub fn from_cbor(bytes: &[u8]) -> Result<Self, ScientificModelError> {
        if bytes.len() as u64 > MAX_METADATA_BYTES {
            return Err(ScientificModelError::TooLarge);
        }
        let mut input = ModelDecoder::new(bytes);
        input.array_exact(7)?;
        if input.uint()? != SCIENTIFIC_SCHEMA_VERSION {
            return Err(ScientificModelError::InvalidSchemaVersion);
        }
        let group_count = input.array_bounded(MAX_SCIENTIFIC_GROUPS)?;
        let mut groups = Vec::with_capacity(group_count);
        for _ in 0..group_count {
            input.array_exact(2)?;
            groups.push(Group {
                path: input.text()?,
                attributes: input.attributes()?,
            });
        }
        let dimension_count = input.array_bounded(MAX_SCIENTIFIC_DIMENSIONS)?;
        let mut dimensions = Vec::with_capacity(dimension_count);
        for _ in 0..dimension_count {
            input.array_exact(3)?;
            dimensions.push(Dimension {
                name: input.text()?,
                length: input.uint()?,
                role: DimensionRole::from_code(input.uint()?)?,
            });
        }
        let variable_count = input.array_bounded(MAX_SCIENTIFIC_VARIABLES)?;
        let mut variables = Vec::with_capacity(variable_count);
        for _ in 0..variable_count {
            input.array_exact(11)?;
            let name = input.text()?;
            let group = input.text()?;
            let data_type = DataType::from_code(input.uint()?)?;
            let byte_order = PayloadByteOrder::from_code(input.uint()?)?;
            let dimensions = input.text_array(MAX_SCIENTIFIC_RANK)?;
            let chunk_shape = input.uint_array(MAX_SCIENTIFIC_RANK)?;
            let fill_value = input.scalar()?;
            let missing_value = input.scalar()?;
            let attributes = input.attributes()?;
            let chunk_count = input.array_bounded(MAX_CHUNKS as usize)?;
            let mut chunks = Vec::with_capacity(chunk_count);
            for _ in 0..chunk_count {
                input.array_exact(3)?;
                chunks.push(ArrayChunkRef {
                    chunk_id: input.uint()?,
                    origin: input.uint_array(MAX_SCIENTIFIC_RANK)?,
                    shape: input.uint_array(MAX_SCIENTIFIC_RANK)?,
                });
            }
            let grid_mapping = input.optional_text()?;
            variables.push(Variable {
                name,
                group,
                data_type,
                byte_order,
                dimensions,
                chunk_shape,
                fill_value,
                missing_value,
                attributes,
                chunks,
                grid_mapping,
            });
        }
        let coordinate_count = input.array_bounded(MAX_SCIENTIFIC_COORDINATES)?;
        let mut coordinates = Vec::with_capacity(coordinate_count);
        for _ in 0..coordinate_count {
            input.array_exact(6)?;
            coordinates.push(Coordinate {
                name: input.text()?,
                variable: input.text()?,
                axis: CoordinateAxis::from_code(input.uint()?)?,
                time: input.time()?,
                vertical: input.vertical()?,
                longitude_convention: input.optional_code(LongitudeConvention::from_code)?,
            });
        }
        let mapping_count = input.array_bounded(MAX_SCIENTIFIC_GRID_MAPPINGS)?;
        let mut grid_mappings = Vec::with_capacity(mapping_count);
        for _ in 0..mapping_count {
            input.array_exact(6)?;
            grid_mappings.push(GridMapping {
                name: input.text()?,
                mapping_name: input.text()?,
                crs_wkt: input.optional_text()?,
                epsg_code: input.optional_text()?,
                axis_order: input.text_array(MAX_SCIENTIFIC_RANK)?,
                attributes: input.attributes()?,
            });
        }
        let provenance_count = input.array_bounded(MAX_SCIENTIFIC_PROVENANCE)?;
        let mut provenance = Vec::with_capacity(provenance_count);
        for _ in 0..provenance_count {
            input.array_exact(7)?;
            provenance.push(Provenance {
                source: input.text()?,
                provider: input.optional_text()?,
                product: input.optional_text()?,
                institution: input.optional_text()?,
                license: input.optional_text()?,
                history: input.optional_text()?,
                reference_time: input.optional_text()?,
            });
        }
        input.finish()?;
        let result = Self {
            groups,
            dimensions,
            variables,
            coordinates,
            grid_mappings,
            provenance,
        };
        result.validate()?;
        if result.to_cbor()?.as_slice() != bytes {
            return Err(ScientificModelError::NonCanonical);
        }
        Ok(result)
    }

    pub fn validate(&self) -> Result<(), ScientificModelError> {
        if self.groups.len() > MAX_SCIENTIFIC_GROUPS
            || self.dimensions.len() > MAX_SCIENTIFIC_DIMENSIONS
            || self.variables.len() > MAX_SCIENTIFIC_VARIABLES
            || self.coordinates.len() > MAX_SCIENTIFIC_COORDINATES
            || self.grid_mappings.len() > MAX_SCIENTIFIC_GRID_MAPPINGS
            || self.provenance.len() > MAX_SCIENTIFIC_PROVENANCE
        {
            return Err(ScientificModelError::TooManyObjects);
        }
        let mut groups = BTreeSet::new();
        for group in &self.groups {
            if !valid_group_path(&group.path) || !groups.insert(group.path.as_str()) {
                return Err(ScientificModelError::InvalidGroup);
            }
            validate_attributes(&group.attributes)?;
        }
        for group in &self.groups {
            let parent = group
                .path
                .rsplit_once('/')
                .map(|(parent, _)| if parent.is_empty() { "/" } else { parent })
                .ok_or(ScientificModelError::InvalidGroup)?;
            if parent != "/" && !groups.contains(parent) {
                return Err(ScientificModelError::UnknownGroup);
            }
        }
        let mut dimensions = BTreeSet::new();
        for dimension in &self.dimensions {
            if !valid_identifier(&dimension.name) || !dimensions.insert(dimension.name.as_str()) {
                return Err(ScientificModelError::InvalidDimension);
            }
        }
        let mut variable_paths = BTreeSet::new();
        let mut chunk_ids = BTreeSet::new();
        let mut total_chunk_references = 0u64;
        let group_set: BTreeSet<&str> = groups.iter().copied().collect();
        for variable in &self.variables {
            if !valid_identifier(&variable.name)
                || (variable.group != "/" && !group_set.contains(variable.group.as_str()))
                || !variable_paths.insert(variable.path())
            {
                return Err(ScientificModelError::InvalidVariable);
            }
            let rank = variable.dimensions.len();
            if rank > MAX_SCIENTIFIC_RANK || variable.chunk_shape.len() != rank {
                return Err(ScientificModelError::RankMismatch);
            }
            let mut used_dimensions = BTreeSet::new();
            let mut dimension_lengths = Vec::with_capacity(rank);
            for (index, name) in variable.dimensions.iter().enumerate() {
                if !dimensions.contains(name.as_str()) || !used_dimensions.insert(name.as_str()) {
                    return Err(ScientificModelError::UnknownDimension);
                }
                let dim = self
                    .dimensions
                    .iter()
                    .find(|dimension| dimension.name == *name)
                    .ok_or(ScientificModelError::UnknownDimension)?;
                let chunk = variable.chunk_shape[index];
                if chunk == 0 {
                    return Err(ScientificModelError::InvalidChunkShape);
                }
                dimension_lengths.push(dim.length);
            }
            variable.element_count(self)?;
            let expected_chunks = if dimension_lengths.contains(&0) {
                0
            } else {
                dimension_lengths
                    .iter()
                    .zip(&variable.chunk_shape)
                    .try_fold(1u64, |total, (length, chunk)| {
                        total
                            .checked_mul(length.div_ceil(*chunk))
                            .ok_or(ScientificModelError::DimensionProductOverflow)
                    })?
            };
            if expected_chunks > MAX_CHUNKS || variable.chunks.len() as u64 != expected_chunks {
                return Err(ScientificModelError::ChunkCoverage);
            }
            total_chunk_references = total_chunk_references
                .checked_add(variable.chunks.len() as u64)
                .ok_or(ScientificModelError::TooManyChunkReferences)?;
            if total_chunk_references > MAX_CHUNKS {
                return Err(ScientificModelError::TooManyChunkReferences);
            }
            let mut origins = BTreeSet::new();
            for chunk in &variable.chunks {
                if chunk.origin.len() != rank
                    || chunk.shape.len() != rank
                    || !chunk_ids.insert(chunk.chunk_id)
                {
                    return Err(ScientificModelError::InvalidChunkMap);
                }
                for axis in 0..rank {
                    let dimension = self
                        .dimensions
                        .iter()
                        .find(|dimension| dimension.name == variable.dimensions[axis])
                        .ok_or(ScientificModelError::UnknownDimension)?;
                    let origin = chunk.origin[axis];
                    let size = chunk.shape[axis];
                    let nominal = variable.chunk_shape[axis];
                    if dimension.length == 0
                        || size == 0
                        || origin % nominal != 0
                        || origin >= dimension.length
                    {
                        return Err(ScientificModelError::InvalidChunkMap);
                    }
                    let expected = nominal.min(dimension.length - origin);
                    if size != expected {
                        return Err(ScientificModelError::InvalidChunkMap);
                    }
                }
                if !origins.insert(chunk.origin.clone()) {
                    return Err(ScientificModelError::InvalidChunkMap);
                }
            }
            if variable
                .fill_value
                .is_some_and(|value| !value.matches(variable.data_type))
                || variable
                    .missing_value
                    .is_some_and(|value| !value.matches(variable.data_type))
            {
                return Err(ScientificModelError::InvalidScalarType);
            }
            validate_attributes(&variable.attributes)?;
        }
        let mut mapping_names = BTreeSet::new();
        for mapping in &self.grid_mappings {
            if !valid_identifier(&mapping.name)
                || mapping.mapping_name.is_empty()
                || !mapping_names.insert(mapping.name.as_str())
                || (mapping.crs_wkt.as_ref().is_none_or(String::is_empty)
                    && mapping.epsg_code.as_ref().is_none_or(String::is_empty))
            {
                return Err(ScientificModelError::InvalidGridMapping);
            }
            if mapping.axis_order.is_empty()
                || mapping.axis_order.iter().any(|axis| axis.is_empty())
            {
                return Err(ScientificModelError::InvalidGridMapping);
            }
            validate_attributes(&mapping.attributes)?;
        }
        let mut coordinate_names = BTreeSet::new();
        for coordinate in &self.coordinates {
            if !valid_identifier(&coordinate.name)
                || !coordinate_names.insert(coordinate.name.as_str())
                || !variable_paths.contains(coordinate.variable.as_str())
            {
                return Err(ScientificModelError::InvalidCoordinate);
            }
            let time_axis = matches!(
                coordinate.axis,
                CoordinateAxis::Time | CoordinateAxis::ForecastReferenceTime
            );
            let vertical_axis = matches!(
                coordinate.axis,
                CoordinateAxis::Vertical | CoordinateAxis::Height
            );
            if time_axis != coordinate.time.is_some()
                || vertical_axis != coordinate.vertical.is_some()
                || (coordinate.axis == CoordinateAxis::Longitude)
                    != coordinate.longitude_convention.is_some()
            {
                return Err(ScientificModelError::InvalidCoordinate);
            }
            if coordinate.time.as_ref().is_some_and(|time| {
                time.units.is_empty() || time.epoch.is_empty() || time.timezone.is_empty()
            }) {
                return Err(ScientificModelError::InvalidCoordinate);
            }
            if coordinate
                .vertical
                .as_ref()
                .is_some_and(|vertical| vertical.units.is_empty())
            {
                return Err(ScientificModelError::InvalidCoordinate);
            }
        }
        for variable in &self.variables {
            if variable
                .grid_mapping
                .as_ref()
                .is_some_and(|name| !mapping_names.contains(name.as_str()))
            {
                return Err(ScientificModelError::UnknownGridMapping);
            }
        }
        for provenance in &self.provenance {
            if provenance.source.is_empty() {
                return Err(ScientificModelError::InvalidProvenance);
            }
        }
        self.validate_grid_topologies()?;
        Ok(())
    }

    pub(crate) fn validate_descriptors(
        &self,
        descriptors: &[ChunkDescriptor],
    ) -> Result<(), ScientificModelError> {
        self.validate()?;
        let by_id: BTreeMap<u64, u64> = descriptors
            .iter()
            .map(|descriptor| (descriptor.id, descriptor.raw_len))
            .collect();
        for variable in &self.variables {
            for chunk in &variable.chunks {
                let stored_len = *by_id
                    .get(&chunk.chunk_id)
                    .ok_or(ScientificModelError::MissingChunk)?;
                let elements = checked_product(&chunk.shape)?;
                let expected = elements
                    .checked_mul(variable.data_type.element_size())
                    .ok_or(ScientificModelError::DimensionProductOverflow)?;
                if stored_len != expected {
                    return Err(ScientificModelError::ChunkLengthMismatch);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_inputs(
        &self,
        chunks: &[EncodedChunkInput<'_>],
    ) -> Result<(), ScientificModelError> {
        self.validate()?;
        let by_id: BTreeMap<u64, &[u8]> =
            chunks.iter().map(|chunk| (chunk.id, chunk.data)).collect();
        for variable in &self.variables {
            for chunk in &variable.chunks {
                let bytes = by_id
                    .get(&chunk.chunk_id)
                    .ok_or(ScientificModelError::MissingChunk)?;
                let expected = checked_product(&chunk.shape)?
                    .checked_mul(variable.data_type.element_size())
                    .ok_or(ScientificModelError::DimensionProductOverflow)?;
                if bytes.len() as u64 != expected {
                    return Err(ScientificModelError::ChunkLengthMismatch);
                }
                if variable.data_type == DataType::Bool8 && bytes.iter().any(|value| *value > 1) {
                    return Err(ScientificModelError::InvalidBooleanValue);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_reader_inputs(
        &self,
        chunks: &[ChunkReaderInput<'_>],
    ) -> Result<(), ScientificModelError> {
        self.validate()?;
        let by_id: BTreeMap<u64, u64> = chunks
            .iter()
            .map(|chunk| (chunk.id, chunk.raw_len))
            .collect();
        for variable in &self.variables {
            for chunk in &variable.chunks {
                let raw_len = *by_id
                    .get(&chunk.chunk_id)
                    .ok_or(ScientificModelError::MissingChunk)?;
                let expected = checked_product(&chunk.shape)?
                    .checked_mul(variable.data_type.element_size())
                    .ok_or(ScientificModelError::DimensionProductOverflow)?;
                if raw_len != expected {
                    return Err(ScientificModelError::ChunkLengthMismatch);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScientificModelError {
    InvalidEncoding,
    InvalidSchemaVersion,
    NonCanonical,
    TooLarge,
    TooManyObjects,
    TooManyChunkReferences,
    InvalidIdentifier,
    InvalidGroup,
    UnknownGroup,
    InvalidDimension,
    UnknownDimension,
    InvalidVariable,
    RankMismatch,
    InvalidChunkShape,
    ChunkCoverage,
    InvalidChunkMap,
    InvalidScalarType,
    InvalidCoordinate,
    InvalidGridMapping,
    UnknownGridMapping,
    InvalidProvenance,
    DimensionProductOverflow,
    MissingChunk,
    ChunkLengthMismatch,
    InvalidBooleanValue,
    InvalidSelection,
    ArrayTooLarge,
    MissingScientificModel,
}
impl fmt::Display for ScientificModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WX-1 scientific model error: {self:?}")
    }
}
impl std::error::Error for ScientificModelError {}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.len() <= 255
        && value
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.')
}
fn valid_group_path(value: &str) -> bool {
    if value == "/" || !value.starts_with('/') || value.ends_with('/') {
        return false;
    }
    value[1..].split('/').all(valid_identifier)
}
fn validate_attributes(
    attributes: &BTreeMap<String, AttributeValue>,
) -> Result<(), ScientificModelError> {
    if attributes.len() > 100_000 {
        return Err(ScientificModelError::TooManyObjects);
    }
    for (key, value) in attributes {
        if key.is_empty() || key.len() > 4096 {
            return Err(ScientificModelError::InvalidIdentifier);
        }
        match value {
            AttributeValue::Text(text) if text.len() > MAX_METADATA_BYTES as usize => {
                return Err(ScientificModelError::TooLarge)
            }
            AttributeValue::Bytes(bytes) if bytes.len() > MAX_METADATA_BYTES as usize => {
                return Err(ScientificModelError::TooLarge)
            }
            _ => {}
        }
    }
    Ok(())
}
fn checked_product(values: &[u64]) -> Result<u64, ScientificModelError> {
    values.iter().try_fold(1u64, |product, value| {
        product
            .checked_mul(*value)
            .ok_or(ScientificModelError::DimensionProductOverflow)
    })
}

struct ModelEncoder {
    bytes: Vec<u8>,
}
impl ModelEncoder {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }
    fn append(&mut self, bytes: &[u8]) -> Result<(), ScientificModelError> {
        let end = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or(ScientificModelError::TooLarge)?;
        if end as u64 > MAX_METADATA_BYTES {
            return Err(ScientificModelError::TooLarge);
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn head(&mut self, major: u8, value: u64) -> Result<(), ScientificModelError> {
        let mut b = Vec::with_capacity(9);
        cbor_head(&mut b, major, value);
        self.append(&b)
    }
    fn uint(&mut self, value: u64) -> Result<(), ScientificModelError> {
        self.head(0, value)
    }
    fn integer(&mut self, value: i64) -> Result<(), ScientificModelError> {
        if value >= 0 {
            self.head(0, value as u64)
        } else {
            self.head(1, (-1i128 - value as i128) as u64)
        }
    }
    fn text(&mut self, value: &str) -> Result<(), ScientificModelError> {
        self.head(3, value.len() as u64)?;
        self.append(value.as_bytes())
    }
    fn bytes(&mut self, value: &[u8]) -> Result<(), ScientificModelError> {
        self.head(2, value.len() as u64)?;
        self.append(value)
    }
    fn array(&mut self, count: usize) -> Result<(), ScientificModelError> {
        self.head(4, count as u64)
    }
    fn null(&mut self) -> Result<(), ScientificModelError> {
        self.append(&[0xf6])
    }
    fn boolean(&mut self, value: bool) -> Result<(), ScientificModelError> {
        self.append(&[if value { 0xf5 } else { 0xf4 }])
    }
    fn optional_text(&mut self, value: Option<&str>) -> Result<(), ScientificModelError> {
        match value {
            Some(v) => self.text(v),
            None => self.null(),
        }
    }
    fn text_array(&mut self, values: &[String]) -> Result<(), ScientificModelError> {
        self.array(values.len())?;
        for value in values {
            self.text(value)?;
        }
        Ok(())
    }
    fn uint_array(&mut self, values: &[u64]) -> Result<(), ScientificModelError> {
        self.array(values.len())?;
        for value in values {
            self.uint(*value)?;
        }
        Ok(())
    }
    fn scalar(&mut self, value: Option<ScalarValue>) -> Result<(), ScientificModelError> {
        if let Some(value) = value {
            self.array(2)?;
            self.uint(value.code())?;
            self.uint(value.payload())
        } else {
            self.null()
        }
    }
    fn attributes(
        &mut self,
        values: &BTreeMap<String, AttributeValue>,
    ) -> Result<(), ScientificModelError> {
        self.head(5, values.len() as u64)?;
        let mut entries: Vec<_> = values.iter().collect();
        entries.sort_by(|(a, _), (b, _)| canonical_text_cmp(a, b));
        for (key, value) in entries {
            self.text(key)?;
            match value {
                AttributeValue::Text(v) => {
                    self.array(2)?;
                    self.uint(0)?;
                    self.text(v)?;
                }
                AttributeValue::Integer(v) => {
                    self.array(2)?;
                    self.uint(1)?;
                    self.integer(*v)?;
                }
                AttributeValue::Unsigned(v) => {
                    self.array(2)?;
                    self.uint(2)?;
                    self.uint(*v)?;
                }
                AttributeValue::Float64Bits(v) => {
                    self.array(2)?;
                    self.uint(3)?;
                    self.uint(*v)?;
                }
                AttributeValue::Boolean(v) => {
                    self.array(2)?;
                    self.uint(4)?;
                    self.boolean(*v)?;
                }
                AttributeValue::Bytes(v) => {
                    self.array(2)?;
                    self.uint(5)?;
                    self.bytes(v)?;
                }
            }
        }
        Ok(())
    }
    fn time(&mut self, value: Option<&TimeCoordinate>) -> Result<(), ScientificModelError> {
        if let Some(v) = value {
            self.array(5)?;
            self.uint(v.calendar.code())?;
            self.text(&v.units)?;
            self.text(&v.epoch)?;
            self.text(&v.timezone)?;
            self.uint(v.precision.code())
        } else {
            self.null()
        }
    }
    fn vertical(&mut self, value: Option<&VerticalCoordinate>) -> Result<(), ScientificModelError> {
        if let Some(v) = value {
            self.array(3)?;
            self.uint(v.kind.code())?;
            self.text(&v.units)?;
            self.uint(v.positive.code())
        } else {
            self.null()
        }
    }
    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

struct ModelDecoder<'a> {
    decoder: CborDecoder<'a>,
}
impl<'a> ModelDecoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            decoder: CborDecoder { bytes, pos: 0 },
        }
    }
    fn array_exact(&mut self, expected: usize) -> Result<(), ScientificModelError> {
        let count = usize::try_from(
            self.decoder
                .head(4)
                .map_err(|_| ScientificModelError::InvalidEncoding)?,
        )
        .map_err(|_| ScientificModelError::TooLarge)?;
        if count != expected {
            return Err(ScientificModelError::InvalidEncoding);
        }
        Ok(())
    }
    fn array_bounded(&mut self, max: usize) -> Result<usize, ScientificModelError> {
        let count = usize::try_from(
            self.decoder
                .head(4)
                .map_err(|_| ScientificModelError::InvalidEncoding)?,
        )
        .map_err(|_| ScientificModelError::TooLarge)?;
        if count > max {
            return Err(ScientificModelError::TooManyObjects);
        }
        Ok(count)
    }
    fn uint(&mut self) -> Result<u64, ScientificModelError> {
        self.decoder
            .head(0)
            .map_err(|_| ScientificModelError::InvalidEncoding)
    }
    fn integer(&mut self) -> Result<i64, ScientificModelError> {
        let byte = *self
            .decoder
            .bytes
            .get(self.decoder.pos)
            .ok_or(ScientificModelError::InvalidEncoding)?;
        match byte >> 5 {
            0 => i64::try_from(
                self.decoder
                    .head(0)
                    .map_err(|_| ScientificModelError::InvalidEncoding)?,
            )
            .map_err(|_| ScientificModelError::InvalidEncoding),
            1 => {
                let value = self
                    .decoder
                    .head(1)
                    .map_err(|_| ScientificModelError::InvalidEncoding)?;
                if value > i64::MAX as u64 {
                    return Err(ScientificModelError::InvalidEncoding);
                }
                Ok(-1 - (value as i64))
            }
            _ => Err(ScientificModelError::InvalidEncoding),
        }
    }
    fn text(&mut self) -> Result<String, ScientificModelError> {
        self.decoder
            .text()
            .map_err(|_| ScientificModelError::InvalidEncoding)
    }
    fn bytes(&mut self) -> Result<Vec<u8>, ScientificModelError> {
        let len = usize::try_from(
            self.decoder
                .head(2)
                .map_err(|_| ScientificModelError::InvalidEncoding)?,
        )
        .map_err(|_| ScientificModelError::TooLarge)?;
        if len as u64 > MAX_METADATA_BYTES {
            return Err(ScientificModelError::TooLarge);
        }
        let end = self
            .decoder
            .pos
            .checked_add(len)
            .ok_or(ScientificModelError::InvalidEncoding)?;
        let bytes = self
            .decoder
            .bytes
            .get(self.decoder.pos..end)
            .ok_or(ScientificModelError::InvalidEncoding)?
            .to_vec();
        self.decoder.pos = end;
        Ok(bytes)
    }
    fn boolean(&mut self) -> Result<bool, ScientificModelError> {
        let value = *self
            .decoder
            .bytes
            .get(self.decoder.pos)
            .ok_or(ScientificModelError::InvalidEncoding)?;
        self.decoder.pos += 1;
        match value {
            0xf4 => Ok(false),
            0xf5 => Ok(true),
            _ => Err(ScientificModelError::InvalidEncoding),
        }
    }
    fn is_null(&self) -> bool {
        self.decoder.bytes.get(self.decoder.pos) == Some(&0xf6)
    }
    fn consume_null(&mut self) -> Result<(), ScientificModelError> {
        if !self.is_null() {
            return Err(ScientificModelError::InvalidEncoding);
        }
        self.decoder.pos += 1;
        Ok(())
    }
    fn optional_text(&mut self) -> Result<Option<String>, ScientificModelError> {
        if self.is_null() {
            self.consume_null()?;
            Ok(None)
        } else {
            Ok(Some(self.text()?))
        }
    }
    fn optional_code<T>(
        &mut self,
        parse: impl FnOnce(u64) -> Result<T, ScientificModelError>,
    ) -> Result<Option<T>, ScientificModelError> {
        if self.is_null() {
            self.consume_null()?;
            Ok(None)
        } else {
            Ok(Some(parse(self.uint()?)?))
        }
    }
    fn text_array(&mut self, max: usize) -> Result<Vec<String>, ScientificModelError> {
        let count = self.array_bounded(max)?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.text()?);
        }
        Ok(values)
    }
    fn uint_array(&mut self, max: usize) -> Result<Vec<u64>, ScientificModelError> {
        let count = self.array_bounded(max)?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.uint()?);
        }
        Ok(values)
    }
    fn scalar(&mut self) -> Result<Option<ScalarValue>, ScientificModelError> {
        if self.is_null() {
            self.consume_null()?;
            Ok(None)
        } else {
            self.array_exact(2)?;
            let code = self.uint()?;
            let payload = self.uint()?;
            Ok(Some(ScalarValue::from_parts(code, payload)?))
        }
    }
    fn attributes(&mut self) -> Result<BTreeMap<String, AttributeValue>, ScientificModelError> {
        let count = usize::try_from(
            self.decoder
                .head(5)
                .map_err(|_| ScientificModelError::InvalidEncoding)?,
        )
        .map_err(|_| ScientificModelError::TooLarge)?;
        if count > 100_000 {
            return Err(ScientificModelError::TooManyObjects);
        }
        let mut map = BTreeMap::new();
        let mut previous: Option<String> = None;
        for _ in 0..count {
            let key = self.text()?;
            if previous
                .as_ref()
                .is_some_and(|prev| canonical_text_cmp(prev, &key) != std::cmp::Ordering::Less)
            {
                return Err(ScientificModelError::NonCanonical);
            }
            self.array_exact(2)?;
            let tag = self.uint()?;
            let value = match tag {
                0 => AttributeValue::Text(self.text()?),
                1 => AttributeValue::Integer(self.integer()?),
                2 => AttributeValue::Unsigned(self.uint()?),
                3 => AttributeValue::Float64Bits(self.uint()?),
                4 => AttributeValue::Boolean(self.boolean()?),
                5 => AttributeValue::Bytes(self.bytes()?),
                _ => return Err(ScientificModelError::InvalidEncoding),
            };
            if map.insert(key.clone(), value).is_some() {
                return Err(ScientificModelError::NonCanonical);
            }
            previous = Some(key);
        }
        Ok(map)
    }
    fn time(&mut self) -> Result<Option<TimeCoordinate>, ScientificModelError> {
        if self.is_null() {
            self.consume_null()?;
            Ok(None)
        } else {
            self.array_exact(5)?;
            Ok(Some(TimeCoordinate {
                calendar: TimeCalendar::from_code(self.uint()?)?,
                units: self.text()?,
                epoch: self.text()?,
                timezone: self.text()?,
                precision: TimePrecision::from_code(self.uint()?)?,
            }))
        }
    }
    fn vertical(&mut self) -> Result<Option<VerticalCoordinate>, ScientificModelError> {
        if self.is_null() {
            self.consume_null()?;
            Ok(None)
        } else {
            self.array_exact(3)?;
            Ok(Some(VerticalCoordinate {
                kind: VerticalKind::from_code(self.uint()?)?,
                units: self.text()?,
                positive: VerticalDirection::from_code(self.uint()?)?,
            }))
        }
    }
    fn finish(self) -> Result<(), ScientificModelError> {
        if self.decoder.pos == self.decoder.bytes.len() {
            Ok(())
        } else {
            Err(ScientificModelError::InvalidEncoding)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArraySelection {
    pub start: Vec<u64>,
    pub count: Vec<u64>,
}
impl ArraySelection {
    pub fn new(start: Vec<u64>, count: Vec<u64>) -> Self {
        Self { start, count }
    }
}

/// Read plan for a bounded array selection.
///
/// The plan identifies only the chunks that intersect the requested selection. It is an
/// experimental, additive API intended to let importers and schedulers inspect I/O work
/// before decoding data. It does not expose file offsets or change the on-disk format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayReadPlan {
    /// Absolute scientific variable path.
    pub variable_path: String,
    /// Datatype of the selected variable; payload bytes are not converted.
    pub data_type: DataType,
    /// Byte order of the selected variable's payload.
    pub byte_order: PayloadByteOrder,
    /// Starting coordinate of the requested selection.
    pub start: Vec<u64>,
    /// Shape of the requested output selection.
    pub shape: Vec<u64>,
    /// Exact output size in bytes, checked against `MAX_ARRAY_READ_BYTES`.
    pub output_bytes: u64,
    /// IDs of chunks intersecting the selection, in model order.
    pub chunk_ids: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScientificArray {
    pub data_type: DataType,
    pub byte_order: PayloadByteOrder,
    pub shape: Vec<u64>,
    pub data: Vec<u8>,
}

/// A validated scientific view pinned to one immutable generation snapshot.
#[derive(Debug, Clone)]
pub struct ScientificSnapshot {
    snapshot: GenerationSnapshot,
    dataset: ScientificDataset,
}
impl ScientificSnapshot {
    pub fn generation(&self) -> u64 {
        self.snapshot.generation()
    }
    pub fn valid_end(&self) -> u64 {
        self.snapshot.valid_end()
    }
    pub fn snapshot(&self) -> &super::GenerationSnapshot {
        &self.snapshot
    }
    pub fn dataset(&self) -> &ScientificDataset {
        &self.dataset
    }
    pub fn metadata(&self) -> &super::Metadata {
        self.snapshot.metadata()
    }
    pub fn descriptors(&self) -> &[super::ChunkDescriptor] {
        self.snapshot.descriptors()
    }

    /// Plans a bounded row-major selection without reading or decompressing chunk payloads.
    ///
    /// Returned chunk IDs are the only chunks `read_array_slice` will decode for this
    /// selection. The plan is valid for this immutable scientific snapshot.
    pub fn plan_array_slice(
        &self,
        variable_path: &str,
        selection: &ArraySelection,
    ) -> Result<ArrayReadPlan, ChunkIoError> {
        self.dataset
            .plan_array_slice(variable_path, selection)
            .map_err(ChunkIoError::ScientificModel)
    }

    /// Performs exhaustive cryptographic and CRC32C verification across all stored scientific chunks.
    pub fn validate_full<R: Read + Seek>(&self, reader: &mut R) -> Result<(), ChunkIoError> {
        self.snapshot.validate_full(reader)?;
        self.dataset
            .validate_descriptors(self.snapshot.descriptors())
            .map_err(ChunkIoError::ScientificModel)
    }

    /// Reads a bounded row-major selection, decoding only the chunks in its read plan.
    pub fn read_array_slice<R: Read + Seek>(
        &self,
        reader: &mut R,
        variable_path: &str,
        selection: &ArraySelection,
    ) -> Result<ScientificArray, ChunkIoError> {
        self.read_array_slice_mode(reader, variable_path, selection, true)
    }

    /// Reads a bounded row-major selection, optionally skipping per-chunk BLAKE3 verification
    /// (verifying CRC32C only on read while `validate_full` verifies BLAKE3).
    pub fn read_array_slice_mode<R: Read + Seek>(
        &self,
        reader: &mut R,
        variable_path: &str,
        selection: &ArraySelection,
        verify_blake3: bool,
    ) -> Result<ScientificArray, ChunkIoError> {
        let plan = self.plan_array_slice(variable_path, selection)?;
        let variable =
            self.dataset
                .variable(variable_path)
                .ok_or(ChunkIoError::ScientificModel(
                    ScientificModelError::InvalidVariable,
                ))?;
        let output_len = usize::try_from(plan.output_bytes)
            .map_err(|_| ChunkIoError::ScientificModel(ScientificModelError::ArrayTooLarge))?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(output_len)
            .map_err(|_| ChunkIoError::AllocationFailed)?;
        output.resize(output_len, 0);
        if plan.output_bytes == 0 {
            return Ok(ScientificArray {
                data_type: plan.data_type,
                byte_order: plan.byte_order,
                shape: plan.shape,
                data: output,
            });
        }
        let chunk_indices = self
            .dataset
            .plan_intersecting_chunk_indices(variable_path, selection)
            .map_err(ChunkIoError::ScientificModel)?;
        if chunk_indices.len() == 1 {
            let chunk = &variable.chunks[chunk_indices[0]];
            let bytes = self
                .snapshot
                .read_chunk_mode(reader, chunk.chunk_id, verify_blake3)?;
            copy_intersection(variable, chunk, selection, &bytes, &mut output)?;
        } else {
            const READ_BATCH: usize = 8;
            for batch in chunk_indices.chunks(READ_BATCH) {
                let ids: Vec<u64> = batch
                    .iter()
                    .map(|&idx| variable.chunks[idx].chunk_id)
                    .collect();
                let decoded =
                    self.snapshot
                        .read_chunks_parallel_mode(reader, &ids, verify_blake3)?;
                for (&idx, bytes) in batch.iter().zip(decoded.iter()) {
                    let chunk = &variable.chunks[idx];
                    copy_intersection(variable, chunk, selection, bytes, &mut output)?;
                }
            }
        }
        if plan.data_type == DataType::Bool8 && output.iter().any(|value| *value > 1) {
            return Err(ChunkIoError::ScientificModel(
                ScientificModelError::InvalidBooleanValue,
            ));
        }
        Ok(ScientificArray {
            data_type: plan.data_type,
            byte_order: plan.byte_order,
            shape: plan.shape,
            data: output,
        })
    }

    /// Reads a batch of discrete multidimensional point coordinates (R8/R4),
    /// grouping coordinates by chunk to decode each intersecting chunk exactly once.
    pub fn read_point_coordinates<R: Read + Seek>(
        &self,
        reader: &mut R,
        variable_path: &str,
        points: &[Vec<u64>],
    ) -> Result<ScientificArray, ChunkIoError> {
        let variable =
            self.dataset
                .variable(variable_path)
                .ok_or(ChunkIoError::ScientificModel(
                    ScientificModelError::InvalidVariable,
                ))?;
        let rank = variable.dimensions.len();
        let item_size = usize::try_from(variable.data_type.element_size())
            .map_err(|_| ChunkIoError::LengthOverflow)?;

        if points.is_empty() {
            return Ok(ScientificArray {
                data_type: variable.data_type,
                byte_order: variable.byte_order,
                shape: vec![0],
                data: Vec::new(),
            });
        }

        let mut dimension_lengths = Vec::with_capacity(rank);
        for name in &variable.dimensions {
            let dimension = self
                .dataset
                .dimensions
                .iter()
                .find(|dim| dim.name == *name)
                .ok_or(ChunkIoError::ScientificModel(
                    ScientificModelError::UnknownDimension,
                ))?;
            dimension_lengths.push(dimension.length);
        }

        let total_output_bytes = (points.len() as u64).checked_mul(item_size as u64).ok_or(
            ChunkIoError::ScientificModel(ScientificModelError::ArrayTooLarge),
        )?;
        if total_output_bytes > MAX_ARRAY_READ_BYTES {
            return Err(ChunkIoError::ScientificModel(
                ScientificModelError::ArrayTooLarge,
            ));
        }

        let mut chunk_map: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();

        for (pt_idx, pt) in points.iter().enumerate() {
            if pt.len() != rank {
                return Err(ChunkIoError::ScientificModel(
                    ScientificModelError::RankMismatch,
                ));
            }
            for (&coord, &dim_len) in pt.iter().zip(dimension_lengths.iter()) {
                if coord >= dim_len {
                    return Err(ChunkIoError::ScientificModel(
                        ScientificModelError::InvalidSelection,
                    ));
                }
            }

            let mut found_chunk_idx = None;
            let mut local_byte_offset = 0usize;

            for (c_idx, chunk) in variable.chunks.iter().enumerate() {
                let mut covers = true;
                let mut offset = 0usize;
                let mut stride = 1usize;

                for d in (0..rank).rev() {
                    let c_orig = chunk.origin[d];
                    let c_shape = chunk.shape[d];
                    let p = pt[d];
                    if p < c_orig || p >= c_orig + c_shape {
                        covers = false;
                        break;
                    }
                    offset += usize::try_from((p - c_orig) * (stride as u64))
                        .map_err(|_| ChunkIoError::LengthOverflow)?;
                    stride *= usize::try_from(c_shape).map_err(|_| ChunkIoError::LengthOverflow)?;
                }

                if covers {
                    found_chunk_idx = Some(c_idx);
                    local_byte_offset = offset
                        .checked_mul(item_size)
                        .ok_or(ChunkIoError::LengthOverflow)?;
                    break;
                }
            }

            let c_idx = found_chunk_idx.ok_or(ChunkIoError::ScientificModel(
                ScientificModelError::InvalidChunkMap,
            ))?;
            chunk_map
                .entry(c_idx)
                .or_default()
                .push((pt_idx, local_byte_offset));
        }

        let mut output = Vec::new();
        output
            .try_reserve_exact(total_output_bytes as usize)
            .map_err(|_| ChunkIoError::AllocationFailed)?;
        output.resize(total_output_bytes as usize, 0);

        const READ_BATCH: usize = 8;
        let unique_chunks: Vec<usize> = chunk_map.keys().copied().collect();

        for batch in unique_chunks.chunks(READ_BATCH) {
            let ids: Vec<u64> = batch
                .iter()
                .map(|&idx| variable.chunks[idx].chunk_id)
                .collect();
            let decoded = self.snapshot.read_chunks_parallel(reader, &ids)?;
            for (&c_idx, chunk_bytes) in batch.iter().zip(decoded.iter()) {
                if let Some(pt_entries) = chunk_map.get(&c_idx) {
                    for &(query_idx, local_offset) in pt_entries {
                        let out_start = query_idx * item_size;
                        if local_offset + item_size > chunk_bytes.len() {
                            return Err(ChunkIoError::ScientificModel(
                                ScientificModelError::ChunkLengthMismatch,
                            ));
                        }
                        output[out_start..out_start + item_size]
                            .copy_from_slice(&chunk_bytes[local_offset..local_offset + item_size]);
                    }
                }
            }
        }

        Ok(ScientificArray {
            data_type: variable.data_type,
            byte_order: variable.byte_order,
            shape: vec![points.len() as u64],
            data: output,
        })
    }
}

fn checked_array_output_bytes(
    shape: &[u64],
    element_size: u64,
) -> Result<u64, ScientificModelError> {
    let elements = checked_product(shape).map_err(|_| ScientificModelError::ArrayTooLarge)?;
    let bytes = elements
        .checked_mul(element_size)
        .ok_or(ScientificModelError::ArrayTooLarge)?;
    if bytes > MAX_ARRAY_READ_BYTES {
        return Err(ScientificModelError::ArrayTooLarge);
    }
    Ok(bytes)
}

fn copy_intersection(
    variable: &Variable,
    chunk: &ArrayChunkRef,
    selection: &ArraySelection,
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ChunkIoError> {
    let rank = variable.dimensions.len();
    let item = usize::try_from(variable.data_type.element_size())
        .map_err(|_| ChunkIoError::LengthOverflow)?;
    if source.len() as u64
        != checked_product(&chunk.shape)
            .map_err(ChunkIoError::ScientificModel)?
            .checked_mul(item as u64)
            .ok_or(ChunkIoError::LengthOverflow)?
    {
        return Err(ChunkIoError::ScientificModel(
            ScientificModelError::ChunkLengthMismatch,
        ));
    }
    if rank == 0 {
        if source.len() != item || target.len() != item {
            return Err(ChunkIoError::ScientificModel(
                ScientificModelError::ChunkLengthMismatch,
            ));
        }
        target.copy_from_slice(source);
        return Ok(());
    }
    let mut starts = Vec::with_capacity(rank);
    let mut lengths = Vec::with_capacity(rank);
    for ((selection_start, selection_count), (chunk_origin, chunk_length)) in selection
        .start
        .iter()
        .zip(&selection.count)
        .zip(chunk.origin.iter().zip(&chunk.shape))
    {
        let selection_end = selection_start
            .checked_add(*selection_count)
            .ok_or(ChunkIoError::LengthOverflow)?;
        let chunk_end = chunk_origin
            .checked_add(*chunk_length)
            .ok_or(ChunkIoError::LengthOverflow)?;
        let start = (*selection_start).max(*chunk_origin);
        let end = selection_end.min(chunk_end);
        starts.push(start);
        lengths.push(end - start);
    }
    let mut contig_start = rank - 1;
    while contig_start > 0 {
        let a = contig_start;
        if starts[a] == chunk.origin[a]
            && lengths[a] == chunk.shape[a]
            && starts[a] == selection.start[a]
            && lengths[a] == selection.count[a]
        {
            contig_start -= 1;
        } else {
            break;
        }
    }
    let rows = checked_product(&lengths[..contig_start]).map_err(ChunkIoError::ScientificModel)?;
    let block_elements =
        checked_product(&lengths[contig_start..]).map_err(ChunkIoError::ScientificModel)?;
    let byte_count = usize::try_from(
        block_elements
            .checked_mul(item as u64)
            .ok_or(ChunkIoError::LengthOverflow)?,
    )
    .map_err(|_| ChunkIoError::LengthOverflow)?;
    let mut source_strides = vec![1u64; rank];
    let mut target_strides = vec![1u64; rank];
    for axis in (0..rank - 1).rev() {
        source_strides[axis] = source_strides[axis + 1]
            .checked_mul(chunk.shape[axis + 1])
            .ok_or(ChunkIoError::LengthOverflow)?;
        target_strides[axis] = target_strides[axis + 1]
            .checked_mul(selection.count[axis + 1])
            .ok_or(ChunkIoError::LengthOverflow)?;
    }
    for row in 0..rows {
        let mut remainder = row;
        let mut source_element = 0u64;
        let mut target_element = 0u64;
        for axis in (0..contig_start).rev() {
            let coordinate = if lengths[axis] == 0 {
                0
            } else {
                remainder % lengths[axis]
            };
            if let Some(quotient) = remainder.checked_div(lengths[axis]) {
                remainder = quotient;
            }
            let absolute = starts[axis]
                .checked_add(coordinate)
                .ok_or(ChunkIoError::LengthOverflow)?;
            source_element = source_element
                .checked_add(
                    (absolute - chunk.origin[axis])
                        .checked_mul(source_strides[axis])
                        .ok_or(ChunkIoError::LengthOverflow)?,
                )
                .ok_or(ChunkIoError::LengthOverflow)?;
            target_element = target_element
                .checked_add(
                    (absolute - selection.start[axis])
                        .checked_mul(target_strides[axis])
                        .ok_or(ChunkIoError::LengthOverflow)?,
                )
                .ok_or(ChunkIoError::LengthOverflow)?;
        }
        source_element = source_element
            .checked_add(
                (starts[contig_start] - chunk.origin[contig_start])
                    .checked_mul(source_strides[contig_start])
                    .ok_or(ChunkIoError::LengthOverflow)?,
            )
            .ok_or(ChunkIoError::LengthOverflow)?;
        target_element = target_element
            .checked_add(
                (starts[contig_start] - selection.start[contig_start])
                    .checked_mul(target_strides[contig_start])
                    .ok_or(ChunkIoError::LengthOverflow)?,
            )
            .ok_or(ChunkIoError::LengthOverflow)?;
        let source_start = usize::try_from(
            source_element
                .checked_mul(item as u64)
                .ok_or(ChunkIoError::LengthOverflow)?,
        )
        .map_err(|_| ChunkIoError::LengthOverflow)?;
        let target_start = usize::try_from(
            target_element
                .checked_mul(item as u64)
                .ok_or(ChunkIoError::LengthOverflow)?,
        )
        .map_err(|_| ChunkIoError::LengthOverflow)?;
        let source_end = source_start
            .checked_add(byte_count)
            .ok_or(ChunkIoError::LengthOverflow)?;
        let target_end = target_start
            .checked_add(byte_count)
            .ok_or(ChunkIoError::LengthOverflow)?;
        let source_slice =
            source
                .get(source_start..source_end)
                .ok_or(ChunkIoError::ScientificModel(
                    ScientificModelError::ChunkLengthMismatch,
                ))?;
        let target_slice =
            target
                .get_mut(target_start..target_end)
                .ok_or(ChunkIoError::ScientificModel(
                    ScientificModelError::ArrayTooLarge,
                ))?;
        target_slice.copy_from_slice(source_slice);
    }
    Ok(())
}

fn try_regular_grid_indices(
    variable: &Variable,
    dimension_lengths: &[u64],
    selection: &ArraySelection,
) -> Option<Vec<usize>> {
    let rank = dimension_lengths.len();
    if rank == 0 || variable.chunk_shape.len() != rank || variable.chunk_shape.contains(&0) {
        return None;
    }
    let mut chunks_per_dim = Vec::with_capacity(rank);
    let mut total_chunks = 1u64;
    for (&dim_len, &c_len) in dimension_lengths.iter().zip(&variable.chunk_shape) {
        let count = (dim_len.checked_add(c_len - 1)?) / c_len;
        chunks_per_dim.push(count);
        total_chunks = total_chunks.checked_mul(count)?;
    }
    if variable.chunks.len() as u64 != total_chunks {
        return None;
    }

    let mut strides = vec![1u64; rank];
    for d in (0..rank.saturating_sub(1)).rev() {
        strides[d] = strides[d + 1].checked_mul(chunks_per_dim[d + 1])?;
    }

    let mut min_indices = Vec::with_capacity(rank);
    let mut max_indices = Vec::with_capacity(rank);
    let mut total_k = 1usize;
    for ((&s, &c), (&chunk_step, &dim_chunks)) in selection
        .start
        .iter()
        .zip(&selection.count)
        .zip(variable.chunk_shape.iter().zip(&chunks_per_dim))
    {
        if c == 0 {
            return Some(Vec::new());
        }
        let i_min = s / chunk_step;
        let i_max = (s.checked_add(c - 1)?) / chunk_step;
        if i_max >= dim_chunks {
            return None;
        }
        let k_d = usize::try_from(i_max.checked_sub(i_min)?.checked_add(1)?).ok()?;
        total_k = total_k.checked_mul(k_d)?;
        min_indices.push(i_min);
        max_indices.push(i_max);
    }

    let mut indices = Vec::with_capacity(total_k);
    let mut current = min_indices.clone();
    loop {
        let mut linear_idx = 0u64;
        for (&cur, &stride) in current.iter().zip(&strides) {
            linear_idx = linear_idx.checked_add(cur.checked_mul(stride)?)?;
        }
        let usize_idx = usize::try_from(linear_idx).ok()?;
        let chunk = variable.chunks.get(usize_idx)?;
        for (&cur, (&c_shape, &origin)) in current
            .iter()
            .zip(variable.chunk_shape.iter().zip(&chunk.origin))
        {
            if origin != cur * c_shape {
                return None;
            }
        }
        indices.push(usize_idx);

        let mut dim = rank;
        while dim > 0 {
            dim -= 1;
            if current[dim] < max_indices[dim] {
                current[dim] += 1;
                break;
            } else {
                current[dim] = min_indices[dim];
            }
        }
        if dim == 0 && current[0] == min_indices[0] {
            break;
        }
    }
    Some(indices)
}

pub fn open_scientific_snapshot<R: Read + Seek>(
    reader: &mut R,
) -> Result<ScientificSnapshot, ChunkIoError> {
    open_scientific_snapshot_mode(reader, true)
}

pub fn open_scientific_snapshot_lazy<R: Read + Seek>(
    reader: &mut R,
) -> Result<ScientificSnapshot, ChunkIoError> {
    open_scientific_snapshot_mode(reader, false)
}

fn open_scientific_snapshot_mode<R: Read + Seek>(
    reader: &mut R,
    eager_validate: bool,
) -> Result<ScientificSnapshot, ChunkIoError> {
    let snapshot = if eager_validate {
        super::open_generation_snapshot(reader)?
    } else {
        super::open_generation_snapshot_lazy(reader)?
    };
    let dataset =
        snapshot
            .metadata()
            .scientific_model()
            .cloned()
            .ok_or(ChunkIoError::ScientificModel(
                ScientificModelError::MissingScientificModel,
            ))?;
    dataset
        .validate_descriptors(snapshot.descriptors())
        .map_err(ChunkIoError::ScientificModel)?;
    Ok(ScientificSnapshot { snapshot, dataset })
}

#[cfg(test)]
mod scientific_unit_tests {
    use super::{checked_array_output_bytes, ScientificModelError, MAX_ARRAY_READ_BYTES};

    #[test]
    fn array_output_limit_rejects_oversized_and_overflowing_shapes_before_allocation() {
        assert_eq!(
            checked_array_output_bytes(&[MAX_ARRAY_READ_BYTES + 1], 1),
            Err(ScientificModelError::ArrayTooLarge),
        );
        assert_eq!(
            checked_array_output_bytes(&[u64::MAX, 2], 1),
            Err(ScientificModelError::ArrayTooLarge),
        );
    }
}
