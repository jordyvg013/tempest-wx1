//! Safe parsing primitives for the independent WX-1 storage project.
//!
//! This crate implements experimental metadata containers and bounded chunk storage. The legacy prefix probe
//! below is retained solely for diagnostics and is not a normative format.

#![forbid(unsafe_code)]

use std::fmt;

mod scientific;
pub use scientific::{
    open_scientific_snapshot, open_scientific_snapshot_lazy, ArrayChunkRef, ArrayReadPlan,
    ArraySelection, AttributeValue, Coordinate, CoordinateAxis, DataType, Dimension, DimensionRole,
    GridMapping, Group, LongitudeConvention, PayloadByteOrder, Provenance, ScalarValue,
    ScientificArray, ScientificDataset, ScientificModelError, ScientificSnapshot, TimeCalendar,
    TimeCoordinate, TimePrecision, Variable, VerticalCoordinate, VerticalDirection, VerticalKind,
    MAX_ARRAY_READ_BYTES, MAX_SCIENTIFIC_RANK, SCIENTIFIC_SCHEMA_VERSION,
};

pub mod quantization;
pub use quantization::{
    byte_shuffle, byte_unshuffle, quantize_f32_slice, ErrorMetric, QuantizationConfig,
    QuantizationError, QuantizationProfile, QuantizationReport,
};

pub mod integer_packing;
pub use integer_packing::{
    decode_integer_packed, encode_integer_packed, IntegerPackedBuffer, IntegerPredictor,
};

pub mod staging;
pub use staging::{
    append_staged_scientific_generation, append_staged_scientific_generation_file, stage_chunk,
    stage_chunks_parallel, write_staged_generation_container,
    write_staged_generation_container_atomic, AsyncScientificWriter, AsyncWriterOptions,
    AsyncWriterTelemetry, StagedChunk, WriteDurability, MAX_STAGING_WORKERS,
};

pub mod topology;
pub use topology::{
    CubedSphereTopology, EdgeNeighbor, GnomonicProjection, GridFamily, GridTopologyDescriptor,
    HaloConsistencyState, HaloSpec, HorizontalStagger, NestingDescriptor, PanelConnectivity,
    PanelEdge, ReducedGaussianTopology, VariableGridLayout, VerticalStagger,
};

/// Explicit byte order for on-disk fixed-width numeric fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteOrder {
    LittleEndian,
    BigEndian,
}

/// Bounds-checked primitive decoder. It never allocates and never assumes host endianness.
#[derive(Debug, Clone, Copy)]
pub struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
    order: ByteOrder,
}
impl<'a> Decoder<'a> {
    /// Creates a decoder over a bounded byte slice.
    pub fn new(bytes: &'a [u8], order: ByteOrder) -> Self {
        Self {
            bytes,
            offset: 0,
            order,
        }
    }
    /// Current byte offset.
    pub fn position(&self) -> usize {
        self.offset
    }
    /// Number of unread bytes.
    pub fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }
    fn take<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or(DecodeError::OffsetOverflow)?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or(DecodeError::UnexpectedEof)?;
        let out: [u8; N] = slice.try_into().map_err(|_| DecodeError::UnexpectedEof)?;
        self.offset = end;
        Ok(out)
    }
    /// Reads an unsigned 8-bit integer. Byte order does not affect a single byte.
    pub fn read_u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take::<1>()?[0])
    }
    /// Reads an unsigned 16-bit integer.
    pub fn read_u16(&mut self) -> Result<u16, DecodeError> {
        let b = self.take::<2>()?;
        Ok(match self.order {
            ByteOrder::LittleEndian => u16::from_le_bytes(b),
            ByteOrder::BigEndian => u16::from_be_bytes(b),
        })
    }
    /// Reads an unsigned 32-bit integer.
    pub fn read_u32(&mut self) -> Result<u32, DecodeError> {
        let b = self.take::<4>()?;
        Ok(match self.order {
            ByteOrder::LittleEndian => u32::from_le_bytes(b),
            ByteOrder::BigEndian => u32::from_be_bytes(b),
        })
    }
    /// Reads an unsigned 64-bit integer.
    pub fn read_u64(&mut self) -> Result<u64, DecodeError> {
        let b = self.take::<8>()?;
        Ok(match self.order {
            ByteOrder::LittleEndian => u64::from_le_bytes(b),
            ByteOrder::BigEndian => u64::from_be_bytes(b),
        })
    }
    /// Reads an IEEE-754 binary32 bit pattern.
    pub fn read_f32(&mut self) -> Result<f32, DecodeError> {
        Ok(f32::from_bits(self.read_u32()?))
    }
    /// Reads an IEEE-754 binary64 bit pattern.
    pub fn read_f64(&mut self) -> Result<f64, DecodeError> {
        Ok(f64::from_bits(self.read_u64()?))
    }
}

/// Failure while decoding bounded fixed-width values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    UnexpectedEof,
    OffsetOverflow,
}
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of input"),
            Self::OffsetOverflow => write!(f, "decoder offset overflow"),
        }
    }
}
impl std::error::Error for DecodeError {}

/// Experimental byte-order marker used by the draft container header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HeaderByteOrder {
    LittleEndian = 1,
    BigEndian = 2,
}

impl HeaderByteOrder {
    fn from_marker(value: u8) -> Result<Self, ContainerError> {
        match value {
            1 => Ok(Self::LittleEndian),
            2 => Ok(Self::BigEndian),
            _ => Err(ContainerError::InvalidByteOrder),
        }
    }
    fn decoder(self, bytes: &[u8]) -> Decoder<'_> {
        Decoder::new(
            bytes,
            match self {
                Self::LittleEndian => ByteOrder::LittleEndian,
                Self::BigEndian => ByteOrder::BigEndian,
            },
        )
    }
}

/// Draft WX-1 container header. This layout is experimental and is not yet a
/// stable interoperability contract; see ADR 0009 before using it in files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerHeader {
    pub major: u16,
    pub minor: u16,
    pub byte_order: HeaderByteOrder,
    pub flags: u8,
    pub metadata_len: u64,
    pub directory_offset: u64,
}

pub const CONTAINER_MAGIC: [u8; 4] = *b"WX1\0";
pub const CONTAINER_HEADER_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerError {
    UnexpectedEof,
    InvalidMagic,
    InvalidByteOrder,
    UnsupportedMajor(u16),
    InvalidHeaderLength,
    UnknownFlags(u8),
    NonZeroReserved,
    LengthOverflow,
    MetadataTooLarge,
    DirectoryBeforeMetadata,
    OffsetOutsideFile,
    DirectoryNotSupported,
    TrailingFileData,
}

impl fmt::Display for ContainerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WX-1 container error: {self:?}")
    }
}
impl std::error::Error for ContainerError {}

impl ContainerHeader {
    /// Encodes the experimental fixed 32-byte header.
    pub fn encode(self) -> [u8; CONTAINER_HEADER_LEN] {
        let mut out = [0u8; CONTAINER_HEADER_LEN];
        out[..4].copy_from_slice(&CONTAINER_MAGIC);
        let (major, minor, metadata_len, directory_offset) = match self.byte_order {
            HeaderByteOrder::LittleEndian => (
                self.major.to_le_bytes(),
                self.minor.to_le_bytes(),
                self.metadata_len.to_le_bytes(),
                self.directory_offset.to_le_bytes(),
            ),
            HeaderByteOrder::BigEndian => (
                self.major.to_be_bytes(),
                self.minor.to_be_bytes(),
                self.metadata_len.to_be_bytes(),
                self.directory_offset.to_be_bytes(),
            ),
        };
        out[4..6].copy_from_slice(&major);
        out[6..8].copy_from_slice(&minor);
        out[8] = self.byte_order as u8;
        out[9] = self.flags;
        let header_len: u16 = CONTAINER_HEADER_LEN as u16;
        let header_len = match self.byte_order {
            HeaderByteOrder::LittleEndian => header_len.to_le_bytes(),
            HeaderByteOrder::BigEndian => header_len.to_be_bytes(),
        };
        out[10..12].copy_from_slice(&header_len);
        out[12..20].copy_from_slice(&metadata_len);
        out[20..28].copy_from_slice(&directory_offset);
        out
    }

    /// Decodes and validates a complete experimental fixed header.
    pub fn decode(bytes: &[u8]) -> Result<Self, ContainerError> {
        if bytes.len() < CONTAINER_HEADER_LEN {
            return Err(ContainerError::UnexpectedEof);
        }
        if bytes[..4] != CONTAINER_MAGIC {
            return Err(ContainerError::InvalidMagic);
        }
        let order = HeaderByteOrder::from_marker(bytes[8])?;
        let mut d = order.decoder(&bytes[4..]);
        let major = d.read_u16().map_err(|_| ContainerError::UnexpectedEof)?;
        let minor = d.read_u16().map_err(|_| ContainerError::UnexpectedEof)?;
        let _marker = d.read_u8().map_err(|_| ContainerError::UnexpectedEof)?;
        let flags = d.read_u8().map_err(|_| ContainerError::UnexpectedEof)?;
        let header_len = d.read_u16().map_err(|_| ContainerError::UnexpectedEof)?;
        let metadata_len = d.read_u64().map_err(|_| ContainerError::UnexpectedEof)?;
        let directory_offset = d.read_u64().map_err(|_| ContainerError::UnexpectedEof)?;
        let reserved = d.read_u32().map_err(|_| ContainerError::UnexpectedEof)?;
        if major != 1 {
            return Err(ContainerError::UnsupportedMajor(major));
        }
        if header_len as usize != CONTAINER_HEADER_LEN {
            return Err(ContainerError::InvalidHeaderLength);
        }
        if flags != 0 {
            return Err(ContainerError::UnknownFlags(flags));
        }
        if reserved != 0 {
            return Err(ContainerError::NonZeroReserved);
        }
        Ok(Self {
            major,
            minor,
            byte_order: order,
            flags,
            metadata_len,
            directory_offset,
        })
    }

    /// Checks that metadata and directory ranges fit inside a known file size.
    pub fn validate_file_bounds(self, file_len: u64) -> Result<(), ContainerError> {
        let metadata_end = (CONTAINER_HEADER_LEN as u64)
            .checked_add(self.metadata_len)
            .ok_or(ContainerError::LengthOverflow)?;
        if metadata_end > file_len {
            return Err(ContainerError::MetadataTooLarge);
        }
        if self.directory_offset != 0 && self.directory_offset < metadata_end {
            return Err(ContainerError::DirectoryBeforeMetadata);
        }
        if self.directory_offset > file_len {
            return Err(ContainerError::OffsetOutsideFile);
        }
        Ok(())
    }
}

/// Maximum metadata payload accepted by the experimental reader (32 MiB, ADR 0034).
pub const MAX_METADATA_BYTES: u64 = 32 * 1024 * 1024;

/// Versioned WX-1 metadata envelope.
///
/// Schema v1 retains the original three-entry deterministic-CBOR map. Schema v2
/// adds a separately versioned scientific model as a bounded CBOR byte string.
/// Scientific metadata is accepted only inside the generation-container profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// Stable identifier assigned by the data producer.
    pub dataset_id: String,
    /// Extensible string-valued dataset attributes retained by schema v1.
    pub attributes: std::collections::BTreeMap<String, String>,
    /// Optional schema-v2 scientific model. Existing schema-v1 metadata remains unchanged when absent.
    scientific_model: Option<ScientificDataset>,
}

impl Metadata {
    /// Creates metadata with no additional attributes.
    pub fn new(dataset_id: impl Into<String>) -> Self {
        Self {
            dataset_id: dataset_id.into(),
            attributes: std::collections::BTreeMap::new(),
            scientific_model: None,
        }
    }

    /// Attaches a validated-at-serialization scientific dataset model (metadata schema v2).
    pub fn with_scientific_model(mut self, model: ScientificDataset) -> Self {
        self.scientific_model = Some(model);
        self
    }

    /// Returns the optional scientific dataset model.
    pub fn scientific_model(&self) -> Option<&ScientificDataset> {
        self.scientific_model.as_ref()
    }

    /// Encodes this metadata using deterministic CBOR schema v1 or v2.
    pub fn to_cbor(&self) -> Result<Vec<u8>, MetadataError> {
        if self.dataset_id.is_empty() {
            return Err(MetadataError::EmptyDatasetId);
        }
        if self.attributes.len() > 100_000 {
            return Err(MetadataError::TooManyAttributes);
        }
        let mut estimated = 32usize
            .checked_add(cbor_text_len(&self.dataset_id))
            .ok_or(MetadataError::TooLarge)?;
        if estimated as u64 > MAX_METADATA_BYTES {
            return Err(MetadataError::TooLarge);
        }
        for (key, value) in &self.attributes {
            estimated = estimated
                .checked_add(cbor_text_len(key))
                .and_then(|n| n.checked_add(cbor_text_len(value)))
                .ok_or(MetadataError::TooLarge)?;
            if estimated as u64 > MAX_METADATA_BYTES {
                return Err(MetadataError::TooLarge);
            }
        }
        let scientific_bytes = self
            .scientific_model
            .as_ref()
            .map(ScientificDataset::to_cbor)
            .transpose()
            .map_err(|_| MetadataError::InvalidScientificModel)?;
        let extra = scientific_bytes
            .as_ref()
            .map_or(0, |bytes| cbor_byte_string_len(bytes.len()));
        estimated = estimated
            .checked_add(extra)
            .ok_or(MetadataError::TooLarge)?;
        if estimated as u64 > MAX_METADATA_BYTES {
            return Err(MetadataError::TooLarge);
        }
        let mut out = Vec::with_capacity(estimated);
        let schema_version = if scientific_bytes.is_some() { 2 } else { 1 };
        cbor_head(&mut out, 5, if scientific_bytes.is_some() { 4 } else { 3 });
        cbor_head(&mut out, 0, 0);
        cbor_head(&mut out, 0, schema_version);
        cbor_head(&mut out, 0, 1);
        cbor_text(&mut out, &self.dataset_id);
        cbor_head(&mut out, 0, 2);
        cbor_head(&mut out, 5, self.attributes.len() as u64);
        let mut entries: Vec<_> = self.attributes.iter().collect();
        entries.sort_by(|(a, _), (b, _)| canonical_text_cmp(a, b));
        for (key, value) in entries {
            cbor_text(&mut out, key);
            cbor_text(&mut out, value);
        }
        if let Some(scientific) = scientific_bytes {
            cbor_head(&mut out, 0, 3);
            cbor_head(&mut out, 2, scientific.len() as u64);
            out.extend_from_slice(&scientific);
        }
        if out.len() as u64 > MAX_METADATA_BYTES {
            return Err(MetadataError::TooLarge);
        }
        Ok(out)
    }

    /// Decodes and strictly validates supported deterministic CBOR metadata schemas.
    pub fn from_cbor(bytes: &[u8]) -> Result<Self, MetadataError> {
        if bytes.len() as u64 > MAX_METADATA_BYTES {
            return Err(MetadataError::TooLarge);
        }
        let mut d = CborDecoder { bytes, pos: 0 };
        let map_count = d.head(5)?;
        if map_count != 3 && map_count != 4 {
            return Err(MetadataError::InvalidSchema);
        }
        if d.head(0)? != 0 {
            return Err(MetadataError::InvalidSchema);
        }
        let schema_version = d.head(0)?;
        if (schema_version == 1 && map_count != 3)
            || (schema_version == 2 && map_count != 4)
            || (schema_version != 1 && schema_version != 2)
        {
            return Err(MetadataError::InvalidSchema);
        }
        if d.head(0)? != 1 {
            return Err(MetadataError::InvalidSchema);
        }
        let dataset_id = d.text()?;
        if dataset_id.is_empty() {
            return Err(MetadataError::EmptyDatasetId);
        }
        if d.head(0)? != 2 {
            return Err(MetadataError::InvalidSchema);
        }
        let count = d.head(5)?;
        if count > 100_000 {
            return Err(MetadataError::TooManyAttributes);
        }
        let mut attributes = std::collections::BTreeMap::new();
        let mut previous: Option<String> = None;
        for _ in 0..count {
            let key = d.text()?;
            if let Some(prev) = &previous {
                if canonical_text_cmp(prev, &key) != std::cmp::Ordering::Less {
                    return Err(MetadataError::NonCanonical);
                }
            }
            let value = d.text()?;
            if attributes.insert(key.clone(), value).is_some() {
                return Err(MetadataError::DuplicateAttribute);
            }
            previous = Some(key);
        }
        let scientific_model = if schema_version == 2 {
            if d.head(0)? != 3 {
                return Err(MetadataError::InvalidSchema);
            }
            let model_bytes = d.byte_string()?;
            Some(
                ScientificDataset::from_cbor(&model_bytes)
                    .map_err(|_| MetadataError::InvalidScientificModel)?,
            )
        } else {
            None
        };
        if d.pos != bytes.len() {
            return Err(MetadataError::TrailingData);
        }
        Ok(Self {
            dataset_id,
            attributes,
            scientific_model,
        })
    }
}

/// Metadata serialization and validation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataError {
    InvalidSchema,
    InvalidCbor,
    NonCanonical,
    DuplicateAttribute,
    EmptyDatasetId,
    TooManyAttributes,
    TooLarge,
    TrailingData,
    InvalidScientificModel,
    ScientificModelRequiresGeneration,
}
impl fmt::Display for MetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WX-1 metadata error: {self:?}")
    }
}
impl std::error::Error for MetadataError {}

fn canonical_text_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let al = cbor_text_len(a);
    let bl = cbor_text_len(b);
    al.cmp(&bl).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}
fn cbor_text_len(s: &str) -> usize {
    let n = s.len();
    if n < 24 {
        1 + n
    } else if n <= u8::MAX as usize {
        2 + n
    } else if n <= u16::MAX as usize {
        3 + n
    } else if (n as u128) <= u32::MAX as u128 {
        5 + n
    } else {
        9 + n
    }
}
fn cbor_byte_string_len(n: usize) -> usize {
    if n < 24 {
        1 + n
    } else if n <= u8::MAX as usize {
        2 + n
    } else if n <= u16::MAX as usize {
        3 + n
    } else if (n as u128) <= u32::MAX as u128 {
        5 + n
    } else {
        9 + n
    }
}
const U32_MAX_U64: u64 = u32::MAX as u64;

fn cbor_head(out: &mut Vec<u8>, major: u8, value: u64) {
    let prefix = major << 5;
    match value {
        0..=23 => out.push(prefix | value as u8),
        24..=255 => {
            out.push(prefix | 24);
            out.push(value as u8);
        }
        256..=65535 => {
            out.push(prefix | 25);
            out.extend_from_slice(&(value as u16).to_be_bytes());
        }
        65536..=U32_MAX_U64 => {
            out.push(prefix | 26);
            out.extend_from_slice(&(value as u32).to_be_bytes());
        }
        _ => {
            out.push(prefix | 27);
            out.extend_from_slice(&value.to_be_bytes());
        }
    }
}
fn cbor_text(out: &mut Vec<u8>, value: &str) {
    cbor_head(out, 3, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}
struct CborDecoder<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> CborDecoder<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], MetadataError> {
        let end = self.pos.checked_add(n).ok_or(MetadataError::InvalidCbor)?;
        let b = self
            .bytes
            .get(self.pos..end)
            .ok_or(MetadataError::InvalidCbor)?;
        self.pos = end;
        Ok(b)
    }
    fn head(&mut self, expected_major: u8) -> Result<u64, MetadataError> {
        let first = *self.take(1)?.first().ok_or(MetadataError::InvalidCbor)?;
        if first >> 5 != expected_major {
            return Err(MetadataError::InvalidSchema);
        }
        let ai = first & 31;
        let (value, min) = match ai {
            0..=23 => (ai as u64, 0),
            24 => (self.take(1)?[0] as u64, 24),
            25 => (
                u16::from_be_bytes(
                    self.take(2)?
                        .try_into()
                        .map_err(|_| MetadataError::InvalidCbor)?,
                ) as u64,
                256,
            ),
            26 => (
                u32::from_be_bytes(
                    self.take(4)?
                        .try_into()
                        .map_err(|_| MetadataError::InvalidCbor)?,
                ) as u64,
                65536,
            ),
            27 => (
                u64::from_be_bytes(
                    self.take(8)?
                        .try_into()
                        .map_err(|_| MetadataError::InvalidCbor)?,
                ),
                u32::MAX as u64 + 1,
            ),
            _ => return Err(MetadataError::InvalidCbor),
        };
        if value < min {
            return Err(MetadataError::NonCanonical);
        }
        Ok(value)
    }
    fn text(&mut self) -> Result<String, MetadataError> {
        let n = usize::try_from(self.head(3)?).map_err(|_| MetadataError::TooLarge)?;
        if n as u64 > MAX_METADATA_BYTES {
            return Err(MetadataError::TooLarge);
        }
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| MetadataError::InvalidCbor)
    }
    fn byte_string(&mut self) -> Result<Vec<u8>, MetadataError> {
        let n = usize::try_from(self.head(2)?).map_err(|_| MetadataError::TooLarge)?;
        if n as u64 > MAX_METADATA_BYTES {
            return Err(MetadataError::TooLarge);
        }
        Ok(self.take(n)?.to_vec())
    }
}

/// Errors returned by the experimental container reader and writer.
#[derive(Debug)]
pub enum ContainerIoError {
    Io(std::io::Error),
    Header(ContainerError),
    Metadata(MetadataError),
}
impl fmt::Display for ContainerIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Header(e) => write!(f, "{e}"),
            Self::Metadata(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for ContainerIoError {}
impl From<std::io::Error> for ContainerIoError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Writes a metadata-only experimental WX-1 container to a stream.
/// The stream is not truncated automatically; callers creating files should
/// use a newly created file or truncate it before calling this function.
pub fn write_container<W: std::io::Write>(
    writer: &mut W,
    metadata: &Metadata,
    order: HeaderByteOrder,
) -> Result<(), ContainerIoError> {
    if metadata.scientific_model().is_some() {
        return Err(ContainerIoError::Metadata(
            MetadataError::ScientificModelRequiresGeneration,
        ));
    }
    let payload = metadata.to_cbor().map_err(ContainerIoError::Metadata)?;
    let header = ContainerHeader {
        major: 1,
        minor: 1,
        byte_order: order,
        flags: 0,
        metadata_len: payload.len() as u64,
        directory_offset: 0,
    };
    writer.write_all(&header.encode())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

/// Reads and validates a metadata-only experimental WX-1 container.
/// The complete file length is checked before metadata allocation.
pub fn read_container<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
) -> Result<Metadata, ContainerIoError> {
    use std::io::SeekFrom;
    let file_len = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut raw = [0u8; CONTAINER_HEADER_LEN];
    reader.read_exact(&mut raw)?;
    let header = ContainerHeader::decode(&raw).map_err(ContainerIoError::Header)?;
    header
        .validate_file_bounds(file_len)
        .map_err(ContainerIoError::Header)?;
    if header.directory_offset != 0 {
        return Err(ContainerIoError::Header(
            ContainerError::DirectoryNotSupported,
        ));
    }
    let expected_len = (CONTAINER_HEADER_LEN as u64)
        .checked_add(header.metadata_len)
        .ok_or(ContainerIoError::Header(ContainerError::LengthOverflow))?;
    if expected_len != file_len {
        return Err(ContainerIoError::Header(ContainerError::TrailingFileData));
    }
    if header.metadata_len > MAX_METADATA_BYTES {
        return Err(ContainerIoError::Metadata(MetadataError::TooLarge));
    }
    let n = usize::try_from(header.metadata_len)
        .map_err(|_| ContainerIoError::Metadata(MetadataError::TooLarge))?;
    let mut payload = vec![0u8; n];
    reader.read_exact(&mut payload)?;
    Metadata::from_cbor(&payload).map_err(ContainerIoError::Metadata)
}

/// Legacy experimental signature. Not the final WX-1 file magic.
pub const LEGACY_EXPERIMENTAL_MAGIC: [u8; 4] = *b"WX1X";
/// Number of bytes required by the legacy prefix probe.
pub const LEGACY_PREFIX_LEN: usize = 8;

/// Failure while inspecting the legacy experimental prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixError {
    /// Fewer than eight bytes were supplied.
    TooShort,
    /// The four-byte legacy signature did not match.
    InvalidMagic,
    /// The little-endian header length is smaller than the prefix itself.
    InvalidHeaderLength,
}

impl fmt::Display for PrefixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => write!(f, "legacy prefix is shorter than 8 bytes"),
            Self::InvalidMagic => write!(f, "legacy experimental signature does not match"),
            Self::InvalidHeaderLength => write!(f, "legacy header length is smaller than 8 bytes"),
        }
    }
}

impl std::error::Error for PrefixError {}

/// Information extracted from the non-normative legacy prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyPrefix {
    /// Header length encoded in the legacy prefix.
    pub header_len: u32,
}

/// Inspects the old eight-byte experimental prefix without reading a file.
///
/// This function only checks the signature and minimum header length. It does
/// not validate a complete file, offsets, metadata, payloads, checksums, or a
/// commit marker. Do not use it as a general WX-1 validator.
///
/// # Errors
/// Returns [`PrefixError`] for truncated input, an unexpected signature, or
/// a header length smaller than the prefix.
pub fn inspect_legacy_prefix(bytes: &[u8]) -> Result<LegacyPrefix, PrefixError> {
    if bytes.len() < LEGACY_PREFIX_LEN {
        return Err(PrefixError::TooShort);
    }
    if bytes[..4] != LEGACY_EXPERIMENTAL_MAGIC {
        return Err(PrefixError::InvalidMagic);
    }
    let raw_len: [u8; 4] = bytes[4..8].try_into().map_err(|_| PrefixError::TooShort)?;
    let header_len = u32::from_le_bytes(raw_len);
    if header_len < 8_u32 {
        return Err(PrefixError::InvalidHeaderLength);
    }
    Ok(LegacyPrefix { header_len })
}

#[cfg(test)]
mod tests {
    use super::{inspect_legacy_prefix, LegacyPrefix, PrefixError, LEGACY_EXPERIMENTAL_MAGIC};

    fn prefix(length: u32) -> Vec<u8> {
        let mut bytes = LEGACY_EXPERIMENTAL_MAGIC.to_vec();
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes
    }

    #[test]
    fn inspects_minimal_prefix() {
        assert_eq!(
            inspect_legacy_prefix(&prefix(8)),
            Ok(LegacyPrefix { header_len: 8 })
        );
    }

    #[test]
    fn inspects_larger_header_length() {
        assert_eq!(
            inspect_legacy_prefix(&prefix(4096)),
            Ok(LegacyPrefix { header_len: 4096 })
        );
    }

    #[test]
    fn rejects_every_truncated_prefix_length() {
        let bytes = prefix(8);
        for length in 0..8 {
            assert_eq!(
                inspect_legacy_prefix(&bytes[..length]),
                Err(PrefixError::TooShort)
            );
        }
    }

    #[test]
    fn rejects_wrong_signature() {
        assert_eq!(
            inspect_legacy_prefix(b"NOPE\x08\x00\x00\x00"),
            Err(PrefixError::InvalidMagic)
        );
    }

    #[test]
    fn rejects_header_length_below_prefix() {
        assert_eq!(
            inspect_legacy_prefix(&prefix(7)),
            Err(PrefixError::InvalidHeaderLength)
        );
    }
}

#[cfg(test)]
mod primitive_tests {
    use super::{ByteOrder, DecodeError, Decoder};
    #[test]
    fn decodes_little_and_big_endian() {
        assert_eq!(
            Decoder::new(&[0x34, 0x12], ByteOrder::LittleEndian).read_u16(),
            Ok(0x1234)
        );
        assert_eq!(
            Decoder::new(&[0x12, 0x34], ByteOrder::BigEndian).read_u16(),
            Ok(0x1234)
        );
    }
    #[test]
    fn rejects_truncated_value_without_advancing() {
        let mut d = Decoder::new(&[1, 2, 3], ByteOrder::LittleEndian);
        assert_eq!(d.read_u32(), Err(DecodeError::UnexpectedEof));
        assert_eq!(d.position(), 0);
    }
    #[test]
    fn decodes_float_bit_patterns() {
        let bytes = 1.5f32.to_le_bytes();
        let mut decoder = Decoder::new(&bytes, ByteOrder::LittleEndian);
        assert_eq!(decoder.read_f32(), Ok(1.5));
        assert_eq!(decoder.position(), 4);
    }
}

#[cfg(test)]
mod container_header_tests {
    use super::{ContainerError, ContainerHeader, HeaderByteOrder};

    fn sample(order: HeaderByteOrder) -> ContainerHeader {
        ContainerHeader {
            major: 1,
            minor: 0,
            byte_order: order,
            flags: 0,
            metadata_len: 128,
            directory_offset: 256,
        }
    }

    #[test]
    fn header_roundtrips_both_byte_orders() {
        for order in [HeaderByteOrder::LittleEndian, HeaderByteOrder::BigEndian] {
            let header = sample(order);
            assert_eq!(ContainerHeader::decode(&header.encode()), Ok(header));
        }
    }

    #[test]
    fn rejects_truncated_header() {
        let bytes = sample(HeaderByteOrder::LittleEndian).encode();
        for n in 0..32 {
            assert_eq!(
                ContainerHeader::decode(&bytes[..n]),
                Err(ContainerError::UnexpectedEof)
            );
        }
    }

    #[test]
    fn rejects_unknown_flags_and_nonzero_reserved() {
        let mut bytes = sample(HeaderByteOrder::LittleEndian).encode();
        bytes[9] = 1;
        assert_eq!(
            ContainerHeader::decode(&bytes),
            Err(ContainerError::UnknownFlags(1))
        );
        bytes[9] = 0;
        bytes[28] = 1;
        assert_eq!(
            ContainerHeader::decode(&bytes),
            Err(ContainerError::NonZeroReserved)
        );
    }

    #[test]
    fn validates_file_ranges_without_overflow() {
        let header = sample(HeaderByteOrder::LittleEndian);
        assert_eq!(header.validate_file_bounds(384), Ok(()));
        assert_eq!(
            header.validate_file_bounds(100),
            Err(ContainerError::MetadataTooLarge)
        );
        let invalid = ContainerHeader {
            directory_offset: 100,
            ..header
        };
        assert_eq!(
            invalid.validate_file_bounds(384),
            Err(ContainerError::DirectoryBeforeMetadata)
        );
        let overflow = ContainerHeader {
            metadata_len: u64::MAX,
            ..header
        };
        assert_eq!(
            overflow.validate_file_bounds(u64::MAX),
            Err(ContainerError::LengthOverflow)
        );
    }
}

#[cfg(test)]
mod container_io_tests {
    use super::{read_container, write_container, HeaderByteOrder, Metadata, MetadataError};
    use std::io::Cursor;

    #[test]
    fn metadata_cbor_is_deterministic_and_roundtrips() {
        let mut metadata = Metadata::new("ecmwf-20261030-00z");
        metadata
            .attributes
            .insert("title".into(), "Forecast".into());
        metadata.attributes.insert("centre".into(), "ecmwf".into());
        let a = metadata.to_cbor().unwrap();
        let b = metadata.to_cbor().unwrap();
        assert_eq!(a, b);
        assert_eq!(Metadata::from_cbor(&a), Ok(metadata));
    }

    #[test]
    fn metadata_rejects_noncanonical_and_trailing_bytes() {
        assert_eq!(
            Metadata::from_cbor(&[0xa3, 0x00, 0x01, 0x01, 0x61, b'x', 0x02, 0xa0, 0x00]),
            Err(MetadataError::TrailingData)
        );
        assert_eq!(
            Metadata::from_cbor(&[0xa3, 0x00, 0x18, 0x01, 0x01, 0x61, b'x', 0x02, 0xa0]),
            Err(MetadataError::NonCanonical)
        );
        assert_eq!(
            Metadata::from_cbor(&[
                0xa3, 0x00, 0x01, 0x01, 0x61, b'x', 0x02, 0xa2, 0x61, b'b', 0x61, b'2', 0x61, b'a',
                0x61, b'1'
            ]),
            Err(MetadataError::NonCanonical)
        );
        assert!(Metadata::from_cbor(&[0xbf]).is_err());
    }

    #[test]
    fn metadata_enforces_size_limit_before_encoding() {
        let metadata = Metadata::new("x".repeat(super::MAX_METADATA_BYTES as usize));
        assert_eq!(metadata.to_cbor().unwrap_err(), MetadataError::TooLarge);
    }

    #[test]
    fn file_roundtrips_both_byte_orders() {
        let mut metadata = Metadata::new("dataset-001");
        metadata.attributes.insert("source".into(), "KNMI".into());
        for order in [HeaderByteOrder::LittleEndian, HeaderByteOrder::BigEndian] {
            let mut cursor = Cursor::new(Vec::new());
            write_container(&mut cursor, &metadata, order).unwrap();
            cursor.set_position(0);
            assert_eq!(read_container(&mut cursor).unwrap(), metadata);
        }
    }

    #[test]
    fn file_reader_rejects_truncation_and_trailing_bytes() {
        let metadata = Metadata::new("dataset-002");
        let mut cursor = Cursor::new(Vec::new());
        write_container(&mut cursor, &metadata, HeaderByteOrder::LittleEndian).unwrap();
        let mut bytes = cursor.into_inner();
        bytes.pop();
        assert!(read_container(&mut Cursor::new(bytes)).is_err());
        let mut cursor = Cursor::new(Vec::new());
        write_container(&mut cursor, &metadata, HeaderByteOrder::LittleEndian).unwrap();
        let mut bytes = cursor.into_inner();
        bytes.push(0);
        assert!(read_container(&mut Cursor::new(bytes)).is_err());
    }

    #[test]
    fn cbor_unsigned_integer_encoder_uses_shortest_width_at_boundaries() {
        let cases: &[(u64, &[u8])] = &[
            (23, &[0x17]),
            (24, &[0x18, 0x18]),
            (255, &[0x18, 0xff]),
            (256, &[0x19, 0x01, 0x00]),
            (65_535, &[0x19, 0xff, 0xff]),
            (65_536, &[0x1a, 0x00, 0x01, 0x00, 0x00]),
            (u32::MAX as u64, &[0x1a, 0xff, 0xff, 0xff, 0xff]),
            (
                u32::MAX as u64 + 1,
                &[0x1b, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00],
            ),
            (
                u64::MAX,
                &[0x1b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
            ),
        ];
        for (value, expected) in cases {
            let mut encoded = Vec::new();
            super::cbor_head(&mut encoded, 0, *value);
            assert_eq!(encoded, *expected, "value {value}");
        }
    }
}

// Experimental v0.3.5 raw chunk directory. This remains a draft wire layout.
pub const CHUNK_DIRECTORY_MAGIC: [u8; 4] = *b"WX1D";
pub const CHUNK_DIRECTORY_HEADER_LEN: u64 = 16;
pub const CHUNK_DESCRIPTOR_LEN: u64 = 40;
pub const MAX_CHUNKS: u64 = 1_048_576;
pub const MAX_CHUNK_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum stored representation size, allowing modest codec framing overhead.
pub const MAX_STORED_CHUNK_BYTES: u64 = MAX_CHUNK_BYTES + 1024 * 1024;

/// Lossless chunk codec identifiers used by the experimental directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ChunkCodec {
    /// Store bytes without compression.
    Raw = 0,
    /// Zstandard frame compression.
    Zstandard = 1,
    /// LZ4 block compression.
    Lz4 = 2,
    /// 4-byte shuffle filter followed by Zstandard frame compression.
    ZstdByteShuffle4 = 3,
    /// 8-byte shuffle filter followed by Zstandard frame compression.
    ZstdByteShuffle8 = 4,
    /// 2-byte shuffle filter followed by Zstandard frame compression.
    ZstdByteShuffle2 = 5,
}

impl TryFrom<u8> for ChunkCodec {
    type Error = ChunkIoError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Raw),
            1 => Ok(Self::Zstandard),
            2 => Ok(Self::Lz4),
            3 => Ok(Self::ZstdByteShuffle4),
            4 => Ok(Self::ZstdByteShuffle8),
            5 => Ok(Self::ZstdByteShuffle2),
            _ => Err(ChunkIoError::InvalidDirectory),
        }
    }
}

/// A raw, uncompressed chunk supplied to the experimental container writer.
#[derive(Debug, Clone, Copy)]
pub struct ChunkInput<'a> {
    /// Unique identifier within this container.
    pub id: u64,
    /// Uncompressed chunk bytes.
    pub data: &'a [u8],
}

/// Chunk input with an explicitly selected lossless codec.
#[derive(Debug, Clone, Copy)]
pub struct EncodedChunkInput<'a> {
    /// Unique identifier within this container.
    pub id: u64,
    /// Uncompressed chunk bytes (maximum 64 MiB).
    pub data: &'a [u8],
    /// Codec to apply; lossy codecs are not available.
    pub codec: ChunkCodec,
}

/// A validated descriptor read from the experimental chunk directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkDescriptor {
    /// Unique identifier within the container.
    pub id: u64,
    /// Absolute byte offset of the stored payload.
    pub offset: u64,
    /// Number of bytes stored on disk.
    pub stored_len: u64,
    /// Number of bytes after decoding; equal to stored length for raw codec.
    pub raw_len: u64,
    /// CRC32C (Castagnoli) of the stored payload.
    pub crc32c: u32,
    /// Lossless codec identifier.
    pub codec: ChunkCodec,
    /// BLAKE3 digest of stored payload bytes for directory v2; `None` for v1 descriptors.
    pub blake3: Option<[u8; 32]>,
}

/// Errors returned by the experimental chunk-directory reader/writer.
#[derive(Debug)]
pub enum ChunkIoError {
    Io(std::io::Error),
    Header(ContainerError),
    Metadata(MetadataError),
    InvalidDirectory,
    CommitFooterInvalid,
    CommitHashMismatch,
    TooManyChunks,
    ChunkTooLarge,
    InvalidStreamBufferSize,
    AllocationFailed,
    DuplicateChunkId,
    ChunkNotFound(u64),
    ChecksumMismatch(u64),
    Compression(String),
    Decompression(String),
    LengthMismatch(u64),
    LengthOverflow,
    DirectoryMagicMismatch,
    DirectoryVersionUnsupported(u16),
    DirectoryEntrySizeInvalid,
    DirectoryReservedNonZero,
    DirectoryBoundsInvalid,
    DirectoryOffsetInvalid,
    DirectoryOverlap,
    DirectoryDuplicateChunkId,
    DirectoryDescriptorMismatch,
    UnsupportedCodec(u8),
    ChunkHashMismatch(u64),
    GenerationFooterInvalid,
    GenerationHashMismatch,
    GenerationChainInvalid,
    GenerationHeaderDirectoryOffsetNonZero,
    UnsupportedGenerationVersion(u16),
    NoValidGeneration,
    UnrepairedTail,
    RecoveryScanLimit,
    IndexPageMagicMismatch,
    IndexPageVersionUnsupported(u16),
    IndexPageHashMismatch(u64),
    IndexPageBoundsInvalid,
    IndexPageReservedNonZero,
    IndexPageKindUnsupported(u8),
    IndexCatalogInvalid,
    IndexTreeInvalid,
    IndexTreeDepthExceeded,
    IndexTreePageLimit,
    IndexDuplicateEntry,
    IndexChunkNotFound(u64),
    IndexNotPresent(u16),
    UnsupportedRequiredIndex(u16),
    GenerationProfileCannotAppend,
    ScientificModelCannotAppend,
    ScientificModelMultipleGenerations,
    GenerationDestinationNotEmpty,
    ScientificModel(ScientificModelError),
}
impl fmt::Display for ChunkIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Header(e) => write!(f, "{e}"),
            Self::Metadata(e) => write!(f, "{e}"),
            Self::InvalidDirectory => write!(f, "invalid WX-1 chunk directory"),
            Self::CommitFooterInvalid => write!(f, "invalid WX-1 commit footer"),
            Self::CommitHashMismatch => write!(f, "WX-1 commit footer BLAKE3 hash mismatch"),
            Self::TooManyChunks => write!(f, "too many WX-1 chunks"),
            Self::ChunkTooLarge => write!(f, "WX-1 chunk exceeds the 64 MiB limit"),
            Self::InvalidStreamBufferSize => write!(f, "WX-1 stream buffer size must be between 1 byte and 8 MiB"),
            Self::AllocationFailed => write!(f, "not enough memory for a bounded WX-1 codec buffer"),
            Self::DuplicateChunkId => write!(f, "duplicate WX-1 chunk identifier"),
            Self::ChunkNotFound(id) => write!(f, "WX-1 chunk {id} not found"),
            Self::ChecksumMismatch(id) => write!(f, "CRC32C mismatch for WX-1 chunk {id}"),
            Self::Compression(message) => write!(f, "chunk compression failed: {message}"),
            Self::Decompression(message) => write!(f, "chunk decompression failed: {message}"),
            Self::LengthMismatch(id) => write!(f, "decoded length mismatch for WX-1 chunk {id}"),
            Self::LengthOverflow => write!(f, "WX-1 chunk length or offset overflow"),
            Self::DirectoryMagicMismatch => write!(f, "invalid WX-1 directory magic"),
            Self::DirectoryVersionUnsupported(version) => write!(f, "unsupported WX-1 directory version {version}"),
            Self::DirectoryEntrySizeInvalid => write!(f, "invalid WX-1 directory entry size"),
            Self::DirectoryReservedNonZero => write!(f, "nonzero reserved field in WX-1 directory"),
            Self::DirectoryBoundsInvalid => write!(f, "WX-1 directory bounds do not match the generation"),
            Self::DirectoryOffsetInvalid => write!(f, "WX-1 directory contains an invalid payload offset"),
            Self::DirectoryOverlap => write!(f, "WX-1 directory contains overlapping chunk ranges"),
            Self::DirectoryDuplicateChunkId => write!(f, "duplicate WX-1 chunk identifier across directory generations"),
            Self::DirectoryDescriptorMismatch => write!(f, "chunk descriptor is not part of the selected WX-1 snapshot"),
            Self::UnsupportedCodec(codec) => write!(f, "unsupported WX-1 chunk codec {codec}"),
            Self::ChunkHashMismatch(id) => write!(f, "BLAKE3 digest mismatch for WX-1 chunk {id}"),
            Self::GenerationFooterInvalid => write!(f, "invalid WX-1 generation footer"),
            Self::GenerationHashMismatch => write!(f, "WX-1 generation BLAKE3 digest mismatch"),
            Self::GenerationChainInvalid => write!(f, "invalid WX-1 generation footer chain"),
            Self::GenerationHeaderDirectoryOffsetNonZero => write!(f, "generation-profile WX-1 header directory_offset must be zero"),
            Self::UnsupportedGenerationVersion(version) => write!(f, "unsupported WX-1 generation profile version {version}"),
            Self::NoValidGeneration => write!(f, "no fully valid WX-1 generation was found"),
            Self::UnrepairedTail => write!(f, "WX-1 file has an invalid trailing tail; run explicit repair before appending"),
            Self::RecoveryScanLimit => write!(f, "WX-1 recovery scan candidate limit exceeded"),
            Self::IndexPageMagicMismatch => write!(f, "invalid WX-1 B+tree page magic"),
            Self::IndexPageVersionUnsupported(version) => write!(f, "unsupported WX-1 B+tree page version {version}"),
            Self::IndexPageHashMismatch(offset) => write!(f, "BLAKE3 digest mismatch for WX-1 index page at offset {offset}"),
            Self::IndexPageBoundsInvalid => write!(f, "WX-1 index page offset is outside committed index pages"),
            Self::IndexPageReservedNonZero => write!(f, "nonzero reserved or padding bytes in WX-1 index page"),
            Self::IndexPageKindUnsupported(kind) => write!(f, "unsupported WX-1 index page kind {kind}"),
            Self::IndexCatalogInvalid => write!(f, "invalid WX-1 persistent-index catalog"),
            Self::IndexTreeInvalid => write!(f, "invalid WX-1 B+tree structure or ordering"),
            Self::IndexTreeDepthExceeded => write!(f, "WX-1 B+tree depth limit exceeded"),
            Self::IndexTreePageLimit => write!(f, "WX-1 B+tree page or entry limit exceeded"),
            Self::IndexDuplicateEntry => write!(f, "duplicate WX-1 persistent-index key/chunk mapping"),
            Self::IndexChunkNotFound(id) => write!(f, "WX-1 index references unknown chunk {id}"),
            Self::IndexNotPresent(kind) => write!(f, "WX-1 logical index kind {kind} is not present"),
            Self::UnsupportedRequiredIndex(kind) => write!(f, "WX-1 file requires unsupported logical index kind {kind}"),
            Self::GenerationProfileCannotAppend => write!(f, "cannot append v0.3.9 indexes to a v0.3.8 generation profile; rewrite into a new v0.3.9 file first"),
            Self::ScientificModelCannotAppend => write!(f, "cannot append to a scientific schema-v2 file because its metadata preamble is immutable; rewrite to a new file"),
            Self::ScientificModelMultipleGenerations => write!(f, "scientific schema-v2 files must contain exactly one committed generation"),
            Self::GenerationDestinationNotEmpty => write!(f, "new WX-1 generation writer requires an empty destination"),
            Self::ScientificModel(error) => write!(f, "{error}"),
        }
    }
}
impl std::error::Error for ChunkIoError {}
impl From<std::io::Error> for ChunkIoError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

const fn make_crc32c_tables_8() -> [[u32; 256]; 8] {
    let mut tables = [[0u32; 256]; 8];
    let mut index = 0usize;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 != 0 {
                (value >> 1) ^ 0x82F6_3B78
            } else {
                value >> 1
            };
            bit += 1;
        }
        tables[0][index] = value;
        index += 1;
    }
    let mut k = 1usize;
    while k < 8 {
        let mut i = 0usize;
        while i < 256 {
            let prev = tables[k - 1][i];
            tables[k][i] = (prev >> 8) ^ tables[0][(prev & 0xFF) as usize];
            i += 1;
        }
        k += 1;
    }
    tables
}
const CRC32C_TABLES_8: [[u32; 256]; 8] = make_crc32c_tables_8();
const CRC32C_TABLE: [u32; 256] = CRC32C_TABLES_8[0];

/// Computes CRC32C (Castagnoli) over a byte slice using a compile-time slice-by-8 lookup table.
pub fn crc32c(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        let c = crc ^ u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        crc = CRC32C_TABLES_8[7][(c & 0xFF) as usize]
            ^ CRC32C_TABLES_8[6][((c >> 8) & 0xFF) as usize]
            ^ CRC32C_TABLES_8[5][((c >> 16) & 0xFF) as usize]
            ^ CRC32C_TABLES_8[4][((c >> 24) & 0xFF) as usize]
            ^ CRC32C_TABLES_8[3][chunk[4] as usize]
            ^ CRC32C_TABLES_8[2][chunk[5] as usize]
            ^ CRC32C_TABLES_8[1][chunk[6] as usize]
            ^ CRC32C_TABLES_8[0][chunk[7] as usize];
    }
    for &byte in chunks.remainder() {
        let index = ((crc as u8) ^ byte) as usize;
        crc = (crc >> 8) ^ CRC32C_TABLE[index];
    }
    !crc
}

fn put_u16(out: &mut Vec<u8>, value: u16, order: HeaderByteOrder) {
    out.extend_from_slice(&match order {
        HeaderByteOrder::LittleEndian => value.to_le_bytes(),
        HeaderByteOrder::BigEndian => value.to_be_bytes(),
    });
}
fn put_u32(out: &mut Vec<u8>, value: u32, order: HeaderByteOrder) {
    out.extend_from_slice(&match order {
        HeaderByteOrder::LittleEndian => value.to_le_bytes(),
        HeaderByteOrder::BigEndian => value.to_be_bytes(),
    });
}
fn put_u64(out: &mut Vec<u8>, value: u64, order: HeaderByteOrder) {
    out.extend_from_slice(&match order {
        HeaderByteOrder::LittleEndian => value.to_le_bytes(),
        HeaderByteOrder::BigEndian => value.to_be_bytes(),
    });
}
fn read_u16_order(bytes: &[u8], order: HeaderByteOrder) -> Result<u16, ChunkIoError> {
    let a: [u8; 2] = bytes
        .try_into()
        .map_err(|_| ChunkIoError::InvalidDirectory)?;
    Ok(match order {
        HeaderByteOrder::LittleEndian => u16::from_le_bytes(a),
        HeaderByteOrder::BigEndian => u16::from_be_bytes(a),
    })
}
fn read_u32_order(bytes: &[u8], order: HeaderByteOrder) -> Result<u32, ChunkIoError> {
    let a: [u8; 4] = bytes
        .try_into()
        .map_err(|_| ChunkIoError::InvalidDirectory)?;
    Ok(match order {
        HeaderByteOrder::LittleEndian => u32::from_le_bytes(a),
        HeaderByteOrder::BigEndian => u32::from_be_bytes(a),
    })
}
fn read_u64_order(bytes: &[u8], order: HeaderByteOrder) -> Result<u64, ChunkIoError> {
    let a: [u8; 8] = bytes
        .try_into()
        .map_err(|_| ChunkIoError::InvalidDirectory)?;
    Ok(match order {
        HeaderByteOrder::LittleEndian => u64::from_le_bytes(a),
        HeaderByteOrder::BigEndian => u64::from_be_bytes(a),
    })
}

/// Writes a complete experimental container with bounded, lossless chunk codecs.
/// The destination must be new or truncated. Each chunk is encoded independently;
/// only one encoded chunk is held in memory at a time.
pub fn write_chunk_container_with_codecs<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    metadata: &Metadata,
    chunks: &[EncodedChunkInput<'_>],
    order: HeaderByteOrder,
) -> Result<(), ChunkIoError> {
    use std::io::SeekFrom;
    if metadata.scientific_model().is_some() {
        return Err(ChunkIoError::Metadata(
            MetadataError::ScientificModelRequiresGeneration,
        ));
    }
    if chunks.len() as u64 > MAX_CHUNKS {
        return Err(ChunkIoError::TooManyChunks);
    }
    let metadata_bytes = metadata.to_cbor().map_err(ChunkIoError::Metadata)?;
    let mut ids = std::collections::BTreeSet::new();
    for chunk in chunks {
        if !ids.insert(chunk.id) {
            return Err(ChunkIoError::DuplicateChunkId);
        }
        if chunk.data.len() as u64 > MAX_CHUNK_BYTES {
            return Err(ChunkIoError::ChunkTooLarge);
        }
    }

    // Write a placeholder header; directory_offset is backpatched after payloads.
    let placeholder = ContainerHeader {
        major: 1,
        minor: 2,
        byte_order: order,
        flags: 0,
        metadata_len: metadata_bytes.len() as u64,
        directory_offset: 0,
    };
    writer.seek(SeekFrom::Start(0))?;
    writer.write_all(&placeholder.encode())?;
    writer.write_all(&metadata_bytes)?;
    let mut descriptors = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let stored = encode_chunk(chunk.codec, chunk.data)?;
        if stored.len() as u64 > MAX_STORED_CHUNK_BYTES {
            return Err(ChunkIoError::ChunkTooLarge);
        }
        let offset = writer.stream_position()?;
        writer.write_all(&stored)?;
        descriptors.push(ChunkDescriptor {
            id: chunk.id,
            offset,
            stored_len: stored.len() as u64,
            raw_len: chunk.data.len() as u64,
            crc32c: crc32c(&stored),
            codec: chunk.codec,
            blake3: None,
        });
    }
    let directory_offset = writer.stream_position()?;
    let directory_len = CHUNK_DIRECTORY_HEADER_LEN
        .checked_add(
            (chunks.len() as u64)
                .checked_mul(CHUNK_DESCRIPTOR_LEN)
                .ok_or(ChunkIoError::LengthOverflow)?,
        )
        .ok_or(ChunkIoError::LengthOverflow)?;
    directory_offset
        .checked_add(directory_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    let mut directory = Vec::with_capacity(
        usize::try_from(directory_len).map_err(|_| ChunkIoError::LengthOverflow)?,
    );
    directory.extend_from_slice(&CHUNK_DIRECTORY_MAGIC);
    put_u16(&mut directory, 1, order);
    put_u16(&mut directory, CHUNK_DESCRIPTOR_LEN as u16, order);
    put_u32(&mut directory, descriptors.len() as u32, order);
    put_u32(&mut directory, 0, order);
    for descriptor in descriptors {
        put_u64(&mut directory, descriptor.id, order);
        put_u64(&mut directory, descriptor.offset, order);
        put_u64(&mut directory, descriptor.stored_len, order);
        put_u64(&mut directory, descriptor.raw_len, order);
        put_u32(&mut directory, descriptor.crc32c, order);
        directory.push(descriptor.codec as u8);
        directory.push(0);
        put_u16(&mut directory, 0, order);
    }
    writer.write_all(&directory)?;
    let header = ContainerHeader {
        directory_offset,
        ..placeholder
    };
    writer.seek(SeekFrom::Start(0))?;
    writer.write_all(&header.encode())?;
    writer.flush()?;
    Ok(())
}

/// Writes raw chunks using the backwards-compatible convenience API.
pub fn write_chunk_container<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    metadata: &Metadata,
    chunks: &[ChunkInput<'_>],
    order: HeaderByteOrder,
) -> Result<(), ChunkIoError> {
    let encoded: Vec<_> = chunks
        .iter()
        .map(|chunk| EncodedChunkInput {
            id: chunk.id,
            data: chunk.data,
            codec: ChunkCodec::Raw,
        })
        .collect();
    write_chunk_container_with_codecs(writer, metadata, &encoded, order)
}

pub(crate) fn encode_chunk(codec: ChunkCodec, raw: &[u8]) -> Result<Vec<u8>, ChunkIoError> {
    match codec {
        ChunkCodec::Raw => Ok(raw.to_vec()),
        ChunkCodec::Zstandard => zstd::stream::encode_all(std::io::Cursor::new(raw), 3)
            .map_err(|error| ChunkIoError::Compression(error.to_string())),
        ChunkCodec::Lz4 => Ok(lz4_flex::block::compress(raw)),
        ChunkCodec::ZstdByteShuffle4 => {
            let shuffled = quantization::byte_shuffle(raw, 4)?;
            zstd::stream::encode_all(std::io::Cursor::new(&shuffled), 3)
                .map_err(|error| ChunkIoError::Compression(error.to_string()))
        }
        ChunkCodec::ZstdByteShuffle8 => {
            let shuffled = quantization::byte_shuffle(raw, 8)?;
            zstd::stream::encode_all(std::io::Cursor::new(&shuffled), 3)
                .map_err(|error| ChunkIoError::Compression(error.to_string()))
        }
        ChunkCodec::ZstdByteShuffle2 => {
            let shuffled = quantization::byte_shuffle(raw, 2)?;
            zstd::stream::encode_all(std::io::Cursor::new(&shuffled), 3)
                .map_err(|error| ChunkIoError::Compression(error.to_string()))
        }
    }
}

fn decode_chunk(
    codec: ChunkCodec,
    stored: &[u8],
    raw_len: u64,
    id: u64,
) -> Result<Vec<u8>, ChunkIoError> {
    if raw_len > MAX_CHUNK_BYTES {
        return Err(ChunkIoError::ChunkTooLarge);
    }
    let expected = usize::try_from(raw_len).map_err(|_| ChunkIoError::ChunkTooLarge)?;
    let decoded = match codec {
        ChunkCodec::Raw => stored.to_vec(),
        ChunkCodec::Lz4 => lz4_flex::block::decompress(stored, expected)
            .map_err(|error| ChunkIoError::Decompression(error.to_string()))?,
        ChunkCodec::Zstandard => {
            use std::io::Read;
            let decoder = zstd::stream::read::Decoder::new(std::io::Cursor::new(stored))
                .map_err(|error| ChunkIoError::Decompression(error.to_string()))?;
            let limit = raw_len.checked_add(1).ok_or(ChunkIoError::LengthOverflow)?;
            let mut bounded = decoder.take(limit);
            let mut output = Vec::with_capacity(expected.min(16 * 1024 * 1024));
            bounded
                .read_to_end(&mut output)
                .map_err(|error| ChunkIoError::Decompression(error.to_string()))?;
            output
        }
        ChunkCodec::ZstdByteShuffle4 => {
            use std::io::Read;
            let decoder = zstd::stream::read::Decoder::new(std::io::Cursor::new(stored))
                .map_err(|error| ChunkIoError::Decompression(error.to_string()))?;
            let limit = raw_len.checked_add(1).ok_or(ChunkIoError::LengthOverflow)?;
            let mut bounded = decoder.take(limit);
            let mut output = Vec::with_capacity(expected.min(16 * 1024 * 1024));
            bounded
                .read_to_end(&mut output)
                .map_err(|error| ChunkIoError::Decompression(error.to_string()))?;
            if output.len() != expected {
                return Err(ChunkIoError::LengthMismatch(id));
            }
            quantization::byte_unshuffle(&output, 4)?
        }
        ChunkCodec::ZstdByteShuffle8 => {
            use std::io::Read;
            let decoder = zstd::stream::read::Decoder::new(std::io::Cursor::new(stored))
                .map_err(|error| ChunkIoError::Decompression(error.to_string()))?;
            let limit = raw_len.checked_add(1).ok_or(ChunkIoError::LengthOverflow)?;
            let mut bounded = decoder.take(limit);
            let mut output = Vec::with_capacity(expected.min(16 * 1024 * 1024));
            bounded
                .read_to_end(&mut output)
                .map_err(|error| ChunkIoError::Decompression(error.to_string()))?;
            if output.len() != expected {
                return Err(ChunkIoError::LengthMismatch(id));
            }
            quantization::byte_unshuffle(&output, 8)?
        }
        ChunkCodec::ZstdByteShuffle2 => {
            use std::io::Read;
            let decoder = zstd::stream::read::Decoder::new(std::io::Cursor::new(stored))
                .map_err(|error| ChunkIoError::Decompression(error.to_string()))?;
            let limit = raw_len.checked_add(1).ok_or(ChunkIoError::LengthOverflow)?;
            let mut bounded = decoder.take(limit);
            let mut output = Vec::with_capacity(expected.min(16 * 1024 * 1024));
            bounded
                .read_to_end(&mut output)
                .map_err(|error| ChunkIoError::Decompression(error.to_string()))?;
            if output.len() != expected {
                return Err(ChunkIoError::LengthMismatch(id));
            }
            quantization::byte_unshuffle(&output, 2)?
        }
    };
    if decoded.len() != expected {
        return Err(ChunkIoError::LengthMismatch(id));
    }
    Ok(decoded)
}

/// Reads metadata and validates all descriptors in an experimental chunk directory.
pub fn read_chunk_directory<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
) -> Result<(Metadata, Vec<ChunkDescriptor>), ChunkIoError> {
    use std::io::SeekFrom;
    let file_len = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut raw_header = [0u8; CONTAINER_HEADER_LEN];
    reader.read_exact(&mut raw_header)?;
    let header = ContainerHeader::decode(&raw_header).map_err(ChunkIoError::Header)?;
    header
        .validate_file_bounds(file_len)
        .map_err(ChunkIoError::Header)?;
    if is_generation_profile_minor(header.minor) {
        let snapshot = open_generation_snapshot(reader)?;
        return Ok((snapshot.metadata().clone(), snapshot.descriptors().to_vec()));
    }
    if header.minor > GENERATION_PROFILE_MINOR_VERSION {
        return Err(ChunkIoError::UnsupportedGenerationVersion(header.minor));
    }
    if header.directory_offset == 0 {
        return Err(ChunkIoError::InvalidDirectory);
    }
    if header.metadata_len > MAX_METADATA_BYTES {
        return Err(ChunkIoError::Metadata(MetadataError::TooLarge));
    }
    let metadata_end = (CONTAINER_HEADER_LEN as u64)
        .checked_add(header.metadata_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    let metadata_size =
        usize::try_from(header.metadata_len).map_err(|_| ChunkIoError::LengthOverflow)?;
    let mut metadata_bytes = vec![0u8; metadata_size];
    reader.read_exact(&mut metadata_bytes)?;
    let metadata = Metadata::from_cbor(&metadata_bytes).map_err(ChunkIoError::Metadata)?;
    reader.seek(SeekFrom::Start(header.directory_offset))?;
    let mut dir_header = [0u8; CHUNK_DIRECTORY_HEADER_LEN as usize];
    reader.read_exact(&mut dir_header)?;
    if dir_header[..4] != CHUNK_DIRECTORY_MAGIC {
        return Err(ChunkIoError::InvalidDirectory);
    }
    let version = read_u16_order(&dir_header[4..6], header.byte_order)?;
    let entry_size = read_u16_order(&dir_header[6..8], header.byte_order)?;
    let count = read_u32_order(&dir_header[8..12], header.byte_order)? as u64;
    let reserved = read_u32_order(&dir_header[12..16], header.byte_order)?;
    if version != 1
        || entry_size as u64 != CHUNK_DESCRIPTOR_LEN
        || reserved != 0
        || count > MAX_CHUNKS
    {
        return Err(ChunkIoError::InvalidDirectory);
    }
    let directory_entries_position = reader.stream_position()?;
    let expected_dir_len = CHUNK_DIRECTORY_HEADER_LEN
        .checked_add(
            count
                .checked_mul(CHUNK_DESCRIPTOR_LEN)
                .ok_or(ChunkIoError::LengthOverflow)?,
        )
        .ok_or(ChunkIoError::LengthOverflow)?;
    let expected_file_len = header
        .directory_offset
        .checked_add(expected_dir_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    if expected_file_len == file_len {
        // Backward-compatible uncommitted v0.3.5/v0.3.6 experimental container.
    } else {
        let committed_len = expected_file_len
            .checked_add(COMMIT_FOOTER_LEN)
            .ok_or(ChunkIoError::LengthOverflow)?;
        if committed_len != file_len {
            return Err(if file_len > expected_file_len {
                ChunkIoError::CommitFooterInvalid
            } else {
                ChunkIoError::InvalidDirectory
            });
        }
        verify_commit_footer(reader, expected_file_len)?;
        reader.seek(SeekFrom::Start(directory_entries_position))?;
    }
    let mut descriptors =
        Vec::with_capacity(usize::try_from(count).map_err(|_| ChunkIoError::LengthOverflow)?);
    let mut ids = std::collections::BTreeSet::new();
    for _ in 0..count {
        let mut entry = [0u8; CHUNK_DESCRIPTOR_LEN as usize];
        reader.read_exact(&mut entry)?;
        let id = read_u64_order(&entry[0..8], header.byte_order)?;
        let offset = read_u64_order(&entry[8..16], header.byte_order)?;
        let stored_len = read_u64_order(&entry[16..24], header.byte_order)?;
        let raw_len = read_u64_order(&entry[24..32], header.byte_order)?;
        let checksum = read_u32_order(&entry[32..36], header.byte_order)?;
        if entry[37] != 0 || read_u16_order(&entry[38..40], header.byte_order)? != 0 {
            return Err(ChunkIoError::InvalidDirectory);
        }
        let codec = ChunkCodec::try_from(entry[36])?;
        if !ids.insert(id)
            || raw_len > MAX_CHUNK_BYTES
            || stored_len > MAX_STORED_CHUNK_BYTES
            || (codec == ChunkCodec::Raw && stored_len != raw_len)
        {
            return Err(ChunkIoError::InvalidDirectory);
        }
        let end = offset
            .checked_add(stored_len)
            .ok_or(ChunkIoError::LengthOverflow)?;
        if offset < metadata_end || end > header.directory_offset {
            return Err(ChunkIoError::InvalidDirectory);
        }
        descriptors.push(ChunkDescriptor {
            id,
            offset,
            stored_len,
            raw_len,
            crc32c: checksum,
            codec,
            blake3: None,
        });
    }
    let mut by_offset = descriptors.clone();
    by_offset.sort_by_key(|d| d.offset);
    for pair in by_offset.windows(2) {
        let previous_end = pair[0]
            .offset
            .checked_add(pair[0].stored_len)
            .ok_or(ChunkIoError::LengthOverflow)?;
        if previous_end > pair[1].offset {
            return Err(ChunkIoError::InvalidDirectory);
        }
    }
    Ok((metadata, descriptors))
}

/// Reads a previously validated chunk descriptor, verifies stored-byte CRC32C, and decodes it.
/// The descriptor's byte range and decoded-size limit are checked before allocation.
pub fn read_chunk_payload<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    descriptor: ChunkDescriptor,
) -> Result<Vec<u8>, ChunkIoError> {
    use std::io::SeekFrom;
    let file_len = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut raw_header = [0u8; CONTAINER_HEADER_LEN];
    reader.read_exact(&mut raw_header)?;
    let header = ContainerHeader::decode(&raw_header).map_err(ChunkIoError::Header)?;
    header
        .validate_file_bounds(file_len)
        .map_err(ChunkIoError::Header)?;
    if is_generation_profile_minor(header.minor) {
        let snapshot = open_generation_snapshot(reader)?;
        let selected = snapshot
            .chunk(descriptor.id)
            .ok_or(ChunkIoError::DirectoryDescriptorMismatch)?;
        if selected != descriptor {
            return Err(ChunkIoError::DirectoryDescriptorMismatch);
        }
        return read_generation_chunk_payload(reader, selected, snapshot.valid_end());
    }
    if header.minor > GENERATION_PROFILE_MINOR_VERSION {
        return Err(ChunkIoError::UnsupportedGenerationVersion(header.minor));
    }
    let metadata_end = (CONTAINER_HEADER_LEN as u64)
        .checked_add(header.metadata_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    let chunk_end = descriptor
        .offset
        .checked_add(descriptor.stored_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    if header.directory_offset == 0
        || descriptor.offset < metadata_end
        || chunk_end > header.directory_offset
    {
        return Err(ChunkIoError::InvalidDirectory);
    }
    if descriptor.raw_len > MAX_CHUNK_BYTES || descriptor.stored_len > MAX_STORED_CHUNK_BYTES {
        return Err(ChunkIoError::ChunkTooLarge);
    }
    let size = usize::try_from(descriptor.stored_len).map_err(|_| ChunkIoError::ChunkTooLarge)?;
    let mut stored = vec![0u8; size];
    reader.seek(SeekFrom::Start(descriptor.offset))?;
    reader.read_exact(&mut stored)?;
    if crc32c(&stored) != descriptor.crc32c {
        return Err(ChunkIoError::ChecksumMismatch(descriptor.id));
    }
    decode_chunk(descriptor.codec, &stored, descriptor.raw_len, descriptor.id)
}

/// Reads one raw chunk by ID and verifies its CRC32C before returning its bytes.
pub fn read_chunk<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    chunk_id: u64,
) -> Result<Vec<u8>, ChunkIoError> {
    let (_, descriptors) = read_chunk_directory(reader)?;
    let descriptor = descriptors
        .into_iter()
        .find(|d| d.id == chunk_id)
        .ok_or(ChunkIoError::ChunkNotFound(chunk_id))?;
    read_chunk_payload(reader, descriptor)
}

#[cfg(test)]
mod chunk_storage_tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn crc32c_matches_standard_check_value() {
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
    }

    #[test]
    fn chunk_container_roundtrips_both_byte_orders_and_partial_reads() {
        for order in [HeaderByteOrder::LittleEndian, HeaderByteOrder::BigEndian] {
            let metadata = Metadata::new("chunk-test");
            let a = b"temperature-grid";
            let b = b"wind-grid-data";
            let chunks = [ChunkInput { id: 7, data: a }, ChunkInput { id: 9, data: b }];
            let mut cursor = Cursor::new(Vec::new());
            write_chunk_container(&mut cursor, &metadata, &chunks, order).unwrap();
            let (decoded, directory) = read_chunk_directory(&mut cursor).unwrap();
            assert_eq!(decoded, metadata);
            assert_eq!(directory.len(), 2);
            assert_eq!(read_chunk(&mut cursor, 9).unwrap(), b);
        }
    }

    #[test]
    fn rejects_duplicate_chunk_ids_before_writing() {
        let chunks = [
            ChunkInput { id: 1, data: b"a" },
            ChunkInput { id: 1, data: b"b" },
        ];
        let mut cursor = Cursor::new(Vec::new());
        assert!(matches!(
            write_chunk_container(
                &mut cursor,
                &Metadata::new("x"),
                &chunks,
                HeaderByteOrder::LittleEndian
            ),
            Err(ChunkIoError::DuplicateChunkId)
        ));
    }

    #[test]
    fn rejects_corrupted_chunk_checksum() {
        let chunks = [ChunkInput {
            id: 1,
            data: b"payload",
        }];
        let mut cursor = Cursor::new(Vec::new());
        write_chunk_container(
            &mut cursor,
            &Metadata::new("x"),
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let payload_offset = CONTAINER_HEADER_LEN + Metadata::new("x").to_cbor().unwrap().len();
        cursor.get_mut()[payload_offset] ^= 0x01;
        assert!(matches!(
            read_chunk(&mut cursor, 1),
            Err(ChunkIoError::ChecksumMismatch(1))
        ));
    }

    #[test]
    fn rejects_truncated_directory() {
        let chunks = [ChunkInput {
            id: 1,
            data: b"payload",
        }];
        let mut cursor = Cursor::new(Vec::new());
        write_chunk_container(
            &mut cursor,
            &Metadata::new("x"),
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        cursor.get_mut().pop();
        assert!(read_chunk_directory(&mut cursor).is_err());
    }

    #[test]
    fn rejects_missing_chunk_id() {
        let mut cursor = Cursor::new(Vec::new());
        write_chunk_container(
            &mut cursor,
            &Metadata::new("x"),
            &[],
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        assert!(matches!(
            read_chunk(&mut cursor, 99),
            Err(ChunkIoError::ChunkNotFound(99))
        ));
    }

    #[test]
    fn rejects_unknown_codec_in_directory() {
        let chunks = [ChunkInput {
            id: 1,
            data: b"payload",
        }];
        let mut cursor = Cursor::new(Vec::new());
        write_chunk_container(
            &mut cursor,
            &Metadata::new("x"),
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let dir_offset = ContainerHeader::decode(&cursor.get_ref()[..CONTAINER_HEADER_LEN])
            .unwrap()
            .directory_offset as usize;
        cursor.get_mut()[dir_offset + CHUNK_DIRECTORY_HEADER_LEN as usize + 36] = 255;
        assert!(matches!(
            read_chunk_directory(&mut cursor),
            Err(ChunkIoError::InvalidDirectory)
        ));
    }

    #[test]
    fn rejects_overlapping_chunk_ranges() {
        let chunks = [
            ChunkInput {
                id: 1,
                data: b"first",
            },
            ChunkInput {
                id: 2,
                data: b"second",
            },
        ];
        let mut cursor = Cursor::new(Vec::new());
        write_chunk_container(
            &mut cursor,
            &Metadata::new("x"),
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let dir_offset = ContainerHeader::decode(&cursor.get_ref()[..CONTAINER_HEADER_LEN])
            .unwrap()
            .directory_offset as usize;
        let first_offset = cursor.get_ref()[dir_offset + 16 + 8..dir_offset + 16 + 16].to_vec();
        cursor.get_mut()[dir_offset + 16 + 40 + 8..dir_offset + 16 + 40 + 16]
            .copy_from_slice(&first_offset);
        assert!(matches!(
            read_chunk_directory(&mut cursor),
            Err(ChunkIoError::InvalidDirectory)
        ));
    }
    #[test]
    fn compressed_chunks_roundtrip_all_lossless_codecs() {
        let payload = vec![b'A'; 128 * 1024];
        let chunks = [
            EncodedChunkInput {
                id: 1,
                data: &payload,
                codec: ChunkCodec::Raw,
            },
            EncodedChunkInput {
                id: 2,
                data: &payload,
                codec: ChunkCodec::Zstandard,
            },
            EncodedChunkInput {
                id: 3,
                data: &payload,
                codec: ChunkCodec::Lz4,
            },
        ];
        let mut cursor = Cursor::new(Vec::new());
        write_chunk_container_with_codecs(
            &mut cursor,
            &Metadata::new("codec-test"),
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let (_, descriptors) = read_chunk_directory(&mut cursor).unwrap();
        assert_eq!(descriptors.len(), 3);
        for id in 1..=3 {
            assert_eq!(read_chunk(&mut cursor, id).unwrap(), payload);
        }
        assert!(descriptors[1].stored_len < descriptors[1].raw_len);
        assert!(descriptors[2].stored_len < descriptors[2].raw_len);
    }

    #[test]
    fn rejects_decompression_length_mismatch() {
        let payload = b"bounded decompression";
        let chunks = [EncodedChunkInput {
            id: 8,
            data: payload,
            codec: ChunkCodec::Zstandard,
        }];
        let mut cursor = Cursor::new(Vec::new());
        write_chunk_container_with_codecs(
            &mut cursor,
            &Metadata::new("length-test"),
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let (_, mut descriptors) = read_chunk_directory(&mut cursor).unwrap();
        descriptors[0].raw_len += 1;
        assert!(matches!(
            read_chunk_payload(&mut cursor, descriptors[0]),
            Err(ChunkIoError::LengthMismatch(8))
        ));
    }

    #[test]
    fn compressed_chunks_roundtrip_big_endian() {
        let payload = vec![42_u8; 32 * 1024];
        let chunks = [
            EncodedChunkInput {
                id: 10,
                data: &payload,
                codec: ChunkCodec::Zstandard,
            },
            EncodedChunkInput {
                id: 11,
                data: &payload,
                codec: ChunkCodec::Lz4,
            },
        ];
        let mut cursor = Cursor::new(Vec::new());
        write_chunk_container_with_codecs(
            &mut cursor,
            &Metadata::new("be-codec-test"),
            &chunks,
            HeaderByteOrder::BigEndian,
        )
        .unwrap();
        assert_eq!(read_chunk(&mut cursor, 10).unwrap(), payload);
        assert_eq!(read_chunk(&mut cursor, 11).unwrap(), payload);
    }

    #[test]
    fn compressed_stored_bytes_are_checksummed_before_decode() {
        let payload = vec![b'Z'; 4096];
        let chunks = [EncodedChunkInput {
            id: 12,
            data: &payload,
            codec: ChunkCodec::Zstandard,
        }];
        let mut cursor = Cursor::new(Vec::new());
        write_chunk_container_with_codecs(
            &mut cursor,
            &Metadata::new("crc-codec-test"),
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let (_, descriptors) = read_chunk_directory(&mut cursor).unwrap();
        cursor.get_mut()[descriptors[0].offset as usize] ^= 1;
        assert!(matches!(
            read_chunk_payload(&mut cursor, descriptors[0]),
            Err(ChunkIoError::ChecksumMismatch(12))
        ));
    }
}

fn copy_with_buffer<R: std::io::Read, W: std::io::Write>(
    reader: &mut R,
    writer: &mut W,
    buffer: &mut [u8],
) -> std::io::Result<u64> {
    let mut total = 0u64;
    loop {
        let n = reader.read(buffer)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buffer[..n])?;
        total = total
            .checked_add(n as u64)
            .ok_or_else(|| std::io::Error::other("copy length overflow"))?;
    }
    Ok(total)
}

/// Copies exactly `length` bytes using a caller-provided bounded buffer.
/// Unlike `Read::take`, this also works with `dyn Read` inputs.
fn copy_exactly_n<R: std::io::Read + ?Sized, W: std::io::Write>(
    reader: &mut R,
    writer: &mut W,
    length: u64,
    buffer: &mut [u8],
) -> std::io::Result<u64> {
    let mut remaining = length;
    let mut total = 0u64;
    while remaining > 0 {
        let wanted = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| std::io::Error::other("copy length overflow"))?;
        let n = reader.read(&mut buffer[..wanted])?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "input chunk ended before declared length",
            ));
        }
        writer.write_all(&buffer[..n])?;
        remaining -= n as u64;
        total = total
            .checked_add(n as u64)
            .ok_or_else(|| std::io::Error::other("copy length overflow"))?;
    }
    Ok(total)
}

/// Configurable limits for streaming reads. The buffer is reused and never
/// exceeds `MAX_STREAM_BUFFER_BYTES`; this does not change the on-disk chunk limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkReadOptions {
    /// Copy/decompression buffer size in bytes.
    pub buffer_size: usize,
}

/// Maximum streaming buffer accepted by the experimental reader (8 MiB).
pub const MAX_STREAM_BUFFER_BYTES: usize = 8 * 1024 * 1024;

impl Default for ChunkReadOptions {
    fn default() -> Self {
        Self {
            buffer_size: 256 * 1024,
        }
    }
}

impl ChunkReadOptions {
    /// Validates the buffer size before allocating it.
    pub fn validate(self) -> Result<Self, ChunkIoError> {
        if self.buffer_size == 0 || self.buffer_size > MAX_STREAM_BUFFER_BYTES {
            return Err(ChunkIoError::InvalidDirectory);
        }
        Ok(self)
    }
}

/// Verifies a stored chunk in bounded memory, then streams its decoded bytes
/// directly to `output`. Raw and Zstandard chunks never allocate a full decoded
/// chunk. LZ4 block decoding is necessarily materialized by the current block
/// codec implementation and remains capped by `MAX_CHUNK_BYTES`.
pub fn copy_chunk_payload_to_writer<R, W>(
    reader: &mut R,
    descriptor: ChunkDescriptor,
    output: &mut W,
    options: ChunkReadOptions,
) -> Result<u64, ChunkIoError>
where
    R: std::io::Read + std::io::Seek,
    W: std::io::Write,
{
    use std::io::{Read, SeekFrom};
    let options = options.validate()?;
    if descriptor.raw_len > MAX_CHUNK_BYTES || descriptor.stored_len > MAX_STORED_CHUNK_BYTES {
        return Err(ChunkIoError::ChunkTooLarge);
    }
    let file_len = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut header_bytes = [0u8; CONTAINER_HEADER_LEN];
    reader.read_exact(&mut header_bytes)?;
    let header = ContainerHeader::decode(&header_bytes).map_err(ChunkIoError::Header)?;
    header
        .validate_file_bounds(file_len)
        .map_err(ChunkIoError::Header)?;
    let end = descriptor
        .offset
        .checked_add(descriptor.stored_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    let is_generation = is_generation_profile_minor(header.minor);
    if is_generation {
        let snapshot = open_generation_snapshot(reader)?;
        let selected = snapshot
            .chunk(descriptor.id)
            .ok_or(ChunkIoError::DirectoryDescriptorMismatch)?;
        if selected != descriptor || end > snapshot.valid_end() {
            return Err(ChunkIoError::DirectoryDescriptorMismatch);
        }
    } else {
        if header.minor > GENERATION_PROFILE_MINOR_VERSION {
            return Err(ChunkIoError::UnsupportedGenerationVersion(header.minor));
        }
        let metadata_end = (CONTAINER_HEADER_LEN as u64)
            .checked_add(header.metadata_len)
            .ok_or(ChunkIoError::LengthOverflow)?;
        if header.directory_offset == 0
            || descriptor.offset < metadata_end
            || end > header.directory_offset
        {
            return Err(ChunkIoError::InvalidDirectory);
        }
    }

    // First pass: CRC32C and (for v2) BLAKE3 verification without storing the encoded payload.
    reader.seek(SeekFrom::Start(descriptor.offset))?;
    let mut remaining = descriptor.stored_len;
    let mut buffer = vec![0u8; options.buffer_size];
    let mut crc = !0u32;
    let mut stored_hasher = blake3::Hasher::new();
    while remaining > 0 {
        let take = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| ChunkIoError::LengthOverflow)?;
        reader.read_exact(&mut buffer[..take])?;
        stored_hasher.update(&buffer[..take]);
        for &byte in &buffer[..take] {
            let index = ((crc as u8) ^ byte) as usize;
            crc = (crc >> 8) ^ CRC32C_TABLE[index];
        }
        remaining -= take as u64;
    }
    if (!crc) != descriptor.crc32c {
        return Err(ChunkIoError::ChecksumMismatch(descriptor.id));
    }
    if descriptor
        .blake3
        .is_some_and(|expected| expected != *stored_hasher.finalize().as_bytes())
    {
        return Err(ChunkIoError::ChunkHashMismatch(descriptor.id));
    }

    // Second pass: stream decoded bytes to the caller's writer.
    reader.seek(SeekFrom::Start(descriptor.offset))?;
    let written = match descriptor.codec {
        ChunkCodec::Raw => {
            let mut limited = reader.take(descriptor.stored_len);
            let copied = copy_with_buffer(&mut limited, output, &mut buffer)?;
            if copied != descriptor.raw_len {
                return Err(ChunkIoError::LengthMismatch(descriptor.id));
            }
            copied
        }
        ChunkCodec::Zstandard => {
            let limited = reader.take(descriptor.stored_len);
            let decoder = zstd::stream::read::Decoder::new(limited)
                .map_err(|e| ChunkIoError::Decompression(e.to_string()))?;
            let mut bounded = decoder.take(
                descriptor
                    .raw_len
                    .checked_add(1)
                    .ok_or(ChunkIoError::LengthOverflow)?,
            );
            let copied = copy_with_buffer(&mut bounded, output, &mut buffer)
                .map_err(|e| ChunkIoError::Decompression(e.to_string()))?;
            if copied != descriptor.raw_len {
                return Err(ChunkIoError::LengthMismatch(descriptor.id));
            }
            copied
        }
        ChunkCodec::Lz4 => {
            let stored_size =
                usize::try_from(descriptor.stored_len).map_err(|_| ChunkIoError::ChunkTooLarge)?;
            let mut stored = vec![0u8; stored_size];
            reader.read_exact(&mut stored)?;
            let decoded = lz4_flex::block::decompress(
                &stored,
                usize::try_from(descriptor.raw_len).map_err(|_| ChunkIoError::ChunkTooLarge)?,
            )
            .map_err(|e| ChunkIoError::Decompression(e.to_string()))?;
            if decoded.len() as u64 != descriptor.raw_len {
                return Err(ChunkIoError::LengthMismatch(descriptor.id));
            }
            output.write_all(&decoded)?;
            decoded.len() as u64
        }
        ChunkCodec::ZstdByteShuffle4
        | ChunkCodec::ZstdByteShuffle8
        | ChunkCodec::ZstdByteShuffle2 => {
            let item_size = match descriptor.codec {
                ChunkCodec::ZstdByteShuffle4 => 4,
                ChunkCodec::ZstdByteShuffle8 => 8,
                ChunkCodec::ZstdByteShuffle2 => 2,
                _ => unreachable!(),
            };
            let limited = reader.take(descriptor.stored_len);
            let mut decoder = zstd::stream::read::Decoder::new(limited)
                .map_err(|e| ChunkIoError::Decompression(e.to_string()))?;
            let mut decoded_shuffled = Vec::new();
            let raw_size =
                usize::try_from(descriptor.raw_len).map_err(|_| ChunkIoError::ChunkTooLarge)?;
            decoded_shuffled
                .try_reserve_exact(raw_size)
                .map_err(|_| ChunkIoError::AllocationFailed)?;
            use std::io::Read;
            decoder
                .read_to_end(&mut decoded_shuffled)
                .map_err(|e| ChunkIoError::Decompression(e.to_string()))?;
            if decoded_shuffled.len() as u64 != descriptor.raw_len {
                return Err(ChunkIoError::LengthMismatch(descriptor.id));
            }
            let unshuffled = quantization::byte_unshuffle(&decoded_shuffled, item_size)?;
            output.write_all(&unshuffled)?;
            unshuffled.len() as u64
        }
    };
    Ok(written)
}

/// Reads a byte range from an uncompressed chunk without loading the full chunk.
/// The complete stored chunk is CRC-verified first, then only the requested range
/// is copied. Compressed chunks return `InvalidDirectory` for this API.
pub fn read_raw_chunk_range<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    descriptor: ChunkDescriptor,
    offset: u64,
    length: u64,
    output: &mut impl std::io::Write,
    options: ChunkReadOptions,
) -> Result<u64, ChunkIoError> {
    use std::io::{Read, SeekFrom};
    let options = options.validate()?;
    if descriptor.codec != ChunkCodec::Raw || descriptor.raw_len != descriptor.stored_len {
        return Err(ChunkIoError::InvalidDirectory);
    }
    if descriptor.raw_len > MAX_CHUNK_BYTES || descriptor.stored_len > MAX_STORED_CHUNK_BYTES {
        return Err(ChunkIoError::ChunkTooLarge);
    }
    let range_end = offset
        .checked_add(length)
        .ok_or(ChunkIoError::LengthOverflow)?;
    if range_end > descriptor.raw_len {
        return Err(ChunkIoError::InvalidDirectory);
    }
    let file_len = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut header_bytes = [0u8; CONTAINER_HEADER_LEN];
    reader.read_exact(&mut header_bytes)?;
    let header = ContainerHeader::decode(&header_bytes).map_err(ChunkIoError::Header)?;
    header
        .validate_file_bounds(file_len)
        .map_err(ChunkIoError::Header)?;
    let chunk_end = descriptor
        .offset
        .checked_add(descriptor.stored_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    if is_generation_profile_minor(header.minor) {
        let snapshot = open_generation_snapshot(reader)?;
        let selected = snapshot
            .chunk(descriptor.id)
            .ok_or(ChunkIoError::DirectoryDescriptorMismatch)?;
        if selected != descriptor || chunk_end > snapshot.valid_end() {
            return Err(ChunkIoError::DirectoryDescriptorMismatch);
        }
    } else {
        if header.minor > GENERATION_PROFILE_MINOR_VERSION {
            return Err(ChunkIoError::UnsupportedGenerationVersion(header.minor));
        }
        let metadata_end = (CONTAINER_HEADER_LEN as u64)
            .checked_add(header.metadata_len)
            .ok_or(ChunkIoError::LengthOverflow)?;
        if header.directory_offset == 0
            || descriptor.offset < metadata_end
            || chunk_end > header.directory_offset
            || chunk_end > file_len
        {
            return Err(ChunkIoError::InvalidDirectory);
        }
    }
    reader.seek(SeekFrom::Start(descriptor.offset))?;
    let mut remaining = descriptor.stored_len;
    let mut buf = vec![0u8; options.buffer_size];
    let mut crc = !0u32;
    let mut stored_hasher = blake3::Hasher::new();
    while remaining > 0 {
        let n = usize::try_from(remaining.min(buf.len() as u64))
            .map_err(|_| ChunkIoError::LengthOverflow)?;
        reader.read_exact(&mut buf[..n])?;
        stored_hasher.update(&buf[..n]);
        for &byte in &buf[..n] {
            let i = ((crc as u8) ^ byte) as usize;
            crc = (crc >> 8) ^ CRC32C_TABLE[i];
        }
        remaining -= n as u64;
    }
    if (!crc) != descriptor.crc32c {
        return Err(ChunkIoError::ChecksumMismatch(descriptor.id));
    }
    if descriptor
        .blake3
        .is_some_and(|expected| expected != *stored_hasher.finalize().as_bytes())
    {
        return Err(ChunkIoError::ChunkHashMismatch(descriptor.id));
    }
    reader.seek(SeekFrom::Start(
        descriptor
            .offset
            .checked_add(offset)
            .ok_or(ChunkIoError::LengthOverflow)?,
    ))?;
    let mut limited = (&mut *reader).take(length);
    let mut range_buffer = vec![0u8; options.buffer_size];
    let copied = copy_with_buffer(&mut limited, output, &mut range_buffer)?;
    if copied != length {
        return Err(ChunkIoError::LengthMismatch(descriptor.id));
    }
    Ok(copied)
}

/// Computes a BLAKE3 digest of stored bytes using a bounded reusable buffer.
/// For directory-v2 descriptors this must equal the persisted digest; v1
/// descriptors have no persisted per-chunk digest and return the computed value.
pub fn hash_stored_chunk_blake3<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    descriptor: ChunkDescriptor,
    options: ChunkReadOptions,
) -> Result<[u8; 32], ChunkIoError> {
    use std::io::SeekFrom;
    let options = options.validate()?;
    let end = descriptor
        .offset
        .checked_add(descriptor.stored_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    let file_len = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut header_bytes = [0u8; CONTAINER_HEADER_LEN];
    reader.read_exact(&mut header_bytes)?;
    let header = ContainerHeader::decode(&header_bytes).map_err(ChunkIoError::Header)?;
    header
        .validate_file_bounds(file_len)
        .map_err(ChunkIoError::Header)?;
    if descriptor.raw_len > MAX_CHUNK_BYTES || descriptor.stored_len > MAX_STORED_CHUNK_BYTES {
        return Err(ChunkIoError::ChunkTooLarge);
    }
    if is_generation_profile_minor(header.minor) {
        let snapshot = open_generation_snapshot(reader)?;
        let selected = snapshot
            .chunk(descriptor.id)
            .ok_or(ChunkIoError::DirectoryDescriptorMismatch)?;
        if selected != descriptor || end > snapshot.valid_end() {
            return Err(ChunkIoError::DirectoryDescriptorMismatch);
        }
    } else {
        if header.minor > GENERATION_PROFILE_MINOR_VERSION {
            return Err(ChunkIoError::UnsupportedGenerationVersion(header.minor));
        }
        let metadata_end = (CONTAINER_HEADER_LEN as u64)
            .checked_add(header.metadata_len)
            .ok_or(ChunkIoError::LengthOverflow)?;
        if header.directory_offset == 0
            || descriptor.offset < metadata_end
            || end > header.directory_offset
        {
            return Err(ChunkIoError::InvalidDirectory);
        }
    }
    reader.seek(SeekFrom::Start(descriptor.offset))?;
    let mut remaining = descriptor.stored_len;
    let mut buf = vec![0u8; options.buffer_size];
    let mut hasher = blake3::Hasher::new();
    while remaining > 0 {
        let n = usize::try_from(remaining.min(buf.len() as u64))
            .map_err(|_| ChunkIoError::LengthOverflow)?;
        reader.read_exact(&mut buf[..n])?;
        hasher.update(&buf[..n]);
        remaining -= n as u64;
    }
    Ok(*hasher.finalize().as_bytes())
}

/// A reusable in-memory lookup index over validated chunk descriptors.
/// It provides O(log n) ID lookup and a sorted offset view. Semantic time,
/// variable and spatial keys require domain metadata and are deliberately not guessed.
#[derive(Debug, Clone)]
pub struct ChunkIndex {
    by_id: std::collections::BTreeMap<u64, ChunkDescriptor>,
    by_offset: Vec<ChunkDescriptor>,
}

impl ChunkIndex {
    /// Builds the lookup structures from descriptors returned by `read_chunk_directory`.
    pub fn new(descriptors: &[ChunkDescriptor]) -> Result<Self, ChunkIoError> {
        if descriptors.len() as u64 > MAX_CHUNKS {
            return Err(ChunkIoError::TooManyChunks);
        }
        let mut by_id = std::collections::BTreeMap::new();
        let mut by_offset = descriptors.to_vec();
        for descriptor in descriptors {
            if descriptor.raw_len > MAX_CHUNK_BYTES
                || descriptor.stored_len > MAX_STORED_CHUNK_BYTES
                || (descriptor.codec == ChunkCodec::Raw
                    && descriptor.raw_len != descriptor.stored_len)
            {
                return Err(ChunkIoError::ChunkTooLarge);
            }
            if by_id.insert(descriptor.id, *descriptor).is_some() {
                return Err(ChunkIoError::DuplicateChunkId);
            }
        }
        by_offset.sort_by_key(|d| d.offset);
        for pair in by_offset.windows(2) {
            let end = pair[0]
                .offset
                .checked_add(pair[0].stored_len)
                .ok_or(ChunkIoError::LengthOverflow)?;
            if end > pair[1].offset {
                return Err(ChunkIoError::InvalidDirectory);
            }
        }
        Ok(Self { by_id, by_offset })
    }
    /// Returns a descriptor by unique chunk identifier.
    pub fn get(&self, id: u64) -> Option<ChunkDescriptor> {
        self.by_id.get(&id).copied()
    }
    /// Returns descriptors in physical file order for sequential/batched I/O.
    pub fn by_offset(&self) -> &[ChunkDescriptor] {
        &self.by_offset
    }
    /// Number of indexed chunks.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }
    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

/// Visits validated descriptors in bounded batches, in physical file order.
/// This is the scheduling primitive for parallel preparation: each batch contains
/// at most `batch_size` small descriptors, not decoded scientific payloads.
pub fn for_each_chunk_batch<F>(
    index: &ChunkIndex,
    batch_size: usize,
    mut visit: F,
) -> Result<(), ChunkIoError>
where
    F: FnMut(&[ChunkDescriptor]) -> Result<(), ChunkIoError>,
{
    if batch_size == 0 || batch_size > 65_536 {
        return Err(ChunkIoError::InvalidDirectory);
    }
    for batch in index.by_offset().chunks(batch_size) {
        visit(batch)?;
    }
    Ok(())
}

/// Streams an input reader into caller-selected chunk-sized slices. The callback
/// runs once per non-empty chunk; at most `chunk_size` bytes are buffered.
pub fn for_each_input_chunk<R, F>(
    reader: &mut R,
    chunk_size: usize,
    mut consume: F,
) -> Result<u64, ChunkIoError>
where
    R: std::io::Read,
    F: FnMut(u64, &[u8]) -> Result<(), ChunkIoError>,
{
    if chunk_size == 0 || chunk_size as u64 > MAX_CHUNK_BYTES {
        return Err(ChunkIoError::ChunkTooLarge);
    }
    let mut buffer = vec![0u8; chunk_size];
    let mut index = 0u64;
    loop {
        let mut filled = 0usize;
        while filled < buffer.len() {
            let n = reader.read(&mut buffer[filled..])?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled == 0 {
            break;
        }
        consume(index, &buffer[..filled])?;
        index = index.checked_add(1).ok_or(ChunkIoError::LengthOverflow)?;
        if filled < buffer.len() {
            break;
        }
    }
    Ok(index)
}

/// Writes a complete experimental container to a sibling temporary file, calls
/// `sync_all`, and atomically renames it into place. This protects publication of
/// a newly generated file from exposing a partially written destination. It is
/// not an append-only generation/commit-footer recovery protocol.
pub fn write_chunk_container_atomic(
    path: impl AsRef<std::path::Path>,
    metadata: &Metadata,
    chunks: &[EncodedChunkInput<'_>],
    order: HeaderByteOrder,
) -> Result<(), ChunkIoError> {
    use std::io::Write;
    let path = path.as_ref();
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let name = path.file_name().ok_or(ChunkIoError::InvalidDirectory)?;
    let mut temp_name = name.to_os_string();
    temp_name.push(format!(".wx1-tmp-{}", std::process::id()));
    let temp_path = parent.join(temp_name);
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(&temp_path)?;
        write_chunk_container_with_codecs(&mut file, metadata, chunks, order)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp_path, path)?;
        // Directory sync is supported on Unix; on other platforms the rename
        // remains the strongest portable publication primitive available here.
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok::<(), ChunkIoError>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

/// Caller-defined keys for building specialized scientific lookup layers.
/// These keys are application metadata, not inferred from chunk IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScientificChunkKey {
    /// Parameter/variable identifier in the caller's registry namespace.
    pub variable_id: u32,
    /// Time coordinate index (not a Unix timestamp unless caller defines it so).
    pub time_index: u64,
    /// Vertical level index.
    pub level_index: u32,
    /// Spatial tile identifier in the caller's grid scheme.
    pub spatial_tile: u64,
}

/// In-memory specialized lookup map supplied with explicit scientific keys.
/// Persistence and standardized time/spatial key encodings remain future work.
#[derive(Debug, Clone, Default)]
pub struct ScientificChunkIndex {
    entries: std::collections::BTreeMap<ScientificChunkKey, Vec<u64>>,
}
impl ScientificChunkIndex {
    /// Adds a mapping from an explicitly described scientific key to a chunk ID.
    pub fn insert(&mut self, key: ScientificChunkKey, chunk_id: u64) {
        let ids = self.entries.entry(key).or_default();
        if !ids.contains(&chunk_id) {
            ids.push(chunk_id);
            ids.sort_unstable();
        }
    }
    /// Looks up chunk IDs for an exact scientific key.
    pub fn get(&self, key: &ScientificChunkKey) -> &[u64] {
        self.entries.get(key).map(Vec::as_slice).unwrap_or(&[])
    }
    /// Number of distinct scientific keys.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Whether the index has no keys.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Logical namespace for a persistent B+tree. The key's scientific meaning is defined
/// by the caller; v0.3.9 standardizes only the ordinal helpers and byte ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum LogicalIndexKind {
    ChunkId = 1,
    Variable = 2,
    Time = 3,
    Vertical = 4,
    Spatial = 5,
    ModelProvider = 6,
    ScientificManifest = 7,
}

impl LogicalIndexKind {
    fn from_id(value: u16) -> Option<Self> {
        match value {
            1 => Some(Self::ChunkId),
            2 => Some(Self::Variable),
            3 => Some(Self::Time),
            4 => Some(Self::Vertical),
            5 => Some(Self::Spatial),
            6 => Some(Self::ModelProvider),
            7 => Some(Self::ScientificManifest),
            _ => None,
        }
    }
}

/// One persistent logical-index mapping from a 32-byte caller-defined key to a chunk ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistentIndexEntry {
    pub kind: LogicalIndexKind,
    pub key: [u8; 32],
    pub chunk_id: u64,
}

impl PersistentIndexEntry {
    /// Creates a mapping using an already canonicalized fixed-width key.
    pub const fn new(kind: LogicalIndexKind, key: [u8; 32], chunk_id: u64) -> Self {
        Self {
            kind,
            key,
            chunk_id,
        }
    }
    /// Encodes an unsigned 32-bit ordinal as big-endian bytes followed by zero padding.
    pub fn key_u32(value: u32) -> [u8; 32] {
        let mut key = [0u8; 32];
        key[..4].copy_from_slice(&value.to_be_bytes());
        key
    }
    /// Encodes an unsigned 64-bit ordinal as big-endian bytes followed by zero padding.
    pub fn key_u64(value: u64) -> [u8; 32] {
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&value.to_be_bytes());
        key
    }
}

impl ScientificChunkKey {
    /// Converts explicit caller-supplied ordinal fields into variable/time/vertical/spatial mappings.
    /// `time_index`, `level_index`, and `spatial_tile` retain their ordinal meanings; this method
    /// does not infer timestamps, physical units, coordinate reference systems, or geometry.
    pub fn persistent_entries(self, chunk_id: u64) -> [PersistentIndexEntry; 4] {
        [
            PersistentIndexEntry::new(
                LogicalIndexKind::Variable,
                PersistentIndexEntry::key_u32(self.variable_id),
                chunk_id,
            ),
            PersistentIndexEntry::new(
                LogicalIndexKind::Time,
                PersistentIndexEntry::key_u64(self.time_index),
                chunk_id,
            ),
            PersistentIndexEntry::new(
                LogicalIndexKind::Vertical,
                PersistentIndexEntry::key_u32(self.level_index),
                chunk_id,
            ),
            PersistentIndexEntry::new(
                LogicalIndexKind::Spatial,
                PersistentIndexEntry::key_u64(self.spatial_tile),
                chunk_id,
            ),
        ]
    }
}

/// Validated root information for one persistent logical index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexRootInfo {
    pub kind: LogicalIndexKind,
    pub root_offset: u64,
    pub item_count: u64,
    pub level: u16,
    pub required: bool,
}

#[cfg(test)]
mod v037_streaming_tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn zstandard_streams_to_writer_without_full_chunk_api() {
        let raw = vec![b'W'; 2 * 1024 * 1024];
        let chunks = [EncodedChunkInput {
            id: 71,
            data: &raw,
            codec: ChunkCodec::Zstandard,
        }];
        let mut file = Cursor::new(Vec::new());
        write_chunk_container_with_codecs(
            &mut file,
            &Metadata::new("stream-test"),
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let (_, descriptors) = read_chunk_directory(&mut file).unwrap();
        let mut out = Vec::new();
        let copied = copy_chunk_payload_to_writer(
            &mut file,
            descriptors[0],
            &mut out,
            ChunkReadOptions {
                buffer_size: 32 * 1024,
            },
        )
        .unwrap();
        assert_eq!(copied, raw.len() as u64);
        assert_eq!(out, raw);
    }

    #[test]
    fn raw_range_reads_only_requested_bytes_and_verifies_crc() {
        let raw = b"0123456789abcdefghijklmnopqrstuvwxyz";
        let chunks = [ChunkInput { id: 72, data: raw }];
        let mut file = Cursor::new(Vec::new());
        write_chunk_container(
            &mut file,
            &Metadata::new("range-test"),
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let (_, descriptors) = read_chunk_directory(&mut file).unwrap();
        let mut out = Vec::new();
        let n = read_raw_chunk_range(
            &mut file,
            descriptors[0],
            7,
            9,
            &mut out,
            ChunkReadOptions::default(),
        )
        .unwrap();
        assert_eq!(n, 9);
        assert_eq!(out, b"789abcdef");
    }

    #[test]
    fn streaming_buffer_limits_are_enforced() {
        assert!(ChunkReadOptions { buffer_size: 0 }.validate().is_err());
        assert!(ChunkReadOptions {
            buffer_size: MAX_STREAM_BUFFER_BYTES + 1
        }
        .validate()
        .is_err());
    }

    #[test]
    fn blake3_hash_is_stable_and_index_supports_batch_planning() {
        let chunks = [
            ChunkInput {
                id: 5,
                data: b"alpha",
            },
            ChunkInput {
                id: 6,
                data: b"beta",
            },
        ];
        let mut file = Cursor::new(Vec::new());
        write_chunk_container(
            &mut file,
            &Metadata::new("index-test"),
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let (_, descriptors) = read_chunk_directory(&mut file).unwrap();
        let index = ChunkIndex::new(&descriptors).unwrap();
        assert_eq!(index.get(6).unwrap().id, 6);
        let hash = hash_stored_chunk_blake3(&mut file, descriptors[0], ChunkReadOptions::default())
            .unwrap();
        assert_eq!(hash, *blake3::hash(b"alpha").as_bytes());
        let mut batches = 0;
        for_each_chunk_batch(&index, 1, |_| {
            batches += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(batches, 2);
    }

    #[test]
    fn configurable_input_chunking_is_bounded_and_complete() {
        let mut input = Cursor::new(vec![7u8; 25]);
        let mut lengths = Vec::new();
        let count = for_each_input_chunk(&mut input, 8, |_, bytes| {
            lengths.push(bytes.len());
            Ok(())
        })
        .unwrap();
        assert_eq!(count, 4);
        assert_eq!(lengths, vec![8, 8, 8, 1]);
    }

    #[test]
    fn writes_chunks_directly_from_readers() {
        let raw = vec![b'R'; 512 * 1024];
        let mut source = Cursor::new(raw.clone());
        let mut inputs = [ChunkReaderInput {
            id: 88,
            reader: &mut source,
            raw_len: raw.len() as u64,
            codec: ChunkCodec::Zstandard,
        }];
        let mut file = Cursor::new(Vec::new());
        write_chunk_container_from_readers(
            &mut file,
            &Metadata::new("reader-source"),
            &mut inputs,
            HeaderByteOrder::LittleEndian,
            64 * 1024,
        )
        .unwrap();
        let (_, descriptors) = read_chunk_directory(&mut file).unwrap();
        let mut output = Vec::new();
        copy_chunk_payload_to_writer(
            &mut file,
            descriptors[0],
            &mut output,
            ChunkReadOptions::default(),
        )
        .unwrap();
        assert_eq!(output, raw);
    }

    #[test]
    fn scientific_index_uses_explicit_keys() {
        let key = ScientificChunkKey {
            variable_id: 130,
            time_index: 12,
            level_index: 4,
            spatial_tile: 99,
        };
        let mut index = ScientificChunkIndex::default();
        index.insert(key, 10);
        index.insert(key, 11);
        assert_eq!(index.get(&key), &[10, 11]);
    }
}

/// A chunk supplied as a streaming reader instead of a caller-owned byte slice.
pub struct ChunkReaderInput<'a> {
    /// Unique identifier in this container.
    pub id: u64,
    /// Source stream. Exactly `raw_len` bytes are consumed.
    pub reader: &'a mut dyn std::io::Read,
    /// Declared uncompressed length, bounded by `MAX_CHUNK_BYTES`.
    pub raw_len: u64,
    /// Lossless codec.
    pub codec: ChunkCodec,
}

/// Configurable limits for reader-based generation writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkWriteOptions {
    /// Reused input/output copy buffer, between 1 byte and 8 MiB.
    pub buffer_size: usize,
}

/// Maximum stream buffer accepted by the generation writer (8 MiB).
pub const MAX_STREAM_WRITE_BUFFER_BYTES: usize = MAX_STREAM_BUFFER_BYTES;

impl Default for ChunkWriteOptions {
    fn default() -> Self {
        Self {
            buffer_size: 256 * 1024,
        }
    }
}

impl ChunkWriteOptions {
    /// Validates the buffer size before any destination bytes are written.
    pub fn validate(self) -> Result<Self, ChunkIoError> {
        if self.buffer_size == 0 || self.buffer_size > MAX_STREAM_WRITE_BUFFER_BYTES {
            return Err(ChunkIoError::InvalidStreamBufferSize);
        }
        Ok(self)
    }
}

struct ReaderGenerationWriteInput<'a, 'source> {
    metadata: &'a Metadata,
    chunks: &'a mut [ChunkReaderInput<'source>],
    index_entries: &'a [PersistentIndexEntry],
    order: HeaderByteOrder,
    options: ChunkWriteOptions,
}

struct ReaderGenerationRecordsInput<'a, 'source> {
    chunks: &'a mut [ChunkReaderInput<'source>],
    index_entries: &'a [PersistentIndexEntry],
    order: HeaderByteOrder,
    commit: GenerationCommitInfo,
    boolean_chunk_ids: &'a std::collections::BTreeSet<u64>,
    stream_buffer: &'a mut [u8],
}

struct CountingCrcWriter<'a, W: std::io::Write> {
    inner: &'a mut W,
    crc: u32,
    count: u64,
}
impl<'a, W: std::io::Write> CountingCrcWriter<'a, W> {
    fn new(inner: &'a mut W) -> Self {
        Self {
            inner,
            crc: !0u32,
            count: 0,
        }
    }
    fn checksum(&self) -> u32 {
        !self.crc
    }
}
impl<W: std::io::Write> std::io::Write for CountingCrcWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(bytes)?;
        for &byte in &bytes[..n] {
            let i = ((self.crc as u8) ^ byte) as usize;
            self.crc = (self.crc >> 8) ^ CRC32C_TABLE[i];
        }
        self.count = self
            .count
            .checked_add(n as u64)
            .ok_or_else(|| std::io::Error::other("stored chunk length overflow"))?;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

struct CountingIntegrityWriter<'a, W: std::io::Write> {
    inner: &'a mut W,
    crc: u32,
    count: u64,
    chunk_hasher: blake3::Hasher,
    generation_hasher: &'a mut blake3::Hasher,
}

impl<'a, W: std::io::Write> CountingIntegrityWriter<'a, W> {
    fn new(inner: &'a mut W, generation_hasher: &'a mut blake3::Hasher) -> Self {
        Self {
            inner,
            crc: !0u32,
            count: 0,
            chunk_hasher: blake3::Hasher::new(),
            generation_hasher,
        }
    }

    fn result(&self) -> (u64, u32, [u8; 32]) {
        (
            self.count,
            !self.crc,
            *self.chunk_hasher.clone().finalize().as_bytes(),
        )
    }
}

impl<W: std::io::Write> std::io::Write for CountingIntegrityWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(bytes)?;
        let written = &bytes[..n];
        for &byte in written {
            let index = ((self.crc as u8) ^ byte) as usize;
            self.crc = (self.crc >> 8) ^ CRC32C_TABLE[index];
        }
        self.count = self
            .count
            .checked_add(n as u64)
            .ok_or_else(|| std::io::Error::other("stored chunk length overflow"))?;
        self.chunk_hasher.update(written);
        self.generation_hasher.update(written);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

enum StreamCopyError {
    Input(std::io::Error),
    Output(std::io::Error),
    InvalidBooleanValue,
}

fn copy_exactly_n_checked<R, W>(
    reader: &mut R,
    writer: &mut W,
    length: u64,
    buffer: &mut [u8],
    require_boolean_bytes: bool,
) -> Result<u64, StreamCopyError>
where
    R: std::io::Read + ?Sized,
    W: std::io::Write + ?Sized,
{
    let mut remaining = length;
    let mut total = 0u64;
    while remaining > 0 {
        let wanted = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| StreamCopyError::Input(std::io::Error::other("copy length overflow")))?;
        let count = match reader.read(&mut buffer[..wanted]) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(StreamCopyError::Input(error)),
        };
        if count == 0 {
            return Err(StreamCopyError::Input(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "input chunk ended before declared length",
            )));
        }
        if require_boolean_bytes && buffer[..count].iter().any(|value| *value > 1) {
            return Err(StreamCopyError::InvalidBooleanValue);
        }
        writer
            .write_all(&buffer[..count])
            .map_err(StreamCopyError::Output)?;
        remaining -= count as u64;
        total = total
            .checked_add(count as u64)
            .ok_or_else(|| StreamCopyError::Input(std::io::Error::other("copy length overflow")))?;
    }
    Ok(total)
}

fn allocate_stream_buffer(size: usize) -> Result<Vec<u8>, ChunkIoError> {
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(size)
        .map_err(|_| ChunkIoError::AllocationFailed)?;
    buffer.resize(size, 0);
    Ok(buffer)
}

/// Writes chunks directly from input readers. Raw and Zstandard sources are
/// streamed with bounded internal buffers; only LZ4 block encoding currently
/// materializes one chunk (up to 64 MiB). The output must be new or truncated.
pub fn write_chunk_container_from_readers<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    metadata: &Metadata,
    chunks: &mut [ChunkReaderInput<'_>],
    order: HeaderByteOrder,
    buffer_size: usize,
) -> Result<(), ChunkIoError> {
    use std::io::{SeekFrom, Write};
    if metadata.scientific_model().is_some() {
        return Err(ChunkIoError::Metadata(
            MetadataError::ScientificModelRequiresGeneration,
        ));
    }
    if buffer_size == 0 || buffer_size > MAX_STREAM_BUFFER_BYTES {
        return Err(ChunkIoError::InvalidDirectory);
    }
    if chunks.len() as u64 > MAX_CHUNKS {
        return Err(ChunkIoError::TooManyChunks);
    }
    let metadata_bytes = metadata.to_cbor().map_err(ChunkIoError::Metadata)?;
    let mut ids = std::collections::BTreeSet::new();
    for chunk in chunks.iter() {
        if !ids.insert(chunk.id) {
            return Err(ChunkIoError::DuplicateChunkId);
        }
        if chunk.raw_len > MAX_CHUNK_BYTES {
            return Err(ChunkIoError::ChunkTooLarge);
        }
    }
    let placeholder = ContainerHeader {
        major: 1,
        minor: 3,
        byte_order: order,
        flags: 0,
        metadata_len: metadata_bytes.len() as u64,
        directory_offset: 0,
    };
    writer.seek(SeekFrom::Start(0))?;
    writer.write_all(&placeholder.encode())?;
    writer.write_all(&metadata_bytes)?;
    let mut descriptors = Vec::with_capacity(chunks.len());
    for chunk in chunks.iter_mut() {
        let offset = writer.stream_position()?;
        let (stored_len, checksum) = {
            let mut counted = CountingCrcWriter::new(writer);
            match chunk.codec {
                ChunkCodec::Raw => {
                    let mut stream_buffer = vec![0u8; buffer_size];
                    let copied = copy_exactly_n(
                        chunk.reader,
                        &mut counted,
                        chunk.raw_len,
                        &mut stream_buffer,
                    )?;
                    if copied != chunk.raw_len {
                        return Err(ChunkIoError::LengthMismatch(chunk.id));
                    }
                }
                ChunkCodec::Zstandard => {
                    let mut encoder = zstd::stream::write::Encoder::new(&mut counted, 3)
                        .map_err(|e| ChunkIoError::Compression(e.to_string()))?;
                    let mut stream_buffer = vec![0u8; buffer_size];
                    let copied = copy_exactly_n(
                        chunk.reader,
                        &mut encoder,
                        chunk.raw_len,
                        &mut stream_buffer,
                    )
                    .map_err(|e| ChunkIoError::Compression(e.to_string()))?;
                    if copied != chunk.raw_len {
                        return Err(ChunkIoError::LengthMismatch(chunk.id));
                    }
                    encoder
                        .finish()
                        .map_err(|e| ChunkIoError::Compression(e.to_string()))?;
                }
                ChunkCodec::Lz4 => {
                    let raw_size =
                        usize::try_from(chunk.raw_len).map_err(|_| ChunkIoError::ChunkTooLarge)?;
                    let mut raw = vec![0u8; raw_size];
                    chunk.reader.read_exact(&mut raw)?;
                    let encoded = lz4_flex::block::compress(&raw);
                    counted.write_all(&encoded)?;
                }
                ChunkCodec::ZstdByteShuffle4
                | ChunkCodec::ZstdByteShuffle8
                | ChunkCodec::ZstdByteShuffle2 => {
                    let item_size = match chunk.codec {
                        ChunkCodec::ZstdByteShuffle4 => 4,
                        ChunkCodec::ZstdByteShuffle8 => 8,
                        ChunkCodec::ZstdByteShuffle2 => 2,
                        _ => unreachable!(),
                    };
                    let raw_size =
                        usize::try_from(chunk.raw_len).map_err(|_| ChunkIoError::ChunkTooLarge)?;
                    let mut raw = Vec::new();
                    raw.try_reserve_exact(raw_size)
                        .map_err(|_| ChunkIoError::AllocationFailed)?;
                    raw.resize(raw_size, 0);
                    chunk.reader.read_exact(&mut raw)?;
                    let shuffled = quantization::byte_shuffle(&raw, item_size)?;
                    let mut encoder = zstd::stream::write::Encoder::new(&mut counted, 3)
                        .map_err(|e| ChunkIoError::Compression(e.to_string()))?;
                    encoder
                        .write_all(&shuffled)
                        .map_err(|e| ChunkIoError::Compression(e.to_string()))?;
                    encoder
                        .finish()
                        .map_err(|e| ChunkIoError::Compression(e.to_string()))?;
                }
            }
            counted.flush()?;
            (counted.count, counted.checksum())
        };
        if stored_len > MAX_STORED_CHUNK_BYTES {
            return Err(ChunkIoError::ChunkTooLarge);
        }
        descriptors.push(ChunkDescriptor {
            id: chunk.id,
            offset,
            stored_len,
            raw_len: chunk.raw_len,
            crc32c: checksum,
            codec: chunk.codec,
            blake3: None,
        });
    }
    let directory_offset = writer.stream_position()?;
    let directory_len = CHUNK_DIRECTORY_HEADER_LEN
        .checked_add(
            (chunks.len() as u64)
                .checked_mul(CHUNK_DESCRIPTOR_LEN)
                .ok_or(ChunkIoError::LengthOverflow)?,
        )
        .ok_or(ChunkIoError::LengthOverflow)?;
    directory_offset
        .checked_add(directory_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    let mut directory = Vec::with_capacity(
        usize::try_from(directory_len).map_err(|_| ChunkIoError::LengthOverflow)?,
    );
    directory.extend_from_slice(&CHUNK_DIRECTORY_MAGIC);
    put_u16(&mut directory, 1, order);
    put_u16(&mut directory, CHUNK_DESCRIPTOR_LEN as u16, order);
    put_u32(&mut directory, descriptors.len() as u32, order);
    put_u32(&mut directory, 0, order);
    for d in descriptors {
        put_u64(&mut directory, d.id, order);
        put_u64(&mut directory, d.offset, order);
        put_u64(&mut directory, d.stored_len, order);
        put_u64(&mut directory, d.raw_len, order);
        put_u32(&mut directory, d.crc32c, order);
        directory.push(d.codec as u8);
        directory.push(0);
        put_u16(&mut directory, 0, order);
    }
    writer.write_all(&directory)?;
    writer.seek(SeekFrom::Start(0))?;
    writer.write_all(
        &ContainerHeader {
            directory_offset,
            ..placeholder
        }
        .encode(),
    )?;
    writer.flush()?;
    Ok(())
}

/// Fixed-size BLAKE3 commit footer appended after the chunk directory.
pub const COMMIT_FOOTER_LEN: u64 = 48;
pub const COMMIT_FOOTER_MAGIC: [u8; 4] = *b"WX1C";

fn hash_reader_prefix<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    length: u64,
) -> Result<[u8; 32], ChunkIoError> {
    use std::io::SeekFrom;
    reader.seek(SeekFrom::Start(0))?;
    let mut remaining = length;
    let mut buffer = vec![0u8; 256 * 1024];
    let mut hasher = blake3::Hasher::new();
    while remaining > 0 {
        let n = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| ChunkIoError::LengthOverflow)?;
        reader.read_exact(&mut buffer[..n])?;
        hasher.update(&buffer[..n]);
        remaining -= n as u64;
    }
    Ok(*hasher.finalize().as_bytes())
}

fn verify_commit_footer<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    committed_len: u64,
) -> Result<(), ChunkIoError> {
    use std::io::SeekFrom;
    let footer_offset = committed_len;
    reader.seek(SeekFrom::Start(footer_offset))?;
    let mut footer = [0u8; COMMIT_FOOTER_LEN as usize];
    reader
        .read_exact(&mut footer)
        .map_err(|_| ChunkIoError::CommitFooterInvalid)?;
    if footer[..4] != COMMIT_FOOTER_MAGIC
        || u16::from_le_bytes([footer[4], footer[5]]) != 1
        || footer[6..8] != [0, 0]
    {
        return Err(ChunkIoError::CommitFooterInvalid);
    }
    let declared_len = u64::from_le_bytes(
        footer[8..16]
            .try_into()
            .map_err(|_| ChunkIoError::CommitFooterInvalid)?,
    );
    if declared_len != committed_len {
        return Err(ChunkIoError::CommitFooterInvalid);
    }
    let actual = hash_reader_prefix(reader, committed_len)?;
    if actual != footer[16..48] {
        return Err(ChunkIoError::CommitHashMismatch);
    }
    Ok(())
}

/// Appends a BLAKE3 commit footer to a freshly written container. The footer
/// authenticates every preceding byte against accidental corruption; it is not
/// a keyed signature and does not protect against a malicious rewrite.
pub fn append_commit_footer<W: std::io::Read + std::io::Write + std::io::Seek>(
    writer: &mut W,
) -> Result<(), ChunkIoError> {
    use std::io::SeekFrom;
    let committed_len = writer.seek(SeekFrom::End(0))?;
    let digest = hash_reader_prefix(writer, committed_len)?;
    writer.seek(SeekFrom::Start(committed_len))?;
    writer.write_all(&COMMIT_FOOTER_MAGIC)?;
    writer.write_all(&1u16.to_le_bytes())?;
    writer.write_all(&[0, 0])?;
    writer.write_all(&committed_len.to_le_bytes())?;
    writer.write_all(&digest)?;
    writer.flush()?;
    Ok(())
}

/// Writes a container followed by its BLAKE3 commit footer. Readers verify the
/// footer before trusting its directory. The output must be new or truncated.
pub fn write_committed_chunk_container<W: std::io::Read + std::io::Write + std::io::Seek>(
    writer: &mut W,
    metadata: &Metadata,
    chunks: &[EncodedChunkInput<'_>],
    order: HeaderByteOrder,
) -> Result<(), ChunkIoError> {
    write_chunk_container_with_codecs(writer, metadata, chunks, order)?;
    append_commit_footer(writer)
}

/// Crash-safe publication variant: writes a committed file to a sibling temporary
/// file, syncs its contents, and atomically renames it to the requested path.
pub fn write_committed_chunk_container_atomic(
    path: impl AsRef<std::path::Path>,
    metadata: &Metadata,
    chunks: &[EncodedChunkInput<'_>],
    order: HeaderByteOrder,
) -> Result<(), ChunkIoError> {
    let path = path.as_ref();
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let name = path.file_name().ok_or(ChunkIoError::InvalidDirectory)?;
    let mut temp_name = name.to_os_string();
    temp_name.push(format!(".wx1-commit-tmp-{}", std::process::id()));
    let temp_path = parent.join(temp_name);
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        write_committed_chunk_container(&mut file, metadata, chunks, order)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp_path, path)?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok::<(), ChunkIoError>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

#[cfg(test)]
mod v037_commit_tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn committed_container_roundtrips_and_verifies_blake3_footer() {
        let chunks = [EncodedChunkInput {
            id: 91,
            data: b"committed payload",
            codec: ChunkCodec::Zstandard,
        }];
        let mut file = Cursor::new(Vec::new());
        write_committed_chunk_container(
            &mut file,
            &Metadata::new("commit-test"),
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let (_, descriptors) = read_chunk_directory(&mut file).unwrap();
        assert_eq!(
            read_chunk_payload(&mut file, descriptors[0]).unwrap(),
            b"committed payload"
        );
        let last = file.get_ref().len() - 1;
        file.get_mut()[last] ^= 1;
        assert!(matches!(
            read_chunk_directory(&mut file),
            Err(ChunkIoError::CommitHashMismatch)
        ));
    }

    #[test]
    fn rejects_invalid_commit_footer_structure_separately() {
        let chunks = [EncodedChunkInput {
            id: 92,
            data: b"footer structure",
            codec: ChunkCodec::Raw,
        }];
        let mut file = Cursor::new(Vec::new());
        write_committed_chunk_container(
            &mut file,
            &Metadata::new("commit-footer-structure"),
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let footer_offset = file.get_ref().len() - COMMIT_FOOTER_LEN as usize;
        file.get_mut()[footer_offset] ^= 1;
        assert!(matches!(
            read_chunk_directory(&mut file),
            Err(ChunkIoError::CommitFooterInvalid)
        ));
    }

    #[test]
    fn rejects_truncated_commit_footer_separately() {
        let chunks = [EncodedChunkInput {
            id: 93,
            data: b"footer truncation",
            codec: ChunkCodec::Raw,
        }];
        let mut file = Cursor::new(Vec::new());
        write_committed_chunk_container(
            &mut file,
            &Metadata::new("commit-footer-truncated"),
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let truncated_len = file.get_ref().len() - 8;
        file.get_mut().truncate(truncated_len);
        assert!(matches!(
            read_chunk_directory(&mut file),
            Err(ChunkIoError::CommitFooterInvalid)
        ));
    }
}

/// Current v0.4.0 generation profile: directory v2, footer v2 and scientific metadata schema v2.
pub const GENERATION_PROFILE_MINOR_VERSION: u16 = 6;
/// v0.3.8 append-only generation profile (footer v1).
pub const LEGACY_GENERATION_PROFILE_MINOR_VERSION: u16 = 4;
/// v0.3.9 persistent-index profile (footer v2, metadata schema v1).
pub const PERSISTENT_INDEX_PROFILE_MINOR_VERSION: u16 = 5;

fn is_generation_profile_minor(minor: u16) -> bool {
    minor == LEGACY_GENERATION_PROFILE_MINOR_VERSION
        || minor == PERSISTENT_INDEX_PROFILE_MINOR_VERSION
        || minor == GENERATION_PROFILE_MINOR_VERSION
}
fn is_index_generation_minor(minor: u16) -> bool {
    minor == PERSISTENT_INDEX_PROFILE_MINOR_VERSION || minor == GENERATION_PROFILE_MINOR_VERSION
}
/// Directory v2 uses a 16-byte header and 72-byte chunk descriptors.
pub const GENERATION_DIRECTORY_VERSION: u16 = 2;
pub const GENERATION_DESCRIPTOR_LEN: u64 = 72;
/// Footer v2 adds an index-catalog pointer to the v0.3.8 64-byte footer.
pub const GENERATION_FOOTER_LEN: u64 = 80;
pub const LEGACY_GENERATION_FOOTER_LEN: u64 = 64;
pub const GENERATION_FOOTER_MAGIC: [u8; 4] = *b"WX1G";
/// Fixed on-disk size of every persistent B+tree/catalog page.
pub const INDEX_PAGE_SIZE: usize = 4096;
const INDEX_PAGE_HEADER_LEN: usize = 64;
const INDEX_PAGE_MAGIC: [u8; 4] = *b"WX1B";
const INDEX_PAGE_VERSION: u16 = 1;
const INDEX_PAGE_LEAF: u8 = 1;
const INDEX_PAGE_INTERNAL: u8 = 2;
const INDEX_PAGE_CATALOG: u8 = 3;
const INDEX_PAGE_MANIFEST: u8 = 4;
const INDEX_LEAF_ENTRY_LEN: usize = 40;
const INDEX_INTERNAL_ENTRY_LEN: usize = 48;
const INDEX_CATALOG_ENTRY_LEN: usize = 24;
const INDEX_LEAF_CAPACITY: usize = (INDEX_PAGE_SIZE - INDEX_PAGE_HEADER_LEN) / INDEX_LEAF_ENTRY_LEN;
const INDEX_INTERNAL_CAPACITY: usize =
    (INDEX_PAGE_SIZE - INDEX_PAGE_HEADER_LEN - 8) / INDEX_INTERNAL_ENTRY_LEN;
const MAX_INDEX_CATALOG_ENTRIES: usize = 64;
const MAX_INDEX_TREE_DEPTH: u16 = 64;
const MAX_INDEX_TREE_PAGES: u64 = 1_000_000;
const MAX_INDEX_TREE_ENTRIES: u64 = MAX_CHUNKS;
const INDEX_PAGE_DIGEST_RANGE: std::ops::Range<usize> = 24..56;
const MAX_GENERATION_CHAIN: u64 = 1_000_000;
const MAX_RECOVERY_CANDIDATES: u64 = 65_536;
const RECOVERY_SCAN_BLOCK_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy)]
struct GenerationFooter {
    generation: u64,
    prev_footer_offset: u64,
    directory_offset: u64,
    index_catalog_offset: u64,
    digest: [u8; 32],
}

/// A validated, immutable view pinned to one committed generation footer.
/// Appending later generations does not mutate this snapshot's descriptor set.
#[derive(Debug, Clone)]
pub struct GenerationSnapshot {
    metadata: Metadata,
    descriptors: Vec<ChunkDescriptor>,
    generation: u64,
    footer_offset: u64,
    directory_offset: u64,
    valid_end: u64,
    byte_order: HeaderByteOrder,
    profile_minor: u16,
    index_catalog: Vec<IndexCatalogEntry>,
    index_page_regions: Vec<IndexPageRegion>,
}

impl GenerationSnapshot {
    /// Metadata from the static, validated container preamble.
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    /// All descriptors visible at the pinned generation, ordered generation-first.
    pub fn descriptors(&self) -> &[ChunkDescriptor] {
        &self.descriptors
    }
    /// The pinned generation number, starting at one.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Absolute offset of this snapshot's footer.
    pub fn footer_offset(&self) -> u64 {
        self.footer_offset
    }
    /// Absolute offset of the directory for this generation.
    pub fn directory_offset(&self) -> u64 {
        self.directory_offset
    }
    /// Exclusive end offset of the pinned footer. Later trailing bytes are not included.
    pub fn valid_end(&self) -> u64 {
        self.valid_end
    }
    /// Looks up a descriptor visible in this snapshot.
    pub fn chunk(&self, id: u64) -> Option<ChunkDescriptor> {
        if let Some(first) = self.descriptors.first() {
            if let Some(idx) = id
                .checked_sub(first.id)
                .and_then(|delta| usize::try_from(delta).ok())
            {
                if let Some(descriptor) = self.descriptors.get(idx) {
                    if descriptor.id == id {
                        return Some(*descriptor);
                    }
                }
            }
        }
        self.descriptors
            .iter()
            .find(|descriptor| descriptor.id == id)
            .copied()
    }
    /// Returns information about a logical persistent index, if it is present.
    pub fn index_info(&self, kind: LogicalIndexKind) -> Option<IndexRootInfo> {
        self.index_catalog
            .iter()
            .find(|entry| entry.kind_id == kind as u16)
            .map(|entry| IndexRootInfo {
                kind,
                root_offset: entry.root_offset,
                item_count: entry.item_count,
                level: entry.level,
                required: entry.flags & 1 != 0,
            })
    }
    /// Returns all chunk IDs mapped to one exact opaque 32-byte key.
    pub fn lookup_index_exact<R: std::io::Read + std::io::Seek>(
        &self,
        reader: &mut R,
        kind: LogicalIndexKind,
        key: &[u8; 32],
    ) -> Result<Vec<u64>, ChunkIoError> {
        self.lookup_index_range(reader, kind, key, key)
    }
    /// Returns chunk IDs whose opaque 32-byte keys lie in the inclusive range.
    /// The result is ordered by key bytes and then by numeric chunk ID.
    pub fn lookup_index_range<R: std::io::Read + std::io::Seek>(
        &self,
        reader: &mut R,
        kind: LogicalIndexKind,
        lower: &[u8; 32],
        upper: &[u8; 32],
    ) -> Result<Vec<u64>, ChunkIoError> {
        lookup_persistent_index_range(self, reader, kind, lower, upper)
    }
    /// Reads and validates one chunk visible in this pinned snapshot.
    pub fn read_chunk<R: std::io::Read + std::io::Seek>(
        &self,
        reader: &mut R,
        chunk_id: u64,
    ) -> Result<Vec<u8>, ChunkIoError> {
        self.read_chunk_mode(reader, chunk_id, true)
    }

    /// Reads and validates one chunk, optionally skipping per-chunk BLAKE3 verification
    /// (relying on CRC32C during fast reads while `validate_full` verifies BLAKE3).
    pub fn read_chunk_mode<R: std::io::Read + std::io::Seek>(
        &self,
        reader: &mut R,
        chunk_id: u64,
        verify_blake3: bool,
    ) -> Result<Vec<u8>, ChunkIoError> {
        let descriptor = self
            .chunk(chunk_id)
            .ok_or(ChunkIoError::ChunkNotFound(chunk_id))?;
        let stored = read_generation_chunk_stored_bytes(reader, descriptor, self.valid_end)?;
        verify_and_decode_generation_chunk_mode(descriptor, &stored, verify_blake3)
    }

    /// Reads, validates, and decompresses multiple chunks in bounded parallel batches.
    /// Compressed payloads are read sequentially from `reader` in batches of at most 8 chunks
    /// (preserving strict memory bounds), then verified (CRC32C + BLAKE3) and decompressed
    /// across worker threads.
    pub fn read_chunks_parallel<R: std::io::Read + std::io::Seek>(
        &self,
        reader: &mut R,
        chunk_ids: &[u64],
    ) -> Result<Vec<Vec<u8>>, ChunkIoError> {
        self.read_chunks_parallel_mode(reader, chunk_ids, true)
    }

    /// Reads, validates, and decompresses multiple chunks in bounded parallel batches,
    /// optionally skipping per-chunk BLAKE3 verification (verifying CRC32C only).
    pub fn read_chunks_parallel_mode<R: std::io::Read + std::io::Seek>(
        &self,
        reader: &mut R,
        chunk_ids: &[u64],
        verify_blake3: bool,
    ) -> Result<Vec<Vec<u8>>, ChunkIoError> {
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }
        if chunk_ids.len() == 1 {
            return Ok(vec![self.read_chunk_mode(
                reader,
                chunk_ids[0],
                verify_blake3,
            )?]);
        }
        const BATCH_SIZE: usize = 8;
        let mut results = Vec::with_capacity(chunk_ids.len());
        for batch in chunk_ids.chunks(BATCH_SIZE) {
            let mut raw_batch = Vec::with_capacity(batch.len());
            let mut total_stored = 0usize;
            for &chunk_id in batch {
                let descriptor = self
                    .chunk(chunk_id)
                    .ok_or(ChunkIoError::ChunkNotFound(chunk_id))?;
                let stored =
                    read_generation_chunk_stored_bytes(reader, descriptor, self.valid_end)?;
                total_stored += stored.len();
                raw_batch.push((descriptor, stored));
            }
            // Caveman + Ponytail: do not spawn threads if batch is small or total payload < 512 KiB.
            // Spawning threads for small payloads introduces thread spawn/synchronization overhead.
            let decoded_batch: Vec<Result<Vec<u8>, ChunkIoError>> = if raw_batch.len() <= 2
                || total_stored < 512 * 1024
            {
                raw_batch
                    .iter()
                    .map(|(desc, stored)| {
                        verify_and_decode_generation_chunk_mode(*desc, stored, verify_blake3)
                    })
                    .collect()
            } else {
                std::thread::scope(|s| {
                    let handles: Vec<_> = raw_batch
                        .iter()
                        .map(|(descriptor, stored)| {
                            let desc = *descriptor;
                            s.spawn(move || {
                                verify_and_decode_generation_chunk_mode(desc, stored, verify_blake3)
                            })
                        })
                        .collect();
                    handles
                        .into_iter()
                        .map(|h| {
                            h.join().unwrap_or_else(|_| {
                                Err(ChunkIoError::Decompression(
                                    "worker thread panicked".to_owned(),
                                ))
                            })
                        })
                        .collect()
                })
            };
            for res in decoded_batch {
                results.push(res?);
            }
        }
        Ok(results)
    }
}

/// Result of explicitly truncating an invalid tail after the newest valid generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairReport {
    pub generation: u64,
    pub original_len: u64,
    pub retained_len: u64,
}

fn order_u16(bytes: &[u8], order: HeaderByteOrder) -> Result<u16, ChunkIoError> {
    read_u16_order(bytes, order)
}

fn order_u64(bytes: &[u8], order: HeaderByteOrder) -> Result<u64, ChunkIoError> {
    read_u64_order(bytes, order)
}

fn generation_footer_len(profile_minor: u16) -> Result<u64, ChunkIoError> {
    match profile_minor {
        LEGACY_GENERATION_PROFILE_MINOR_VERSION => Ok(LEGACY_GENERATION_FOOTER_LEN),
        PERSISTENT_INDEX_PROFILE_MINOR_VERSION | GENERATION_PROFILE_MINOR_VERSION => {
            Ok(GENERATION_FOOTER_LEN)
        }
        other => Err(ChunkIoError::UnsupportedGenerationVersion(other)),
    }
}

fn encode_generation_footer(
    footer: GenerationFooter,
    order: HeaderByteOrder,
) -> [u8; GENERATION_FOOTER_LEN as usize] {
    let mut bytes = Vec::with_capacity(GENERATION_FOOTER_LEN as usize);
    bytes.extend_from_slice(&GENERATION_FOOTER_MAGIC);
    put_u16(&mut bytes, 2, order);
    put_u16(&mut bytes, GENERATION_FOOTER_LEN as u16, order);
    put_u64(&mut bytes, footer.generation, order);
    put_u64(&mut bytes, footer.prev_footer_offset, order);
    put_u64(&mut bytes, footer.directory_offset, order);
    put_u64(&mut bytes, footer.index_catalog_offset, order);
    bytes.extend_from_slice(&footer.digest);
    put_u64(&mut bytes, 0, order);
    let mut result = [0u8; GENERATION_FOOTER_LEN as usize];
    result.copy_from_slice(&bytes);
    result
}

#[cfg(test)]
fn encode_generation_footer_v038(
    footer: GenerationFooter,
    order: HeaderByteOrder,
) -> [u8; LEGACY_GENERATION_FOOTER_LEN as usize] {
    let mut bytes = Vec::with_capacity(LEGACY_GENERATION_FOOTER_LEN as usize);
    bytes.extend_from_slice(&GENERATION_FOOTER_MAGIC);
    put_u16(&mut bytes, 1, order);
    put_u16(&mut bytes, LEGACY_GENERATION_FOOTER_LEN as u16, order);
    put_u64(&mut bytes, footer.generation, order);
    put_u64(&mut bytes, footer.prev_footer_offset, order);
    put_u64(&mut bytes, footer.directory_offset, order);
    bytes.extend_from_slice(&footer.digest);
    let mut result = [0u8; LEGACY_GENERATION_FOOTER_LEN as usize];
    result.copy_from_slice(&bytes);
    result
}

fn read_generation_footer_at<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    offset: u64,
    file_len: u64,
    metadata_end: u64,
    order: HeaderByteOrder,
    profile_minor: u16,
) -> Result<GenerationFooter, ChunkIoError> {
    use std::io::SeekFrom;
    let footer_len = generation_footer_len(profile_minor)?;
    let end = offset
        .checked_add(footer_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    if end > file_len {
        return Err(ChunkIoError::GenerationFooterInvalid);
    }
    let read_len =
        usize::try_from(footer_len).map_err(|_| ChunkIoError::GenerationFooterInvalid)?;
    let mut bytes = vec![0u8; read_len];
    reader.seek(SeekFrom::Start(offset))?;
    if let Err(error) = reader.read_exact(&mut bytes) {
        return Err(if error.kind() == std::io::ErrorKind::UnexpectedEof {
            ChunkIoError::GenerationFooterInvalid
        } else {
            ChunkIoError::Io(error)
        });
    }
    if bytes[..4] != GENERATION_FOOTER_MAGIC {
        return Err(ChunkIoError::GenerationFooterInvalid);
    }
    let version = order_u16(&bytes[4..6], order)?;
    if version
        != if profile_minor == LEGACY_GENERATION_PROFILE_MINOR_VERSION {
            1
        } else {
            2
        }
        || order_u16(&bytes[6..8], order)? as u64 != footer_len
    {
        return Err(ChunkIoError::GenerationFooterInvalid);
    }
    let generation = order_u64(&bytes[8..16], order)?;
    let prev_footer_offset = order_u64(&bytes[16..24], order)?;
    let directory_offset = order_u64(&bytes[24..32], order)?;
    let index_catalog_offset = if version == 2 {
        order_u64(&bytes[32..40], order)?
    } else {
        0
    };
    let digest_offset = if version == 2 { 40 } else { 32 };
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&bytes[digest_offset..digest_offset + 32]);
    if version == 2 && order_u64(&bytes[72..80], order)? != 0 {
        return Err(ChunkIoError::GenerationFooterInvalid);
    }
    if generation == 0 || directory_offset < metadata_end || directory_offset >= offset {
        return Err(ChunkIoError::GenerationFooterInvalid);
    }
    if version == 2 && (index_catalog_offset < directory_offset || index_catalog_offset >= offset) {
        return Err(ChunkIoError::GenerationFooterInvalid);
    }
    if prev_footer_offset == 0 {
        if generation != 1 {
            return Err(ChunkIoError::GenerationChainInvalid);
        }
    } else {
        if generation <= 1 || prev_footer_offset >= offset {
            return Err(ChunkIoError::GenerationChainInvalid);
        }
        let previous_end = prev_footer_offset
            .checked_add(footer_len)
            .ok_or(ChunkIoError::LengthOverflow)?;
        if previous_end > offset {
            return Err(ChunkIoError::GenerationChainInvalid);
        }
    }
    Ok(GenerationFooter {
        generation,
        prev_footer_offset,
        directory_offset,
        index_catalog_offset,
        digest,
    })
}

fn read_generation_preamble<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    file_len: u64,
) -> Result<(ContainerHeader, Metadata, u64), ChunkIoError> {
    use std::io::SeekFrom;
    if file_len < CONTAINER_HEADER_LEN as u64 {
        return Err(ChunkIoError::NoValidGeneration);
    }
    reader.seek(SeekFrom::Start(0))?;
    let mut raw_header = [0u8; CONTAINER_HEADER_LEN];
    reader.read_exact(&mut raw_header)?;
    let header = ContainerHeader::decode(&raw_header).map_err(ChunkIoError::Header)?;
    if !is_generation_profile_minor(header.minor) {
        return Err(ChunkIoError::UnsupportedGenerationVersion(header.minor));
    }
    if header.directory_offset != 0 {
        return Err(ChunkIoError::GenerationHeaderDirectoryOffsetNonZero);
    }
    header
        .validate_file_bounds(file_len)
        .map_err(ChunkIoError::Header)?;
    if header.metadata_len > MAX_METADATA_BYTES {
        return Err(ChunkIoError::Metadata(MetadataError::TooLarge));
    }
    let metadata_end = (CONTAINER_HEADER_LEN as u64)
        .checked_add(header.metadata_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    if metadata_end > file_len {
        return Err(ChunkIoError::NoValidGeneration);
    }
    let size = usize::try_from(header.metadata_len)
        .map_err(|_| ChunkIoError::Metadata(MetadataError::TooLarge))?;
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(size)
        .map_err(|_| ChunkIoError::Metadata(MetadataError::TooLarge))?;
    payload.resize(size, 0);
    reader.seek(SeekFrom::Start(CONTAINER_HEADER_LEN as u64))?;
    reader.read_exact(&mut payload)?;
    let metadata = Metadata::from_cbor(&payload).map_err(ChunkIoError::Metadata)?;
    if metadata.scientific_model().is_some() && header.minor < GENERATION_PROFILE_MINOR_VERSION {
        return Err(ChunkIoError::UnsupportedGenerationVersion(header.minor));
    }
    Ok((header, metadata, metadata_end))
}

fn parse_generation_directory<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    directory_offset: u64,
    generation_start: u64,
    footer_offset: u64,
    order: HeaderByteOrder,
    profile_minor: u16,
) -> Result<(Vec<ChunkDescriptor>, u64), ChunkIoError> {
    use std::io::SeekFrom;
    if directory_offset < generation_start || directory_offset >= footer_offset {
        return Err(ChunkIoError::DirectoryOffsetInvalid);
    }
    let header_end = directory_offset
        .checked_add(CHUNK_DIRECTORY_HEADER_LEN)
        .ok_or(ChunkIoError::LengthOverflow)?;
    if header_end > footer_offset {
        return Err(ChunkIoError::DirectoryBoundsInvalid);
    }
    reader.seek(SeekFrom::Start(directory_offset))?;
    let mut header = [0u8; CHUNK_DIRECTORY_HEADER_LEN as usize];
    reader
        .read_exact(&mut header)
        .map_err(|_| ChunkIoError::DirectoryBoundsInvalid)?;
    if header[..4] != CHUNK_DIRECTORY_MAGIC {
        return Err(ChunkIoError::DirectoryMagicMismatch);
    }
    let version = read_u16_order(&header[4..6], order)?;
    if version != GENERATION_DIRECTORY_VERSION {
        return Err(ChunkIoError::DirectoryVersionUnsupported(version));
    }
    if read_u16_order(&header[6..8], order)? as u64 != GENERATION_DESCRIPTOR_LEN {
        return Err(ChunkIoError::DirectoryEntrySizeInvalid);
    }
    let count = read_u32_order(&header[8..12], order)? as u64;
    if read_u32_order(&header[12..16], order)? != 0 {
        return Err(ChunkIoError::DirectoryReservedNonZero);
    }
    if count > MAX_CHUNKS {
        return Err(ChunkIoError::TooManyChunks);
    }
    let entries_bytes = count
        .checked_mul(GENERATION_DESCRIPTOR_LEN)
        .ok_or(ChunkIoError::LengthOverflow)?;
    let directory_len = CHUNK_DIRECTORY_HEADER_LEN
        .checked_add(entries_bytes)
        .ok_or(ChunkIoError::LengthOverflow)?;
    let directory_end = directory_offset
        .checked_add(directory_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    if directory_end > footer_offset
        || (profile_minor == LEGACY_GENERATION_PROFILE_MINOR_VERSION
            && directory_end != footer_offset)
    {
        return Err(ChunkIoError::DirectoryBoundsInvalid);
    }
    let capacity = usize::try_from(count).map_err(|_| ChunkIoError::LengthOverflow)?;
    let mut descriptors = Vec::new();
    descriptors
        .try_reserve_exact(capacity)
        .map_err(|_| ChunkIoError::TooManyChunks)?;
    let mut ids = std::collections::BTreeSet::new();
    let entry_len = GENERATION_DESCRIPTOR_LEN as usize;
    let batch_entries = capacity.min(1024);
    let mut batch_buf = vec![0u8; batch_entries * entry_len];
    let mut remaining = capacity;
    while remaining > 0 {
        let take = remaining.min(batch_entries);
        let slice = &mut batch_buf[..take * entry_len];
        reader
            .read_exact(slice)
            .map_err(|_| ChunkIoError::DirectoryBoundsInvalid)?;
        for entry in slice.chunks_exact(entry_len) {
            let id = read_u64_order(&entry[0..8], order)?;
            let offset = read_u64_order(&entry[8..16], order)?;
            let stored_len = read_u64_order(&entry[16..24], order)?;
            let raw_len = read_u64_order(&entry[24..32], order)?;
            let checksum = read_u32_order(&entry[32..36], order)?;
            if entry[37] != 0 || read_u16_order(&entry[38..40], order)? != 0 {
                return Err(ChunkIoError::DirectoryReservedNonZero);
            }
            let codec = ChunkCodec::try_from(entry[36])
                .map_err(|_| ChunkIoError::UnsupportedCodec(entry[36]))?;
            if !ids.insert(id) {
                return Err(ChunkIoError::DirectoryDuplicateChunkId);
            }
            if raw_len > MAX_CHUNK_BYTES || stored_len > MAX_STORED_CHUNK_BYTES {
                return Err(ChunkIoError::ChunkTooLarge);
            }
            if codec == ChunkCodec::Raw && stored_len != raw_len {
                return Err(ChunkIoError::DirectoryBoundsInvalid);
            }
            let range_end = offset
                .checked_add(stored_len)
                .ok_or(ChunkIoError::LengthOverflow)?;
            if offset < generation_start || range_end > directory_offset {
                return Err(ChunkIoError::DirectoryOffsetInvalid);
            }
            let mut digest = [0u8; 32];
            digest.copy_from_slice(&entry[40..72]);
            descriptors.push(ChunkDescriptor {
                id,
                offset,
                stored_len,
                raw_len,
                crc32c: checksum,
                codec,
                blake3: Some(digest),
            });
        }
        remaining -= take;
    }
    let mut by_offset = descriptors.clone();
    by_offset.sort_by_key(|descriptor| descriptor.offset);
    for pair in by_offset.windows(2) {
        let previous_end = pair[0]
            .offset
            .checked_add(pair[0].stored_len)
            .ok_or(ChunkIoError::LengthOverflow)?;
        if previous_end > pair[1].offset {
            return Err(ChunkIoError::DirectoryOverlap);
        }
    }
    Ok((descriptors, directory_end))
}

fn hash_and_crc_file_range<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    offset: u64,
    length: u64,
) -> Result<([u8; 32], u32), ChunkIoError> {
    use std::io::SeekFrom;
    let end = offset
        .checked_add(length)
        .ok_or(ChunkIoError::LengthOverflow)?;
    let file_len = reader.seek(SeekFrom::End(0))?;
    if end > file_len {
        return Err(ChunkIoError::DirectoryOffsetInvalid);
    }
    reader.seek(SeekFrom::Start(offset))?;
    let mut remaining = length;
    let mut buffer = vec![0u8; 256 * 1024];
    let mut hasher = blake3::Hasher::new();
    let mut crc = !0u32;
    while remaining > 0 {
        let count = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| ChunkIoError::LengthOverflow)?;
        reader.read_exact(&mut buffer[..count])?;
        let bytes = &buffer[..count];
        hasher.update(bytes);
        for &byte in bytes {
            let index = ((crc as u8) ^ byte) as usize;
            crc = (crc >> 8) ^ CRC32C_TABLE[index];
        }
        remaining -= count as u64;
    }
    Ok((*hasher.finalize().as_bytes(), !crc))
}

fn verify_generation_chunk<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    descriptor: ChunkDescriptor,
) -> Result<(), ChunkIoError> {
    let (digest, checksum) =
        hash_and_crc_file_range(reader, descriptor.offset, descriptor.stored_len)?;
    if checksum != descriptor.crc32c {
        return Err(ChunkIoError::ChecksumMismatch(descriptor.id));
    }
    if descriptor.blake3.is_some_and(|expected| expected != digest) {
        return Err(ChunkIoError::ChunkHashMismatch(descriptor.id));
    }
    Ok(())
}

fn hash_file_range<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    offset: u64,
    length: u64,
) -> Result<[u8; 32], ChunkIoError> {
    use std::io::SeekFrom;
    let end = offset
        .checked_add(length)
        .ok_or(ChunkIoError::LengthOverflow)?;
    let file_len = reader.seek(SeekFrom::End(0))?;
    if end > file_len {
        return Err(ChunkIoError::DirectoryBoundsInvalid);
    }
    reader.seek(SeekFrom::Start(offset))?;
    let mut remaining = length;
    let mut buffer = vec![0u8; 256 * 1024];
    let mut hasher = blake3::Hasher::new();
    while remaining > 0 {
        let count = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| ChunkIoError::LengthOverflow)?;
        reader.read_exact(&mut buffer[..count])?;
        hasher.update(&buffer[..count]);
        remaining -= count as u64;
    }
    Ok(*hasher.finalize().as_bytes())
}

#[derive(Debug, Clone, Copy)]
struct IndexPageRegion {
    start: u64,
    end: u64,
    generation: u64,
}

#[derive(Debug, Clone, Copy)]
struct IndexCatalogEntry {
    kind_id: u16,
    flags: u16,
    root_offset: u64,
    item_count: u64,
    level: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct IndexCompositeKey {
    key: [u8; 32],
    chunk_id: u64,
}

#[derive(Debug, Clone)]
enum DecodedIndexPage {
    Leaf {
        records: Vec<IndexCompositeKey>,
    },
    Internal {
        level: u16,
        separators: Vec<IndexCompositeKey>,
        children: Vec<u64>,
    },
    Catalog {
        generation: u64,
        entries: Vec<IndexCatalogEntry>,
    },
    Manifest {
        generation: u64,
        payload: Vec<u8>,
    },
}

fn index_page_region_for_offset(
    offset: u64,
    regions: &[IndexPageRegion],
) -> Result<IndexPageRegion, ChunkIoError> {
    let page_end = offset
        .checked_add(INDEX_PAGE_SIZE as u64)
        .ok_or(ChunkIoError::LengthOverflow)?;
    regions
        .iter()
        .copied()
        .find(|region| {
            offset >= region.start
                && page_end <= region.end
                && region.end >= region.start
                && (offset - region.start) % INDEX_PAGE_SIZE as u64 == 0
        })
        .ok_or(ChunkIoError::IndexPageBoundsInvalid)
}

fn read_index_page<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    offset: u64,
    regions: &[IndexPageRegion],
    order: HeaderByteOrder,
    max_generation: u64,
) -> Result<DecodedIndexPage, ChunkIoError> {
    use std::io::SeekFrom;
    let region = index_page_region_for_offset(offset, regions)?;
    let mut bytes = [0u8; INDEX_PAGE_SIZE];
    reader.seek(SeekFrom::Start(offset))?;
    reader
        .read_exact(&mut bytes)
        .map_err(|_| ChunkIoError::IndexPageBoundsInvalid)?;
    if bytes[..4] != INDEX_PAGE_MAGIC {
        return Err(ChunkIoError::IndexPageMagicMismatch);
    }
    let version = read_u16_order(&bytes[4..6], order)?;
    if version != INDEX_PAGE_VERSION {
        return Err(ChunkIoError::IndexPageVersionUnsupported(version));
    }
    if bytes[7] != 0
        || read_u32_order(&bytes[8..12], order)? as usize != INDEX_PAGE_SIZE
        || bytes[56..64].iter().any(|byte| *byte != 0)
    {
        return Err(ChunkIoError::IndexPageReservedNonZero);
    }
    let kind = bytes[6];
    let level = read_u16_order(&bytes[12..14], order)?;
    let count = read_u16_order(&bytes[14..16], order)? as usize;
    let generation = read_u64_order(&bytes[16..24], order)?;
    if generation != region.generation || generation > max_generation || generation == 0 {
        return Err(ChunkIoError::IndexPageBoundsInvalid);
    }
    let mut expected = [0u8; 32];
    expected.copy_from_slice(&bytes[INDEX_PAGE_DIGEST_RANGE.clone()]);
    bytes[INDEX_PAGE_DIGEST_RANGE.clone()].fill(0);
    let actual = *blake3::hash(&bytes).as_bytes();
    if actual != expected {
        return Err(ChunkIoError::IndexPageHashMismatch(offset));
    }

    match kind {
        INDEX_PAGE_LEAF => {
            if level != 0 || count == 0 || count > INDEX_LEAF_CAPACITY {
                return Err(ChunkIoError::IndexTreeInvalid);
            }
            let used = INDEX_PAGE_HEADER_LEN + count * INDEX_LEAF_ENTRY_LEN;
            if bytes[used..].iter().any(|byte| *byte != 0) {
                return Err(ChunkIoError::IndexPageReservedNonZero);
            }
            let mut records = Vec::new();
            records
                .try_reserve_exact(count)
                .map_err(|_| ChunkIoError::IndexTreePageLimit)?;
            for index in 0..count {
                let start = INDEX_PAGE_HEADER_LEN + index * INDEX_LEAF_ENTRY_LEN;
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes[start..start + 32]);
                let chunk_id = read_u64_order(&bytes[start + 32..start + 40], order)?;
                records.push(IndexCompositeKey { key, chunk_id });
            }
            if records.windows(2).any(|pair| pair[0] >= pair[1]) {
                return Err(ChunkIoError::IndexTreeInvalid);
            }
            Ok(DecodedIndexPage::Leaf { records })
        }
        INDEX_PAGE_INTERNAL => {
            if level == 0 || count == 0 || count > INDEX_INTERNAL_CAPACITY {
                return Err(ChunkIoError::IndexTreeInvalid);
            }
            let used = INDEX_PAGE_HEADER_LEN + 8 + count * INDEX_INTERNAL_ENTRY_LEN;
            if bytes[used..].iter().any(|byte| *byte != 0) {
                return Err(ChunkIoError::IndexPageReservedNonZero);
            }
            let mut children = Vec::new();
            let mut separators = Vec::new();
            children
                .try_reserve_exact(count + 1)
                .map_err(|_| ChunkIoError::IndexTreePageLimit)?;
            separators
                .try_reserve_exact(count)
                .map_err(|_| ChunkIoError::IndexTreePageLimit)?;
            children.push(read_u64_order(
                &bytes[INDEX_PAGE_HEADER_LEN..INDEX_PAGE_HEADER_LEN + 8],
                order,
            )?);
            let mut cursor = INDEX_PAGE_HEADER_LEN + 8;
            for _ in 0..count {
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes[cursor..cursor + 32]);
                let chunk_id = read_u64_order(&bytes[cursor + 32..cursor + 40], order)?;
                separators.push(IndexCompositeKey { key, chunk_id });
                children.push(read_u64_order(&bytes[cursor + 40..cursor + 48], order)?);
                cursor += INDEX_INTERNAL_ENTRY_LEN;
            }
            if children.contains(&0) || separators.windows(2).any(|pair| pair[0] >= pair[1]) {
                return Err(ChunkIoError::IndexTreeInvalid);
            }
            Ok(DecodedIndexPage::Internal {
                level,
                separators,
                children,
            })
        }
        INDEX_PAGE_CATALOG => {
            if level != 0 || count > MAX_INDEX_CATALOG_ENTRIES {
                return Err(ChunkIoError::IndexCatalogInvalid);
            }
            let used = INDEX_PAGE_HEADER_LEN + count * INDEX_CATALOG_ENTRY_LEN;
            if bytes[used..].iter().any(|byte| *byte != 0) {
                return Err(ChunkIoError::IndexPageReservedNonZero);
            }
            let mut entries = Vec::new();
            entries
                .try_reserve_exact(count)
                .map_err(|_| ChunkIoError::IndexTreePageLimit)?;
            let mut previous_kind = 0u16;
            for index in 0..count {
                let start = INDEX_PAGE_HEADER_LEN + index * INDEX_CATALOG_ENTRY_LEN;
                let kind_id = read_u16_order(&bytes[start..start + 2], order)?;
                let flags = read_u16_order(&bytes[start + 2..start + 4], order)?;
                let reserved = read_u32_order(&bytes[start + 4..start + 8], order)?;
                let root_offset = read_u64_order(&bytes[start + 8..start + 16], order)?;
                let item_count = read_u64_order(&bytes[start + 16..start + 24], order)?;
                if kind_id == 0
                    || kind_id <= previous_kind
                    || flags & !1 != 0
                    || reserved != 0
                    || (item_count == 0) != (root_offset == 0)
                    || item_count > MAX_INDEX_TREE_ENTRIES
                {
                    return Err(ChunkIoError::IndexCatalogInvalid);
                }
                if LogicalIndexKind::from_id(kind_id).is_none() && flags & 1 != 0 {
                    return Err(ChunkIoError::UnsupportedRequiredIndex(kind_id));
                }
                previous_kind = kind_id;
                entries.push(IndexCatalogEntry {
                    kind_id,
                    flags,
                    root_offset,
                    item_count,
                    level: 0,
                });
            }
            Ok(DecodedIndexPage::Catalog {
                generation,
                entries,
            })
        }
        INDEX_PAGE_MANIFEST => {
            if level != 0 || count > INDEX_PAGE_SIZE - INDEX_PAGE_HEADER_LEN {
                return Err(ChunkIoError::IndexTreeInvalid);
            }
            let used = INDEX_PAGE_HEADER_LEN + count;
            Ok(DecodedIndexPage::Manifest {
                generation,
                payload: bytes[INDEX_PAGE_HEADER_LEN..used].to_vec(),
            })
        }
        other => Err(ChunkIoError::IndexPageKindUnsupported(other)),
    }
}

fn encode_index_page_header(
    bytes: &mut [u8; INDEX_PAGE_SIZE],
    kind: u8,
    level: u16,
    count: usize,
    generation: u64,
    order: HeaderByteOrder,
) -> Result<(), ChunkIoError> {
    if count > u16::MAX as usize {
        return Err(ChunkIoError::IndexTreePageLimit);
    }
    bytes[..4].copy_from_slice(&INDEX_PAGE_MAGIC);
    bytes[4..6].copy_from_slice(&match order {
        HeaderByteOrder::LittleEndian => INDEX_PAGE_VERSION.to_le_bytes(),
        HeaderByteOrder::BigEndian => INDEX_PAGE_VERSION.to_be_bytes(),
    });
    bytes[6] = kind;
    bytes[7] = 0;
    bytes[8..12].copy_from_slice(&match order {
        HeaderByteOrder::LittleEndian => (INDEX_PAGE_SIZE as u32).to_le_bytes(),
        HeaderByteOrder::BigEndian => (INDEX_PAGE_SIZE as u32).to_be_bytes(),
    });
    bytes[12..14].copy_from_slice(&match order {
        HeaderByteOrder::LittleEndian => level.to_le_bytes(),
        HeaderByteOrder::BigEndian => level.to_be_bytes(),
    });
    let count = count as u16;
    bytes[14..16].copy_from_slice(&match order {
        HeaderByteOrder::LittleEndian => count.to_le_bytes(),
        HeaderByteOrder::BigEndian => count.to_be_bytes(),
    });
    bytes[16..24].copy_from_slice(&match order {
        HeaderByteOrder::LittleEndian => generation.to_le_bytes(),
        HeaderByteOrder::BigEndian => generation.to_be_bytes(),
    });
    Ok(())
}

fn finalize_index_page(bytes: &mut [u8; INDEX_PAGE_SIZE]) {
    bytes[INDEX_PAGE_DIGEST_RANGE.clone()].fill(0);
    let digest = *blake3::hash(bytes).as_bytes();
    bytes[INDEX_PAGE_DIGEST_RANGE.clone()].copy_from_slice(&digest);
}

#[derive(Debug, Clone, Copy)]
struct IndexTreeSpan {
    first: IndexCompositeKey,
    last: IndexCompositeKey,
    count: u64,
    level: u16,
}

struct IndexSubtreeValidation<'a> {
    regions: &'a [IndexPageRegion],
    order: HeaderByteOrder,
    max_generation: u64,
    primary_ids: &'a std::collections::BTreeSet<u64>,
    kind: LogicalIndexKind,
    visited: std::collections::BTreeSet<u64>,
    page_count: u64,
    seen_chunk_ids: std::collections::BTreeSet<u64>,
}

fn validate_index_subtree<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    offset: u64,
    validation: &mut IndexSubtreeValidation<'_>,
    depth: u16,
) -> Result<IndexTreeSpan, ChunkIoError> {
    if depth > MAX_INDEX_TREE_DEPTH {
        return Err(ChunkIoError::IndexTreeDepthExceeded);
    }
    validation.page_count = validation
        .page_count
        .checked_add(1)
        .ok_or(ChunkIoError::IndexTreePageLimit)?;
    if validation.page_count > MAX_INDEX_TREE_PAGES {
        return Err(ChunkIoError::IndexTreePageLimit);
    }
    if !validation.visited.insert(offset) {
        return Err(ChunkIoError::IndexTreeInvalid);
    }
    let page = read_index_page(
        reader,
        offset,
        validation.regions,
        validation.order,
        validation.max_generation,
    )?;
    match page {
        DecodedIndexPage::Leaf { records } => {
            for record in &records {
                if !validation.primary_ids.contains(&record.chunk_id) {
                    return Err(ChunkIoError::IndexChunkNotFound(record.chunk_id));
                }
                if validation.kind == LogicalIndexKind::ChunkId
                    && (record.key != PersistentIndexEntry::key_u64(record.chunk_id)
                        || !validation.seen_chunk_ids.insert(record.chunk_id))
                {
                    return Err(ChunkIoError::IndexTreeInvalid);
                }
            }
            let first = records
                .first()
                .copied()
                .ok_or(ChunkIoError::IndexTreeInvalid)?;
            let last = records
                .last()
                .copied()
                .ok_or(ChunkIoError::IndexTreeInvalid)?;
            Ok(IndexTreeSpan {
                first,
                last,
                count: records.len() as u64,
                level: 0,
            })
        }
        DecodedIndexPage::Internal {
            level,
            separators,
            children,
        } => {
            if depth == MAX_INDEX_TREE_DEPTH {
                return Err(ChunkIoError::IndexTreeDepthExceeded);
            }
            let mut spans = Vec::new();
            spans
                .try_reserve_exact(children.len())
                .map_err(|_| ChunkIoError::IndexTreePageLimit)?;
            for child in children {
                if child >= offset {
                    return Err(ChunkIoError::IndexTreeInvalid);
                }
                let child_span = validate_index_subtree(reader, child, validation, depth + 1)?;
                if child_span.level.checked_add(1) != Some(level) {
                    return Err(ChunkIoError::IndexTreeInvalid);
                }
                spans.push(child_span);
            }
            if spans.len() != separators.len() + 1 {
                return Err(ChunkIoError::IndexTreeInvalid);
            }
            for (index, separator) in separators.iter().enumerate() {
                if spans[index].last >= *separator || spans[index + 1].first != *separator {
                    return Err(ChunkIoError::IndexTreeInvalid);
                }
            }
            let count = spans
                .iter()
                .try_fold(0u64, |sum, span| sum.checked_add(span.count))
                .ok_or(ChunkIoError::IndexTreePageLimit)?;
            Ok(IndexTreeSpan {
                first: spans.first().ok_or(ChunkIoError::IndexTreeInvalid)?.first,
                last: spans.last().ok_or(ChunkIoError::IndexTreeInvalid)?.last,
                count,
                level,
            })
        }
        DecodedIndexPage::Catalog { .. } | DecodedIndexPage::Manifest { .. } => {
            Err(ChunkIoError::IndexTreeInvalid)
        }
    }
}

fn read_index_catalog_entries<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    catalog_offset: u64,
    catalog_region: IndexPageRegion,
    regions: &[IndexPageRegion],
    order: HeaderByteOrder,
    generation: u64,
) -> Result<Vec<IndexCatalogEntry>, ChunkIoError> {
    if catalog_offset.checked_add(INDEX_PAGE_SIZE as u64) != Some(catalog_region.end)
        || catalog_region.end - catalog_region.start < INDEX_PAGE_SIZE as u64
        || (catalog_region.end - catalog_region.start) % INDEX_PAGE_SIZE as u64 != 0
    {
        return Err(ChunkIoError::IndexCatalogInvalid);
    }
    let entries = match read_index_page(reader, catalog_offset, regions, order, generation)? {
        DecodedIndexPage::Catalog {
            generation: page_generation,
            entries,
        } if page_generation == generation => entries,
        _ => return Err(ChunkIoError::IndexCatalogInvalid),
    };
    for entry in &entries {
        if entry.root_offset != 0 {
            let _ = index_page_region_for_offset(entry.root_offset, regions)?;
        }
    }
    Ok(entries)
}

fn validate_index_catalog<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    catalog_offset: u64,
    catalog_region: IndexPageRegion,
    regions: &[IndexPageRegion],
    order: HeaderByteOrder,
    generation: u64,
    descriptors: &[ChunkDescriptor],
) -> Result<Vec<IndexCatalogEntry>, ChunkIoError> {
    let mut entries = read_index_catalog_entries(
        reader,
        catalog_offset,
        catalog_region,
        regions,
        order,
        generation,
    )?;
    let mut primary_ids = std::collections::BTreeSet::new();
    for descriptor in descriptors {
        if !primary_ids.insert(descriptor.id) {
            return Err(ChunkIoError::DirectoryDuplicateChunkId);
        }
    }
    let mut chunk_index_seen = std::collections::BTreeSet::new();
    let mut chunk_index_count = 0u64;
    for entry in &mut entries {
        let Some(kind) = LogicalIndexKind::from_id(entry.kind_id) else {
            if entry.root_offset != 0 {
                let _ = index_page_region_for_offset(entry.root_offset, regions)?;
            }
            continue;
        };
        if kind == LogicalIndexKind::ScientificManifest {
            if entry.root_offset != 0 {
                let _ = index_page_region_for_offset(entry.root_offset, regions)?;
                let manifest_page =
                    read_index_page(reader, entry.root_offset, regions, order, generation)?;
                if let DecodedIndexPage::Manifest {
                    generation: page_gen,
                    payload,
                } = manifest_page
                {
                    if page_gen != generation {
                        return Err(ChunkIoError::IndexTreeInvalid);
                    }
                    let _: Metadata =
                        Metadata::from_cbor(&payload).map_err(ChunkIoError::Metadata)?;
                } else {
                    return Err(ChunkIoError::IndexTreeInvalid);
                }
            }
            continue;
        }
        if entry.kind_id == LogicalIndexKind::ChunkId as u16 && entry.flags & 1 == 0 {
            return Err(ChunkIoError::IndexCatalogInvalid);
        }
        if entry.item_count == 0 {
            entry.level = 0;
            if kind == LogicalIndexKind::ChunkId && !primary_ids.is_empty() {
                return Err(ChunkIoError::IndexTreeInvalid);
            }
            continue;
        }
        let seen_chunk_ids = if kind == LogicalIndexKind::ChunkId {
            std::mem::take(&mut chunk_index_seen)
        } else {
            std::collections::BTreeSet::new()
        };
        let mut validation = IndexSubtreeValidation {
            regions,
            order,
            max_generation: generation,
            primary_ids: &primary_ids,
            kind,
            visited: std::collections::BTreeSet::new(),
            page_count: 0,
            seen_chunk_ids,
        };
        let span = validate_index_subtree(reader, entry.root_offset, &mut validation, 1)?;
        if span.count != entry.item_count || span.count > MAX_INDEX_TREE_ENTRIES {
            return Err(ChunkIoError::IndexTreeInvalid);
        }
        entry.level = span.level;
        if kind == LogicalIndexKind::ChunkId {
            chunk_index_count = span.count;
            chunk_index_seen = validation.seen_chunk_ids;
        }
    }
    if !entries
        .iter()
        .any(|entry| entry.kind_id == LogicalIndexKind::ChunkId as u16)
        || chunk_index_count != descriptors.len() as u64
        || chunk_index_seen.len() != primary_ids.len()
        || chunk_index_seen != primary_ids
    {
        return Err(ChunkIoError::IndexTreeInvalid);
    }
    Ok(entries)
}

struct IndexRangeTraversal<'a> {
    regions: &'a [IndexPageRegion],
    order: HeaderByteOrder,
    generation: u64,
    lower: IndexCompositeKey,
    upper: IndexCompositeKey,
    output: Vec<u64>,
    visited: std::collections::BTreeSet<u64>,
    page_count: u64,
}

fn collect_index_range<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    offset: u64,
    traversal: &mut IndexRangeTraversal<'_>,
    expected_level: u16,
) -> Result<(), ChunkIoError> {
    if expected_level > MAX_INDEX_TREE_DEPTH {
        return Err(ChunkIoError::IndexTreeDepthExceeded);
    }
    traversal.page_count = traversal
        .page_count
        .checked_add(1)
        .ok_or(ChunkIoError::IndexTreePageLimit)?;
    if traversal.page_count > MAX_INDEX_TREE_PAGES {
        return Err(ChunkIoError::IndexTreePageLimit);
    }
    if !traversal.visited.insert(offset) {
        return Err(ChunkIoError::IndexTreeInvalid);
    }
    match read_index_page(
        reader,
        offset,
        traversal.regions,
        traversal.order,
        traversal.generation,
    )? {
        DecodedIndexPage::Leaf { records } => {
            if expected_level != 0 {
                return Err(ChunkIoError::IndexTreeInvalid);
            }
            for record in records {
                if record >= traversal.lower && record <= traversal.upper {
                    if traversal.output.len() as u64 >= MAX_INDEX_TREE_ENTRIES {
                        return Err(ChunkIoError::IndexTreePageLimit);
                    }
                    traversal
                        .output
                        .try_reserve(1)
                        .map_err(|_| ChunkIoError::IndexTreePageLimit)?;
                    traversal.output.push(record.chunk_id);
                }
            }
            Ok(())
        }
        DecodedIndexPage::Internal {
            level,
            separators,
            children,
        } => {
            if level == 0 || level != expected_level {
                return Err(ChunkIoError::IndexTreeInvalid);
            }
            let child_level = level.checked_sub(1).ok_or(ChunkIoError::IndexTreeInvalid)?;
            for (child_index, child_offset) in children.iter().copied().enumerate() {
                let child_lower = child_index
                    .checked_sub(1)
                    .and_then(|index| separators.get(index))
                    .copied();
                let child_upper = separators.get(child_index).copied();
                if child_upper.is_some_and(|bound| bound <= traversal.lower)
                    || child_lower.is_some_and(|bound| bound > traversal.upper)
                {
                    continue;
                }
                collect_index_range(reader, child_offset, traversal, child_level)?;
            }
            Ok(())
        }
        DecodedIndexPage::Catalog { .. } | DecodedIndexPage::Manifest { .. } => {
            Err(ChunkIoError::IndexTreeInvalid)
        }
    }
}

fn lookup_persistent_index_range<R: std::io::Read + std::io::Seek>(
    snapshot: &GenerationSnapshot,
    reader: &mut R,
    kind: LogicalIndexKind,
    lower: &[u8; 32],
    upper: &[u8; 32],
) -> Result<Vec<u64>, ChunkIoError> {
    if lower > upper {
        return Ok(Vec::new());
    }
    let root = snapshot
        .index_catalog
        .iter()
        .find(|entry| entry.kind_id == kind as u16)
        .ok_or(ChunkIoError::IndexNotPresent(kind as u16))?;
    if root.item_count == 0 {
        return Ok(Vec::new());
    }
    let lower_key = IndexCompositeKey {
        key: *lower,
        chunk_id: 0,
    };
    let upper_key = IndexCompositeKey {
        key: *upper,
        chunk_id: u64::MAX,
    };
    let mut traversal = IndexRangeTraversal {
        regions: &snapshot.index_page_regions,
        order: snapshot.byte_order,
        generation: snapshot.generation,
        lower: lower_key,
        upper: upper_key,
        output: Vec::new(),
        visited: std::collections::BTreeSet::new(),
        page_count: 0,
    };
    collect_index_range(reader, root.root_offset, &mut traversal, root.level)?;
    for chunk_id in &traversal.output {
        if snapshot.chunk(*chunk_id).is_none() {
            return Err(ChunkIoError::IndexChunkNotFound(*chunk_id));
        }
    }
    Ok(traversal.output)
}

fn validate_generation_candidate<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    candidate_offset: u64,
    file_len: u64,
    header: ContainerHeader,
    metadata: &Metadata,
    metadata_end: u64,
    eager_validate: bool,
) -> Result<GenerationSnapshot, ChunkIoError> {
    let footer_len = generation_footer_len(header.minor)?;
    let mut newest_first = Vec::<(u64, GenerationFooter)>::new();
    let mut current_offset = candidate_offset;
    let mut expected_generation: Option<u64> = None;
    loop {
        if newest_first.len() as u64 >= MAX_GENERATION_CHAIN {
            return Err(ChunkIoError::RecoveryScanLimit);
        }
        if newest_first
            .iter()
            .any(|(seen_offset, _)| *seen_offset == current_offset)
        {
            return Err(ChunkIoError::GenerationChainInvalid);
        }
        let footer = read_generation_footer_at(
            reader,
            current_offset,
            file_len,
            metadata_end,
            header.byte_order,
            header.minor,
        )?;
        if let Some(expected) = expected_generation {
            if footer.generation != expected {
                return Err(ChunkIoError::GenerationChainInvalid);
            }
        }
        newest_first.push((current_offset, footer));
        if footer.prev_footer_offset == 0 {
            if footer.generation != 1 {
                return Err(ChunkIoError::GenerationChainInvalid);
            }
            break;
        }
        expected_generation = Some(
            footer
                .generation
                .checked_sub(1)
                .ok_or(ChunkIoError::GenerationChainInvalid)?,
        );
        current_offset = footer.prev_footer_offset;
    }

    let mut all_descriptors = Vec::new();
    let mut global_ids = std::collections::BTreeSet::new();
    let mut previous_footer = 0u64;
    let mut current_generation_directory = 0u64;
    let mut latest_catalog = Vec::<IndexCatalogEntry>::new();
    let mut page_regions = Vec::<IndexPageRegion>::new();

    for (footer_offset, footer) in newest_first.iter().rev() {
        if footer.prev_footer_offset != previous_footer {
            return Err(ChunkIoError::GenerationChainInvalid);
        }
        let generation_start = if previous_footer == 0 {
            metadata_end
        } else {
            previous_footer
                .checked_add(footer_len)
                .ok_or(ChunkIoError::LengthOverflow)?
        };
        if generation_start > *footer_offset || footer.directory_offset < generation_start {
            return Err(ChunkIoError::GenerationChainInvalid);
        }
        let (descriptors, directory_end) = parse_generation_directory(
            reader,
            footer.directory_offset,
            generation_start,
            *footer_offset,
            header.byte_order,
            header.minor,
        )?;
        let new_total = all_descriptors
            .len()
            .checked_add(descriptors.len())
            .ok_or(ChunkIoError::TooManyChunks)?;
        if new_total as u64 > MAX_CHUNKS {
            return Err(ChunkIoError::TooManyChunks);
        }
        for descriptor in &descriptors {
            if !global_ids.insert(descriptor.id) {
                return Err(ChunkIoError::DirectoryDuplicateChunkId);
            }
            if eager_validate {
                verify_generation_chunk(reader, *descriptor)?;
            }
        }
        all_descriptors
            .try_reserve(descriptors.len())
            .map_err(|_| ChunkIoError::TooManyChunks)?;
        all_descriptors.extend(descriptors);

        if is_index_generation_minor(header.minor) {
            let page_region = IndexPageRegion {
                start: directory_end,
                end: *footer_offset,
                generation: footer.generation,
            };
            if footer
                .index_catalog_offset
                .checked_add(INDEX_PAGE_SIZE as u64)
                != Some(*footer_offset)
                || page_region.end < page_region.start
                || (page_region.end - page_region.start) < INDEX_PAGE_SIZE as u64
                || (page_region.end - page_region.start) % INDEX_PAGE_SIZE as u64 != 0
                || footer.index_catalog_offset < page_region.start
                || (footer.index_catalog_offset - page_region.start) % INDEX_PAGE_SIZE as u64 != 0
            {
                return Err(ChunkIoError::IndexCatalogInvalid);
            }
            page_regions.push(page_region);
            if eager_validate {
                latest_catalog = validate_index_catalog(
                    reader,
                    footer.index_catalog_offset,
                    page_region,
                    &page_regions,
                    header.byte_order,
                    footer.generation,
                    &all_descriptors,
                )?;
            } else {
                latest_catalog = read_index_catalog_entries(
                    reader,
                    footer.index_catalog_offset,
                    page_region,
                    &page_regions,
                    header.byte_order,
                    footer.generation,
                )?;
            }
        } else if directory_end != *footer_offset {
            return Err(ChunkIoError::DirectoryBoundsInvalid);
        }

        // Page-level integrity/structure errors are reported before the enclosing generation
        // digest mismatch, while the digest remains a second independent integrity check.
        if eager_validate {
            let generation_len = footer_offset
                .checked_sub(generation_start)
                .ok_or(ChunkIoError::GenerationChainInvalid)?;
            let actual_digest = hash_file_range(reader, generation_start, generation_len)?;
            if actual_digest != footer.digest {
                return Err(ChunkIoError::GenerationHashMismatch);
            }
        }
        previous_footer = *footer_offset;
        current_generation_directory = footer.directory_offset;
    }

    let (footer_offset, latest_footer) = newest_first
        .first()
        .copied()
        .ok_or(ChunkIoError::NoValidGeneration)?;
    let valid_end = footer_offset
        .checked_add(footer_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    let mut effective_metadata = metadata.clone();
    if is_index_generation_minor(header.minor) {
        if let Some(manifest_entry) = latest_catalog
            .iter()
            .find(|entry| entry.kind_id == LogicalIndexKind::ScientificManifest as u16)
        {
            let manifest_page = read_index_page(
                reader,
                manifest_entry.root_offset,
                &page_regions,
                header.byte_order,
                latest_footer.generation,
            )?;
            if let DecodedIndexPage::Manifest {
                generation,
                payload,
            } = manifest_page
            {
                if generation != latest_footer.generation {
                    return Err(ChunkIoError::IndexTreeInvalid);
                }
                let decoded_manifest =
                    Metadata::from_cbor(&payload).map_err(ChunkIoError::Metadata)?;
                effective_metadata = decoded_manifest;
            } else {
                return Err(ChunkIoError::IndexTreeInvalid);
            }
        }
    }

    let snapshot = GenerationSnapshot {
        metadata: effective_metadata,
        descriptors: all_descriptors,
        generation: latest_footer.generation,
        footer_offset,
        directory_offset: current_generation_directory,
        valid_end,
        byte_order: header.byte_order,
        profile_minor: header.minor,
        index_catalog: latest_catalog,
        index_page_regions: page_regions,
    };
    if let Some(model) = snapshot.metadata.scientific_model() {
        if !is_index_generation_minor(header.minor) && snapshot.generation != 1 {
            return Err(ChunkIoError::ScientificModelMultipleGenerations);
        }
        model
            .validate_descriptors(&snapshot.descriptors)
            .map_err(ChunkIoError::ScientificModel)?;
    }
    Ok(snapshot)
}

impl GenerationSnapshot {
    /// Performs exhaustive cryptographic and CRC32C verification across all stored chunks,
    /// B+tree index pages, and generation byte ranges.
    pub fn validate_full<R: std::io::Read + std::io::Seek>(
        &self,
        reader: &mut R,
    ) -> Result<(), ChunkIoError> {
        use std::io::SeekFrom;
        // 1. Verify CRC32C and preamble bounds for all stored chunks
        for descriptor in &self.descriptors {
            verify_generation_chunk(reader, *descriptor)?;
        }

        // 2. Read footer chain and verify index subtrees + generation digests
        let file_len = reader.seek(SeekFrom::End(0))?;
        let (header, _, metadata_end) = read_generation_preamble(reader, file_len)?;
        let footer_len = generation_footer_len(header.minor)?;
        let mut newest_first = Vec::<(u64, GenerationFooter)>::new();
        let mut current_offset = self.footer_offset;
        loop {
            let footer = read_generation_footer_at(
                reader,
                current_offset,
                file_len,
                metadata_end,
                header.byte_order,
                header.minor,
            )?;
            newest_first.push((current_offset, footer));
            if footer.prev_footer_offset == 0 {
                break;
            }
            current_offset = footer.prev_footer_offset;
        }

        let mut previous_footer = 0u64;
        let mut page_regions = Vec::<IndexPageRegion>::new();
        let mut accumulated_descriptors = Vec::<ChunkDescriptor>::new();
        for (footer_offset, footer) in newest_first.iter().rev() {
            let generation_start = if previous_footer == 0 {
                metadata_end
            } else {
                previous_footer
                    .checked_add(footer_len)
                    .ok_or(ChunkIoError::LengthOverflow)?
            };
            let (descriptors, directory_end) = parse_generation_directory(
                reader,
                footer.directory_offset,
                generation_start,
                *footer_offset,
                header.byte_order,
                header.minor,
            )?;
            accumulated_descriptors.extend(descriptors);

            if is_index_generation_minor(header.minor) {
                let page_region = IndexPageRegion {
                    start: directory_end,
                    end: *footer_offset,
                    generation: footer.generation,
                };
                page_regions.push(page_region);
                validate_index_catalog(
                    reader,
                    footer.index_catalog_offset,
                    page_region,
                    &page_regions,
                    header.byte_order,
                    footer.generation,
                    &accumulated_descriptors,
                )?;
            }

            let generation_len = footer_offset
                .checked_sub(generation_start)
                .ok_or(ChunkIoError::GenerationChainInvalid)?;
            let actual_digest = hash_file_range(reader, generation_start, generation_len)?;
            if actual_digest != footer.digest {
                return Err(ChunkIoError::GenerationHashMismatch);
            }
            previous_footer = *footer_offset;
        }

        if let Some(model) = self.metadata.scientific_model() {
            model
                .validate_descriptors(&self.descriptors)
                .map_err(ChunkIoError::ScientificModel)?;
        }
        Ok(())
    }
}

/// Opens the newest fully validated generation without changing the file.
/// The scan and full-chain verification use bounded buffers; open validates all stored chunks.
pub fn open_generation_snapshot<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
) -> Result<GenerationSnapshot, ChunkIoError> {
    open_generation_snapshot_mode(reader, true)
}

/// Opens the newest generation using lazy validation (O(1) relative to total chunk payloads).
/// Validates container header, footer chain, and directory bounds without eagerly reading chunk
/// payloads or computing full-generation BLAKE3 hashes. Per-chunk CRC32C is checked on read.
pub fn open_generation_snapshot_lazy<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
) -> Result<GenerationSnapshot, ChunkIoError> {
    open_generation_snapshot_mode(reader, false)
}

fn open_generation_snapshot_mode<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    eager_validate: bool,
) -> Result<GenerationSnapshot, ChunkIoError> {
    use std::io::SeekFrom;
    let file_len = reader.seek(SeekFrom::End(0))?;
    let (header, metadata, metadata_end) = read_generation_preamble(reader, file_len)?;
    let footer_len = generation_footer_len(header.minor)?;
    if file_len
        < metadata_end
            .checked_add(footer_len)
            .ok_or(ChunkIoError::LengthOverflow)?
    {
        return Err(ChunkIoError::NoValidGeneration);
    }
    let mut first_error: Option<ChunkIoError> = None;
    let mut attempts = 0u64;
    let mut block_end = file_len;
    while block_end > 0 {
        let block_start = block_end.saturating_sub(RECOVERY_SCAN_BLOCK_BYTES as u64);
        let read_end = block_end.saturating_add(3).min(file_len);
        let read_len = usize::try_from(
            read_end
                .checked_sub(block_start)
                .ok_or(ChunkIoError::LengthOverflow)?,
        )
        .map_err(|_| ChunkIoError::LengthOverflow)?;
        let mut block = Vec::new();
        block
            .try_reserve_exact(read_len)
            .map_err(|_| ChunkIoError::RecoveryScanLimit)?;
        block.resize(read_len, 0);
        reader.seek(SeekFrom::Start(block_start))?;
        reader.read_exact(&mut block)?;
        let valid_starts =
            usize::try_from(block_end - block_start).map_err(|_| ChunkIoError::LengthOverflow)?;
        if block.len() >= 4 {
            for index in (0..valid_starts.min(block.len().saturating_sub(3))).rev() {
                if block[index..index + 4] != GENERATION_FOOTER_MAGIC {
                    continue;
                }
                let candidate = block_start
                    .checked_add(index as u64)
                    .ok_or(ChunkIoError::LengthOverflow)?;
                let candidate_end = candidate
                    .checked_add(footer_len)
                    .ok_or(ChunkIoError::LengthOverflow)?;
                if candidate < metadata_end || candidate_end > file_len {
                    continue;
                }
                attempts = attempts
                    .checked_add(1)
                    .ok_or(ChunkIoError::RecoveryScanLimit)?;
                if attempts > MAX_RECOVERY_CANDIDATES {
                    return Err(ChunkIoError::RecoveryScanLimit);
                }
                match validate_generation_candidate(
                    reader,
                    candidate,
                    file_len,
                    header,
                    &metadata,
                    metadata_end,
                    eager_validate,
                ) {
                    Ok(snapshot) => return Ok(snapshot),
                    Err(error) => {
                        if first_error.is_none() {
                            first_error = Some(error);
                        }
                    }
                }
            }
        }
        if block_start == 0 {
            break;
        }
        block_end = block_start;
    }
    if let Some(error) = first_error {
        return Err(error);
    }
    let terminal_offset = file_len
        .checked_sub(footer_len)
        .ok_or(ChunkIoError::NoValidGeneration)?;
    match read_generation_footer_at(
        reader,
        terminal_offset,
        file_len,
        metadata_end,
        header.byte_order,
        header.minor,
    ) {
        Ok(_) => Err(ChunkIoError::NoValidGeneration),
        Err(error) => Err(error),
    }
}

fn read_generation_chunk_stored_bytes<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    descriptor: ChunkDescriptor,
    valid_end: u64,
) -> Result<Vec<u8>, ChunkIoError> {
    use std::io::SeekFrom;
    if descriptor.raw_len > MAX_CHUNK_BYTES || descriptor.stored_len > MAX_STORED_CHUNK_BYTES {
        return Err(ChunkIoError::ChunkTooLarge);
    }
    let end = descriptor
        .offset
        .checked_add(descriptor.stored_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    if end > valid_end {
        return Err(ChunkIoError::DirectoryOffsetInvalid);
    }
    let stored_size =
        usize::try_from(descriptor.stored_len).map_err(|_| ChunkIoError::ChunkTooLarge)?;
    let mut stored = Vec::new();
    stored
        .try_reserve_exact(stored_size)
        .map_err(|_| ChunkIoError::ChunkTooLarge)?;
    stored.resize(stored_size, 0);
    reader.seek(SeekFrom::Start(descriptor.offset))?;
    reader.read_exact(&mut stored)?;
    Ok(stored)
}

fn verify_and_decode_generation_chunk_mode(
    descriptor: ChunkDescriptor,
    stored: &[u8],
    verify_blake3: bool,
) -> Result<Vec<u8>, ChunkIoError> {
    if crc32c(stored) != descriptor.crc32c {
        return Err(ChunkIoError::ChecksumMismatch(descriptor.id));
    }
    if verify_blake3
        && descriptor
            .blake3
            .is_some_and(|expected| expected != *blake3::hash(stored).as_bytes())
    {
        return Err(ChunkIoError::ChunkHashMismatch(descriptor.id));
    }
    decode_chunk(descriptor.codec, stored, descriptor.raw_len, descriptor.id)
}

fn verify_and_decode_generation_chunk(
    descriptor: ChunkDescriptor,
    stored: &[u8],
) -> Result<Vec<u8>, ChunkIoError> {
    verify_and_decode_generation_chunk_mode(descriptor, stored, true)
}

fn read_generation_chunk_payload<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    descriptor: ChunkDescriptor,
    valid_end: u64,
) -> Result<Vec<u8>, ChunkIoError> {
    let stored = read_generation_chunk_stored_bytes(reader, descriptor, valid_end)?;
    verify_and_decode_generation_chunk(descriptor, &stored)
}

/// Writes a new directory-v2 generation container with a mandatory persistent chunk-ID B+tree.
/// The current writer emits header minor 6; metadata schema v1 or v2 is selected from `metadata`.
/// Secondary logical mappings are supplied explicitly. The generic writer flushes in commit order but
/// cannot promise filesystem durability; use `write_generation_container_atomic_with_indexes` for a file.
pub fn write_generation_container<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    metadata: &Metadata,
    chunks: &[EncodedChunkInput<'_>],
    order: HeaderByteOrder,
) -> Result<(), ChunkIoError> {
    write_generation_container_with_indexes(writer, metadata, chunks, &[], order)
}

/// Writes a new generation file with caller-provided persistent secondary-index mappings.
pub fn write_generation_container_with_indexes<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    metadata: &Metadata,
    chunks: &[EncodedChunkInput<'_>],
    index_entries: &[PersistentIndexEntry],
    order: HeaderByteOrder,
) -> Result<(), ChunkIoError> {
    write_generation_container_with_indexes_hooks(
        writer,
        metadata,
        chunks,
        index_entries,
        order,
        |_| Ok(()),
        |_| Ok(()),
    )
}

/// Writes a new generation-profile container directly from chunk readers.
/// Raw and Zstandard streams use the configured bounded buffer; LZ4 block mode
/// materializes one raw chunk, capped by `MAX_CHUNK_BYTES`.
pub fn write_generation_container_from_readers<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    metadata: &Metadata,
    chunks: &mut [ChunkReaderInput<'_>],
    order: HeaderByteOrder,
    options: ChunkWriteOptions,
) -> Result<(), ChunkIoError> {
    write_generation_container_from_readers_with_indexes(
        writer,
        metadata,
        chunks,
        &[],
        order,
        options,
    )
}

/// Reader-based generation writer with caller-provided persistent secondary-index mappings.
pub fn write_generation_container_from_readers_with_indexes<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    metadata: &Metadata,
    chunks: &mut [ChunkReaderInput<'_>],
    index_entries: &[PersistentIndexEntry],
    order: HeaderByteOrder,
    options: ChunkWriteOptions,
) -> Result<(), ChunkIoError> {
    write_generation_container_from_readers_with_indexes_hooks(
        writer,
        ReaderGenerationWriteInput {
            metadata,
            chunks,
            index_entries,
            order,
            options,
        },
        |_| Ok(()),
        |_| Ok(()),
    )
}

fn write_generation_container_from_readers_with_indexes_hooks<W, F1, F2>(
    writer: &mut W,
    input: ReaderGenerationWriteInput<'_, '_>,
    mut before_footer_sync: F1,
    mut after_footer_sync: F2,
) -> Result<(), ChunkIoError>
where
    W: std::io::Write + std::io::Seek,
    F1: FnMut(&mut W) -> std::io::Result<()>,
    F2: FnMut(&mut W) -> std::io::Result<()>,
{
    use std::io::SeekFrom;
    let ReaderGenerationWriteInput {
        metadata,
        chunks,
        index_entries,
        order,
        options,
    } = input;
    let options = options.validate()?;
    let mut stream_buffer = allocate_stream_buffer(options.buffer_size)?;
    let boolean_chunk_ids = validate_new_generation_reader_inputs(metadata, chunks, index_entries)?;
    let metadata_bytes = metadata.to_cbor().map_err(ChunkIoError::Metadata)?;
    if writer.seek(SeekFrom::End(0))? != 0 {
        return Err(ChunkIoError::GenerationDestinationNotEmpty);
    }

    let header = ContainerHeader {
        major: 1,
        minor: GENERATION_PROFILE_MINOR_VERSION,
        byte_order: order,
        flags: 0,
        metadata_len: metadata_bytes.len() as u64,
        directory_offset: 0,
    };
    writer.seek(SeekFrom::Start(0))?;
    writer.write_all(&header.encode())?;
    writer.write_all(&metadata_bytes)?;
    let generation_start = (CONTAINER_HEADER_LEN as u64)
        .checked_add(metadata_bytes.len() as u64)
        .ok_or(ChunkIoError::LengthOverflow)?;

    write_generation_records_new_from_readers(
        writer,
        ReaderGenerationRecordsInput {
            chunks,
            index_entries,
            order,
            commit: GenerationCommitInfo {
                generation: 1,
                prev_footer_offset: 0,
                generation_start,
            },
            boolean_chunk_ids: &boolean_chunk_ids,
            stream_buffer: &mut stream_buffer,
        },
        &mut before_footer_sync,
        &mut after_footer_sync,
    )?;
    Ok(())
}

fn validate_new_generation_reader_inputs(
    metadata: &Metadata,
    chunks: &[ChunkReaderInput<'_>],
    index_entries: &[PersistentIndexEntry],
) -> Result<std::collections::BTreeSet<u64>, ChunkIoError> {
    if chunks.len() as u64 > MAX_CHUNKS || index_entries.len() as u64 > MAX_INDEX_TREE_ENTRIES {
        return Err(ChunkIoError::TooManyChunks);
    }
    let mut ids = std::collections::BTreeSet::new();
    for chunk in chunks {
        if !ids.insert(chunk.id) {
            return Err(ChunkIoError::DuplicateChunkId);
        }
        if chunk.raw_len > MAX_CHUNK_BYTES {
            return Err(ChunkIoError::ChunkTooLarge);
        }
    }
    for entry in index_entries {
        if entry.kind == LogicalIndexKind::ChunkId {
            return Err(ChunkIoError::IndexDuplicateEntry);
        }
        if !ids.contains(&entry.chunk_id) {
            return Err(ChunkIoError::IndexChunkNotFound(entry.chunk_id));
        }
    }

    let mut boolean_chunk_ids = std::collections::BTreeSet::new();
    if let Some(model) = metadata.scientific_model() {
        model
            .validate_reader_inputs(chunks)
            .map_err(ChunkIoError::ScientificModel)?;
        for variable in &model.variables {
            if variable.data_type == DataType::Bool8 {
                boolean_chunk_ids.extend(variable.chunks.iter().map(|chunk| chunk.chunk_id));
            }
        }
    }
    Ok(boolean_chunk_ids)
}

fn write_generation_container_with_indexes_hooks<W, F1, F2>(
    writer: &mut W,
    metadata: &Metadata,
    chunks: &[EncodedChunkInput<'_>],
    index_entries: &[PersistentIndexEntry],
    order: HeaderByteOrder,
    mut before_footer_sync: F1,
    mut after_footer_sync: F2,
) -> Result<(), ChunkIoError>
where
    W: std::io::Write + std::io::Seek,
    F1: FnMut(&mut W) -> std::io::Result<()>,
    F2: FnMut(&mut W) -> std::io::Result<()>,
{
    use std::io::SeekFrom;
    validate_new_generation_inputs(chunks, index_entries)?;
    if let Some(model) = metadata.scientific_model() {
        model
            .validate_inputs(chunks)
            .map_err(ChunkIoError::ScientificModel)?;
    }
    if writer.seek(SeekFrom::End(0))? != 0 {
        return Err(ChunkIoError::GenerationDestinationNotEmpty);
    }
    let metadata_bytes = metadata.to_cbor().map_err(ChunkIoError::Metadata)?;
    let header = ContainerHeader {
        major: 1,
        minor: GENERATION_PROFILE_MINOR_VERSION,
        byte_order: order,
        flags: 0,
        metadata_len: metadata_bytes.len() as u64,
        directory_offset: 0,
    };
    writer.seek(SeekFrom::Start(0))?;
    writer.write_all(&header.encode())?;
    writer.write_all(&metadata_bytes)?;
    let generation_start = (CONTAINER_HEADER_LEN as u64)
        .checked_add(metadata_bytes.len() as u64)
        .ok_or(ChunkIoError::LengthOverflow)?;
    write_generation_records_new(
        writer,
        chunks,
        index_entries,
        order,
        GenerationCommitInfo {
            generation: 1,
            prev_footer_offset: 0,
            generation_start,
        },
        &mut before_footer_sync,
        &mut after_footer_sync,
    )?;
    Ok(())
}

fn validate_new_generation_inputs(
    chunks: &[EncodedChunkInput<'_>],
    index_entries: &[PersistentIndexEntry],
) -> Result<(), ChunkIoError> {
    if chunks.len() as u64 > MAX_CHUNKS || index_entries.len() as u64 > MAX_INDEX_TREE_ENTRIES {
        return Err(ChunkIoError::TooManyChunks);
    }
    let mut ids = std::collections::BTreeSet::new();
    for chunk in chunks {
        if !ids.insert(chunk.id) {
            return Err(ChunkIoError::DuplicateChunkId);
        }
        if chunk.data.len() as u64 > MAX_CHUNK_BYTES {
            return Err(ChunkIoError::ChunkTooLarge);
        }
    }
    for entry in index_entries {
        if entry.kind == LogicalIndexKind::ChunkId {
            return Err(ChunkIoError::IndexDuplicateEntry);
        }
        if !ids.contains(&entry.chunk_id) {
            return Err(ChunkIoError::IndexChunkNotFound(entry.chunk_id));
        }
    }
    Ok(())
}

fn write_chunks_and_directory<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    chunks: &[EncodedChunkInput<'_>],
    order: HeaderByteOrder,
    hasher: &mut blake3::Hasher,
) -> Result<(Vec<ChunkDescriptor>, u64, u64), ChunkIoError> {
    let mut descriptors = Vec::new();
    descriptors
        .try_reserve_exact(chunks.len())
        .map_err(|_| ChunkIoError::TooManyChunks)?;
    for chunk in chunks {
        let stored = encode_chunk(chunk.codec, chunk.data)?;
        if stored.len() as u64 > MAX_STORED_CHUNK_BYTES {
            return Err(ChunkIoError::ChunkTooLarge);
        }
        let offset = writer.stream_position()?;
        writer.write_all(&stored)?;
        hasher.update(&stored);
        descriptors.push(ChunkDescriptor {
            id: chunk.id,
            offset,
            stored_len: stored.len() as u64,
            raw_len: chunk.data.len() as u64,
            crc32c: crc32c(&stored),
            codec: chunk.codec,
            blake3: Some(*blake3::hash(&stored).as_bytes()),
        });
    }
    let directory_offset = writer.stream_position()?;
    let directory = encode_generation_directory(&descriptors, order)?;
    let directory_len = u64::try_from(directory.len()).map_err(|_| ChunkIoError::LengthOverflow)?;
    let directory_end = directory_offset
        .checked_add(directory_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    writer.write_all(&directory)?;
    hasher.update(&directory);
    Ok((descriptors, directory_offset, directory_end))
}

fn write_staged_chunks_and_directory<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    staged_chunks: &[StagedChunk],
    order: HeaderByteOrder,
    hasher: &mut blake3::Hasher,
) -> Result<(Vec<ChunkDescriptor>, u64, u64), ChunkIoError> {
    let mut sorted_refs: Vec<&StagedChunk> = staged_chunks.iter().collect();
    sorted_refs.sort_by_key(|chunk| chunk.id);
    let mut descriptors = Vec::new();
    descriptors
        .try_reserve_exact(sorted_refs.len())
        .map_err(|_| ChunkIoError::TooManyChunks)?;
    for chunk in sorted_refs {
        if chunk.raw_len > MAX_CHUNK_BYTES
            || chunk.stored_bytes.len() as u64 > MAX_STORED_CHUNK_BYTES
        {
            return Err(ChunkIoError::ChunkTooLarge);
        }
        let offset = writer.stream_position()?;
        writer.write_all(&chunk.stored_bytes)?;
        hasher.update(&chunk.stored_bytes);
        descriptors.push(chunk.descriptor_at(offset));
    }
    let directory_offset = writer.stream_position()?;
    let directory = encode_generation_directory(&descriptors, order)?;
    let directory_len = u64::try_from(directory.len()).map_err(|_| ChunkIoError::LengthOverflow)?;
    let directory_end = directory_offset
        .checked_add(directory_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    writer.write_all(&directory)?;
    hasher.update(&directory);
    Ok((descriptors, directory_offset, directory_end))
}

pub(crate) fn write_staged_generation_container_with_indexes_hooks<W, F1, F2>(
    writer: &mut W,
    metadata: &Metadata,
    staged_chunks: &[StagedChunk],
    index_entries: &[PersistentIndexEntry],
    order: HeaderByteOrder,
    mut before_footer_sync: F1,
    mut after_footer_sync: F2,
) -> Result<(), ChunkIoError>
where
    W: std::io::Write + std::io::Seek,
    F1: FnMut(&mut W) -> std::io::Result<()>,
    F2: FnMut(&mut W) -> std::io::Result<()>,
{
    use std::io::SeekFrom;
    if staged_chunks.len() as u64 > MAX_CHUNKS
        || index_entries.len() as u64 > MAX_INDEX_TREE_ENTRIES
    {
        return Err(ChunkIoError::TooManyChunks);
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut dummy_descriptors = Vec::with_capacity(staged_chunks.len());
    for chunk in staged_chunks {
        if !ids.insert(chunk.id) {
            return Err(ChunkIoError::DuplicateChunkId);
        }
        if chunk.raw_len > MAX_CHUNK_BYTES
            || chunk.stored_bytes.len() as u64 > MAX_STORED_CHUNK_BYTES
        {
            return Err(ChunkIoError::ChunkTooLarge);
        }
        dummy_descriptors.push(chunk.descriptor_at(0));
    }
    for entry in index_entries {
        if entry.kind == LogicalIndexKind::ChunkId {
            return Err(ChunkIoError::IndexDuplicateEntry);
        }
        if !ids.contains(&entry.chunk_id) {
            return Err(ChunkIoError::IndexChunkNotFound(entry.chunk_id));
        }
    }
    if let Some(model) = metadata.scientific_model() {
        model
            .validate_descriptors(&dummy_descriptors)
            .map_err(ChunkIoError::ScientificModel)?;
    }
    if writer.seek(SeekFrom::End(0))? != 0 {
        return Err(ChunkIoError::GenerationDestinationNotEmpty);
    }
    let metadata_bytes = metadata.to_cbor().map_err(ChunkIoError::Metadata)?;
    let header = ContainerHeader {
        major: 1,
        minor: GENERATION_PROFILE_MINOR_VERSION,
        byte_order: order,
        flags: 0,
        metadata_len: metadata_bytes.len() as u64,
        directory_offset: 0,
    };
    writer.seek(SeekFrom::Start(0))?;
    writer.write_all(&header.encode())?;
    writer.write_all(&metadata_bytes)?;
    let generation_start = (CONTAINER_HEADER_LEN as u64)
        .checked_add(metadata_bytes.len() as u64)
        .ok_or(ChunkIoError::LengthOverflow)?;

    let actual_start = writer.stream_position()?;
    if actual_start != generation_start {
        return Err(ChunkIoError::GenerationChainInvalid);
    }
    let mut hasher = blake3::Hasher::new();
    let (descriptors, directory_offset, directory_end) =
        write_staged_chunks_and_directory(writer, staged_chunks, order, &mut hasher)?;
    let mut region = IndexPageRegion {
        start: directory_end,
        end: directory_end,
        generation: 1,
    };
    let (catalog, catalog_offset) = build_initial_index_catalog(
        writer,
        &mut hasher,
        &descriptors,
        index_entries,
        1,
        order,
        &mut region,
    )?;
    let _ = catalog;
    let footer_offset = writer.stream_position()?;
    if catalog_offset.checked_add(INDEX_PAGE_SIZE as u64) != Some(footer_offset) {
        return Err(ChunkIoError::IndexCatalogInvalid);
    }
    writer.flush()?;
    before_footer_sync(writer)?;
    let footer = GenerationFooter {
        generation: 1,
        prev_footer_offset: 0,
        directory_offset,
        index_catalog_offset: catalog_offset,
        digest: *hasher.finalize().as_bytes(),
    };
    writer.write_all(&encode_generation_footer(footer, order))?;
    writer.flush()?;
    after_footer_sync(writer)?;
    Ok(())
}

fn store_u16(bytes: &mut [u8], offset: usize, value: u16, order: HeaderByteOrder) {
    let raw = match order {
        HeaderByteOrder::LittleEndian => value.to_le_bytes(),
        HeaderByteOrder::BigEndian => value.to_be_bytes(),
    };
    bytes[offset..offset + 2].copy_from_slice(&raw);
}
fn store_u32(bytes: &mut [u8], offset: usize, value: u32, order: HeaderByteOrder) {
    let raw = match order {
        HeaderByteOrder::LittleEndian => value.to_le_bytes(),
        HeaderByteOrder::BigEndian => value.to_be_bytes(),
    };
    bytes[offset..offset + 4].copy_from_slice(&raw);
}
fn store_u64(bytes: &mut [u8], offset: usize, value: u64, order: HeaderByteOrder) {
    let raw = match order {
        HeaderByteOrder::LittleEndian => value.to_le_bytes(),
        HeaderByteOrder::BigEndian => value.to_be_bytes(),
    };
    bytes[offset..offset + 8].copy_from_slice(&raw);
}

fn encode_index_leaf_page(
    records: &[IndexCompositeKey],
    generation: u64,
    order: HeaderByteOrder,
) -> Result<[u8; INDEX_PAGE_SIZE], ChunkIoError> {
    if records.is_empty()
        || records.len() > INDEX_LEAF_CAPACITY
        || records.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(ChunkIoError::IndexTreeInvalid);
    }
    let mut bytes = [0u8; INDEX_PAGE_SIZE];
    encode_index_page_header(
        &mut bytes,
        INDEX_PAGE_LEAF,
        0,
        records.len(),
        generation,
        order,
    )?;
    let mut cursor = INDEX_PAGE_HEADER_LEN;
    for record in records {
        bytes[cursor..cursor + 32].copy_from_slice(&record.key);
        store_u64(&mut bytes, cursor + 32, record.chunk_id, order);
        cursor += INDEX_LEAF_ENTRY_LEN;
    }
    finalize_index_page(&mut bytes);
    Ok(bytes)
}

fn encode_index_internal_page(
    separators: &[IndexCompositeKey],
    children: &[u64],
    level: u16,
    generation: u64,
    order: HeaderByteOrder,
) -> Result<[u8; INDEX_PAGE_SIZE], ChunkIoError> {
    if level == 0
        || separators.is_empty()
        || separators.len() > INDEX_INTERNAL_CAPACITY
        || children.len() != separators.len() + 1
        || children.contains(&0)
        || separators.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(ChunkIoError::IndexTreeInvalid);
    }
    let mut bytes = [0u8; INDEX_PAGE_SIZE];
    encode_index_page_header(
        &mut bytes,
        INDEX_PAGE_INTERNAL,
        level,
        separators.len(),
        generation,
        order,
    )?;
    store_u64(&mut bytes, INDEX_PAGE_HEADER_LEN, children[0], order);
    let mut cursor = INDEX_PAGE_HEADER_LEN + 8;
    for (index, separator) in separators.iter().enumerate() {
        bytes[cursor..cursor + 32].copy_from_slice(&separator.key);
        store_u64(&mut bytes, cursor + 32, separator.chunk_id, order);
        store_u64(&mut bytes, cursor + 40, children[index + 1], order);
        cursor += INDEX_INTERNAL_ENTRY_LEN;
    }
    finalize_index_page(&mut bytes);
    Ok(bytes)
}

fn encode_index_catalog_page(
    entries: &[IndexCatalogEntry],
    generation: u64,
    order: HeaderByteOrder,
) -> Result<[u8; INDEX_PAGE_SIZE], ChunkIoError> {
    if entries.len() > MAX_INDEX_CATALOG_ENTRIES
        || entries
            .windows(2)
            .any(|pair| pair[0].kind_id >= pair[1].kind_id)
    {
        return Err(ChunkIoError::IndexCatalogInvalid);
    }
    let mut bytes = [0u8; INDEX_PAGE_SIZE];
    encode_index_page_header(
        &mut bytes,
        INDEX_PAGE_CATALOG,
        0,
        entries.len(),
        generation,
        order,
    )?;
    let mut cursor = INDEX_PAGE_HEADER_LEN;
    for entry in entries {
        store_u16(&mut bytes, cursor, entry.kind_id, order);
        store_u16(&mut bytes, cursor + 2, entry.flags, order);
        store_u32(&mut bytes, cursor + 4, 0, order);
        store_u64(&mut bytes, cursor + 8, entry.root_offset, order);
        store_u64(&mut bytes, cursor + 16, entry.item_count, order);
        cursor += INDEX_CATALOG_ENTRY_LEN;
    }
    finalize_index_page(&mut bytes);
    Ok(bytes)
}

fn encode_index_manifest_page(
    payload: &[u8],
    generation: u64,
    order: HeaderByteOrder,
) -> Result<[u8; INDEX_PAGE_SIZE], ChunkIoError> {
    if payload.len() > INDEX_PAGE_SIZE - INDEX_PAGE_HEADER_LEN {
        return Err(ChunkIoError::Metadata(MetadataError::TooLarge));
    }
    let mut bytes = [0u8; INDEX_PAGE_SIZE];
    encode_index_page_header(
        &mut bytes,
        INDEX_PAGE_MANIFEST,
        0,
        payload.len(),
        generation,
        order,
    )?;
    bytes[INDEX_PAGE_HEADER_LEN..INDEX_PAGE_HEADER_LEN + payload.len()].copy_from_slice(payload);
    finalize_index_page(&mut bytes);
    Ok(bytes)
}

fn write_index_page<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    hasher: &mut blake3::Hasher,
    bytes: &[u8; INDEX_PAGE_SIZE],
    region: &mut IndexPageRegion,
    page_count: &mut u64,
) -> Result<u64, ChunkIoError> {
    use std::io::SeekFrom;
    *page_count = (*page_count)
        .checked_add(1)
        .ok_or(ChunkIoError::IndexTreePageLimit)?;
    if *page_count > MAX_INDEX_TREE_PAGES {
        return Err(ChunkIoError::IndexTreePageLimit);
    }
    // Copy-on-write insertion reads old pages through the same reader/writer handle;
    // always seek back to the append frontier before emitting a new immutable page.
    writer.seek(SeekFrom::Start(region.end))?;
    let offset = writer.stream_position()?;
    if offset != region.end {
        return Err(ChunkIoError::IndexPageBoundsInvalid);
    }
    writer.write_all(bytes)?;
    hasher.update(bytes);
    region.end = writer.stream_position()?;
    Ok(offset)
}

#[derive(Debug, Clone, Copy)]
struct IndexTreeNodeRef {
    offset: u64,
    first: IndexCompositeKey,
}

fn build_btree_bulk<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    hasher: &mut blake3::Hasher,
    records: &mut [IndexCompositeKey],
    generation: u64,
    order: HeaderByteOrder,
    region: &mut IndexPageRegion,
    page_count: &mut u64,
) -> Result<(u64, u16), ChunkIoError> {
    if records.len() as u64 > MAX_INDEX_TREE_ENTRIES {
        return Err(ChunkIoError::IndexTreePageLimit);
    }
    records.sort_unstable();
    if records.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ChunkIoError::IndexDuplicateEntry);
    }
    if records.is_empty() {
        return Ok((0, 0));
    }
    let leaf_count = records.len().div_ceil(INDEX_LEAF_CAPACITY);
    let mut level_nodes = Vec::new();
    level_nodes
        .try_reserve_exact(leaf_count)
        .map_err(|_| ChunkIoError::IndexTreePageLimit)?;
    for group in records.chunks(INDEX_LEAF_CAPACITY) {
        let page = encode_index_leaf_page(group, generation, order)?;
        let offset = write_index_page(writer, hasher, &page, region, page_count)?;
        level_nodes.push(IndexTreeNodeRef {
            offset,
            first: group[0],
        });
    }
    let mut level = 0u16;
    while level_nodes.len() > 1 {
        level = level
            .checked_add(1)
            .ok_or(ChunkIoError::IndexTreeDepthExceeded)?;
        if level > MAX_INDEX_TREE_DEPTH {
            return Err(ChunkIoError::IndexTreeDepthExceeded);
        }
        let group_count = level_nodes.len().div_ceil(INDEX_INTERNAL_CAPACITY + 1);
        let base_size = level_nodes.len() / group_count;
        let remainder = level_nodes.len() % group_count;
        let mut next_level = Vec::new();
        next_level
            .try_reserve_exact(group_count)
            .map_err(|_| ChunkIoError::IndexTreePageLimit)?;
        let mut cursor = 0usize;
        for group_index in 0..group_count {
            let group_size = base_size + if group_index < remainder { 1 } else { 0 };
            let group = &level_nodes[cursor..cursor + group_size];
            if group.len() < 2 || group.len() > INDEX_INTERNAL_CAPACITY + 1 {
                return Err(ChunkIoError::IndexTreeInvalid);
            }
            let children = group.iter().map(|item| item.offset).collect::<Vec<_>>();
            let separators = group
                .iter()
                .skip(1)
                .map(|item| item.first)
                .collect::<Vec<_>>();
            let page =
                encode_index_internal_page(&separators, &children, level, generation, order)?;
            let offset = write_index_page(writer, hasher, &page, region, page_count)?;
            next_level.push(IndexTreeNodeRef {
                offset,
                first: group[0].first,
            });
            cursor += group_size;
        }
        level_nodes = next_level;
    }
    let root = level_nodes
        .first()
        .copied()
        .ok_or(ChunkIoError::IndexTreeInvalid)?;
    Ok((root.offset, level))
}

fn build_initial_index_catalog<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    hasher: &mut blake3::Hasher,
    chunks: &[ChunkDescriptor],
    index_entries: &[PersistentIndexEntry],
    generation: u64,
    order: HeaderByteOrder,
    region: &mut IndexPageRegion,
) -> Result<(Vec<IndexCatalogEntry>, u64), ChunkIoError> {
    let mut by_kind = std::collections::BTreeMap::<u16, Vec<IndexCompositeKey>>::new();
    for descriptor in chunks {
        by_kind
            .entry(LogicalIndexKind::ChunkId as u16)
            .or_default()
            .push(IndexCompositeKey {
                key: PersistentIndexEntry::key_u64(descriptor.id),
                chunk_id: descriptor.id,
            });
    }
    let ids: std::collections::BTreeSet<u64> =
        chunks.iter().map(|descriptor| descriptor.id).collect();
    for entry in index_entries {
        if !ids.contains(&entry.chunk_id) {
            return Err(ChunkIoError::IndexChunkNotFound(entry.chunk_id));
        }
        by_kind
            .entry(entry.kind as u16)
            .or_default()
            .push(IndexCompositeKey {
                key: entry.key,
                chunk_id: entry.chunk_id,
            });
    }
    by_kind.entry(LogicalIndexKind::ChunkId as u16).or_default();
    if by_kind.len() > MAX_INDEX_CATALOG_ENTRIES {
        return Err(ChunkIoError::IndexCatalogInvalid);
    }
    let mut page_count = 0u64;
    let mut catalog = Vec::new();
    catalog
        .try_reserve_exact(by_kind.len())
        .map_err(|_| ChunkIoError::IndexTreePageLimit)?;
    for (kind_id, mut records) in by_kind {
        if records.len() as u64 > MAX_INDEX_TREE_ENTRIES {
            return Err(ChunkIoError::IndexTreePageLimit);
        }
        let item_count = records.len() as u64;
        let (root_offset, level) = build_btree_bulk(
            writer,
            hasher,
            &mut records,
            generation,
            order,
            region,
            &mut page_count,
        )?;
        catalog.push(IndexCatalogEntry {
            kind_id,
            flags: if kind_id == LogicalIndexKind::ChunkId as u16 {
                1
            } else {
                0
            },
            root_offset,
            item_count,
            level,
        });
    }
    let page = encode_index_catalog_page(&catalog, generation, order)?;
    let catalog_offset = write_index_page(writer, hasher, &page, region, &mut page_count)?;
    Ok((catalog, catalog_offset))
}

fn encode_generation_directory(
    descriptors: &[ChunkDescriptor],
    order: HeaderByteOrder,
) -> Result<Vec<u8>, ChunkIoError> {
    if descriptors.len() as u64 > MAX_CHUNKS {
        return Err(ChunkIoError::TooManyChunks);
    }
    let directory_len = CHUNK_DIRECTORY_HEADER_LEN
        .checked_add(
            (descriptors.len() as u64)
                .checked_mul(GENERATION_DESCRIPTOR_LEN)
                .ok_or(ChunkIoError::LengthOverflow)?,
        )
        .ok_or(ChunkIoError::LengthOverflow)?;
    let capacity = usize::try_from(directory_len).map_err(|_| ChunkIoError::LengthOverflow)?;
    let mut directory = Vec::new();
    directory
        .try_reserve_exact(capacity)
        .map_err(|_| ChunkIoError::TooManyChunks)?;
    directory.extend_from_slice(&CHUNK_DIRECTORY_MAGIC);
    put_u16(&mut directory, GENERATION_DIRECTORY_VERSION, order);
    put_u16(&mut directory, GENERATION_DESCRIPTOR_LEN as u16, order);
    put_u32(&mut directory, descriptors.len() as u32, order);
    put_u32(&mut directory, 0, order);
    for descriptor in descriptors {
        put_u64(&mut directory, descriptor.id, order);
        put_u64(&mut directory, descriptor.offset, order);
        put_u64(&mut directory, descriptor.stored_len, order);
        put_u64(&mut directory, descriptor.raw_len, order);
        put_u32(&mut directory, descriptor.crc32c, order);
        directory.push(descriptor.codec as u8);
        directory.push(0);
        put_u16(&mut directory, 0, order);
        let digest = descriptor
            .blake3
            .ok_or(ChunkIoError::DirectoryBoundsInvalid)?;
        directory.extend_from_slice(&digest);
    }
    Ok(directory)
}

struct GenerationCommitInfo {
    generation: u64,
    prev_footer_offset: u64,
    generation_start: u64,
}

struct AppendGenerationRecords<'slice, 'data> {
    chunks: &'slice [EncodedChunkInput<'data>],
    index_entries: &'slice [PersistentIndexEntry],
    existing_descriptors: &'slice [ChunkDescriptor],
    prior_catalog: &'slice [IndexCatalogEntry],
    prior_regions: &'slice [IndexPageRegion],
    order: HeaderByteOrder,
    commit: GenerationCommitInfo,
    manifest_page: Option<[u8; INDEX_PAGE_SIZE]>,
}

fn append_index_page_to_region<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    hasher: &mut blake3::Hasher,
    page: &[u8; INDEX_PAGE_SIZE],
    regions: &mut [IndexPageRegion],
    page_count: &mut u64,
) -> Result<u64, ChunkIoError> {
    let region = regions
        .last_mut()
        .ok_or(ChunkIoError::IndexPageBoundsInvalid)?;
    write_index_page(writer, hasher, page, region, page_count)
}

#[derive(Debug, Clone, Copy)]
enum BTreeInsertOutcome {
    Single {
        offset: u64,
        first: IndexCompositeKey,
        level: u16,
    },
    Split {
        left_offset: u64,
        right_offset: u64,
        separator: IndexCompositeKey,
        first: IndexCompositeKey,
        level: u16,
    },
}

fn first_key_in_subtree<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    offset: u64,
    regions: &[IndexPageRegion],
    order: HeaderByteOrder,
    generation: u64,
    depth: u16,
) -> Result<IndexCompositeKey, ChunkIoError> {
    if depth > MAX_INDEX_TREE_DEPTH {
        return Err(ChunkIoError::IndexTreeDepthExceeded);
    }
    match read_index_page(reader, offset, regions, order, generation)? {
        DecodedIndexPage::Leaf { records } => records
            .first()
            .copied()
            .ok_or(ChunkIoError::IndexTreeInvalid),
        DecodedIndexPage::Internal { children, .. } => first_key_in_subtree(
            reader,
            *children.first().ok_or(ChunkIoError::IndexTreeInvalid)?,
            regions,
            order,
            generation,
            depth + 1,
        ),
        DecodedIndexPage::Catalog { .. } | DecodedIndexPage::Manifest { .. } => {
            Err(ChunkIoError::IndexTreeInvalid)
        }
    }
}

struct BTreeCowWriteContext<'a> {
    generation: u64,
    order: HeaderByteOrder,
    regions: &'a mut Vec<IndexPageRegion>,
    hasher: &'a mut blake3::Hasher,
    page_count: &'a mut u64,
}

fn insert_btree_node_cow<W: std::io::Read + std::io::Write + std::io::Seek>(
    writer: &mut W,
    offset: u64,
    record: IndexCompositeKey,
    context: &mut BTreeCowWriteContext<'_>,
    depth: u16,
) -> Result<BTreeInsertOutcome, ChunkIoError> {
    if depth > MAX_INDEX_TREE_DEPTH {
        return Err(ChunkIoError::IndexTreeDepthExceeded);
    }
    let page = read_index_page(
        writer,
        offset,
        context.regions,
        context.order,
        context.generation,
    )?;
    match page {
        DecodedIndexPage::Leaf { mut records } => {
            match records.binary_search(&record) {
                Ok(_) => return Err(ChunkIoError::IndexDuplicateEntry),
                Err(position) => records.insert(position, record),
            }
            if records.len() <= INDEX_LEAF_CAPACITY {
                let first = records
                    .first()
                    .copied()
                    .ok_or(ChunkIoError::IndexTreeInvalid)?;
                let page = encode_index_leaf_page(&records, context.generation, context.order)?;
                let new_offset = append_index_page_to_region(
                    writer,
                    context.hasher,
                    &page,
                    context.regions,
                    context.page_count,
                )?;
                Ok(BTreeInsertOutcome::Single {
                    offset: new_offset,
                    first,
                    level: 0,
                })
            } else {
                let split_at = records.len() / 2;
                let right_records = records.split_off(split_at);
                let left_first = records
                    .first()
                    .copied()
                    .ok_or(ChunkIoError::IndexTreeInvalid)?;
                let separator = right_records
                    .first()
                    .copied()
                    .ok_or(ChunkIoError::IndexTreeInvalid)?;
                let left_page =
                    encode_index_leaf_page(&records, context.generation, context.order)?;
                let right_page =
                    encode_index_leaf_page(&right_records, context.generation, context.order)?;
                let left_offset = append_index_page_to_region(
                    writer,
                    context.hasher,
                    &left_page,
                    context.regions,
                    context.page_count,
                )?;
                let right_offset = append_index_page_to_region(
                    writer,
                    context.hasher,
                    &right_page,
                    context.regions,
                    context.page_count,
                )?;
                Ok(BTreeInsertOutcome::Split {
                    left_offset,
                    right_offset,
                    separator,
                    first: left_first,
                    level: 0,
                })
            }
        }
        DecodedIndexPage::Internal {
            level,
            mut separators,
            mut children,
        } => {
            let child_index = separators.partition_point(|separator| *separator <= record);
            let child_offset = *children
                .get(child_index)
                .ok_or(ChunkIoError::IndexTreeInvalid)?;
            let child_outcome =
                insert_btree_node_cow(writer, child_offset, record, context, depth + 1)?;
            let (left_offset, child_first, split) = match child_outcome {
                BTreeInsertOutcome::Single { offset, first, .. } => (offset, first, None),
                BTreeInsertOutcome::Split {
                    left_offset,
                    right_offset,
                    separator,
                    first,
                    ..
                } => (left_offset, first, Some((right_offset, separator))),
            };
            children[child_index] = left_offset;
            if child_index > 0 {
                separators[child_index - 1] = child_first;
            }
            if let Some((right_offset, separator)) = split {
                separators.insert(child_index, separator);
                children.insert(child_index + 1, right_offset);
            }
            let first = if child_index == 0 {
                child_first
            } else {
                first_key_in_subtree(
                    writer,
                    children[0],
                    context.regions,
                    context.order,
                    context.generation,
                    depth + 1,
                )?
            };
            if separators.len() <= INDEX_INTERNAL_CAPACITY {
                let page = encode_index_internal_page(
                    &separators,
                    &children,
                    level,
                    context.generation,
                    context.order,
                )?;
                let new_offset = append_index_page_to_region(
                    writer,
                    context.hasher,
                    &page,
                    context.regions,
                    context.page_count,
                )?;
                Ok(BTreeInsertOutcome::Single {
                    offset: new_offset,
                    first,
                    level,
                })
            } else {
                let promote_at = separators.len() / 2;
                let separator = separators[promote_at];
                let right_separators = separators.split_off(promote_at + 1);
                separators.pop();
                let right_children = children.split_off(promote_at + 1);
                let left_page = encode_index_internal_page(
                    &separators,
                    &children,
                    level,
                    context.generation,
                    context.order,
                )?;
                let right_page = encode_index_internal_page(
                    &right_separators,
                    &right_children,
                    level,
                    context.generation,
                    context.order,
                )?;
                let left_offset = append_index_page_to_region(
                    writer,
                    context.hasher,
                    &left_page,
                    context.regions,
                    context.page_count,
                )?;
                let right_offset = append_index_page_to_region(
                    writer,
                    context.hasher,
                    &right_page,
                    context.regions,
                    context.page_count,
                )?;
                Ok(BTreeInsertOutcome::Split {
                    left_offset,
                    right_offset,
                    separator,
                    first,
                    level,
                })
            }
        }
        DecodedIndexPage::Catalog { .. } | DecodedIndexPage::Manifest { .. } => {
            Err(ChunkIoError::IndexTreeInvalid)
        }
    }
}

fn insert_btree_cow<W: std::io::Read + std::io::Write + std::io::Seek>(
    writer: &mut W,
    root_offset: u64,
    record: IndexCompositeKey,
    context: &mut BTreeCowWriteContext<'_>,
) -> Result<(u64, u16), ChunkIoError> {
    if root_offset == 0 {
        let records = [record];
        let page = encode_index_leaf_page(&records, context.generation, context.order)?;
        let offset = append_index_page_to_region(
            writer,
            context.hasher,
            &page,
            context.regions,
            context.page_count,
        )?;
        return Ok((offset, 0));
    }
    match insert_btree_node_cow(writer, root_offset, record, context, 1)? {
        BTreeInsertOutcome::Single { offset, level, .. } => Ok((offset, level)),
        BTreeInsertOutcome::Split {
            left_offset,
            right_offset,
            separator,
            level,
            ..
        } => {
            let new_level = level
                .checked_add(1)
                .ok_or(ChunkIoError::IndexTreeDepthExceeded)?;
            if new_level > MAX_INDEX_TREE_DEPTH {
                return Err(ChunkIoError::IndexTreeDepthExceeded);
            }
            let children = [left_offset, right_offset];
            let separators = [separator];
            let page = encode_index_internal_page(
                &separators,
                &children,
                new_level,
                context.generation,
                context.order,
            )?;
            let offset = append_index_page_to_region(
                writer,
                context.hasher,
                &page,
                context.regions,
                context.page_count,
            )?;
            Ok((offset, new_level))
        }
    }
}

fn write_generation_records_new<W, F1, F2>(
    writer: &mut W,
    chunks: &[EncodedChunkInput<'_>],
    index_entries: &[PersistentIndexEntry],
    order: HeaderByteOrder,
    commit: GenerationCommitInfo,
    before_footer_sync: &mut F1,
    after_footer_sync: &mut F2,
) -> Result<u64, ChunkIoError>
where
    W: std::io::Write + std::io::Seek,
    F1: FnMut(&mut W) -> std::io::Result<()>,
    F2: FnMut(&mut W) -> std::io::Result<()>,
{
    let actual_start = writer.stream_position()?;
    if actual_start != commit.generation_start {
        return Err(ChunkIoError::GenerationChainInvalid);
    }
    let mut hasher = blake3::Hasher::new();
    let (descriptors, directory_offset, directory_end) =
        write_chunks_and_directory(writer, chunks, order, &mut hasher)?;
    let mut region = IndexPageRegion {
        start: directory_end,
        end: directory_end,
        generation: commit.generation,
    };
    let (catalog, catalog_offset) = build_initial_index_catalog(
        writer,
        &mut hasher,
        &descriptors,
        index_entries,
        commit.generation,
        order,
        &mut region,
    )?;
    let _ = catalog;
    let footer_offset = writer.stream_position()?;
    if catalog_offset.checked_add(INDEX_PAGE_SIZE as u64) != Some(footer_offset) {
        return Err(ChunkIoError::IndexCatalogInvalid);
    }
    let valid_end = footer_offset
        .checked_add(GENERATION_FOOTER_LEN)
        .ok_or(ChunkIoError::LengthOverflow)?;
    writer.flush()?;
    before_footer_sync(writer)?;
    let footer = GenerationFooter {
        generation: commit.generation,
        prev_footer_offset: commit.prev_footer_offset,
        directory_offset,
        index_catalog_offset: catalog_offset,
        digest: *hasher.finalize().as_bytes(),
    };
    writer.write_all(&encode_generation_footer(footer, order))?;
    writer.flush()?;
    after_footer_sync(writer)?;
    Ok(valid_end)
}

fn write_generation_chunks_and_directory_from_readers<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    chunks: &mut [ChunkReaderInput<'_>],
    order: HeaderByteOrder,
    hasher: &mut blake3::Hasher,
    boolean_chunk_ids: &std::collections::BTreeSet<u64>,
    stream_buffer: &mut [u8],
) -> Result<(Vec<ChunkDescriptor>, u64, u64), ChunkIoError> {
    use std::io::Write;
    let mut descriptors = Vec::new();
    descriptors
        .try_reserve_exact(chunks.len())
        .map_err(|_| ChunkIoError::TooManyChunks)?;

    for chunk in chunks.iter_mut() {
        let offset = writer.stream_position()?;
        let validate_boolean = boolean_chunk_ids.contains(&chunk.id);
        let (stored_len, checksum, digest) = {
            let mut counted = CountingIntegrityWriter::new(writer, hasher);
            match chunk.codec {
                ChunkCodec::Raw => {
                    copy_exactly_n_checked(
                        chunk.reader,
                        &mut counted,
                        chunk.raw_len,
                        stream_buffer,
                        validate_boolean,
                    )
                    .map_err(|error| match error {
                        StreamCopyError::Input(error) | StreamCopyError::Output(error) => {
                            ChunkIoError::Io(error)
                        }
                        StreamCopyError::InvalidBooleanValue => {
                            ChunkIoError::ScientificModel(ScientificModelError::InvalidBooleanValue)
                        }
                    })?;
                }
                ChunkCodec::Zstandard => {
                    let mut encoder = zstd::stream::write::Encoder::new(&mut counted, 3)
                        .map_err(|error| ChunkIoError::Compression(error.to_string()))?;
                    copy_exactly_n_checked(
                        chunk.reader,
                        &mut encoder,
                        chunk.raw_len,
                        stream_buffer,
                        validate_boolean,
                    )
                    .map_err(|error| match error {
                        StreamCopyError::Input(error) => ChunkIoError::Io(error),
                        StreamCopyError::Output(error) => {
                            ChunkIoError::Compression(error.to_string())
                        }
                        StreamCopyError::InvalidBooleanValue => {
                            ChunkIoError::ScientificModel(ScientificModelError::InvalidBooleanValue)
                        }
                    })?;
                    encoder
                        .finish()
                        .map_err(|error| ChunkIoError::Compression(error.to_string()))?;
                }
                ChunkCodec::Lz4 => {
                    let raw_size =
                        usize::try_from(chunk.raw_len).map_err(|_| ChunkIoError::ChunkTooLarge)?;
                    let mut raw = Vec::new();
                    raw.try_reserve_exact(raw_size)
                        .map_err(|_| ChunkIoError::AllocationFailed)?;
                    raw.resize(raw_size, 0);
                    chunk.reader.read_exact(&mut raw)?;
                    if validate_boolean && raw.iter().any(|value| *value > 1) {
                        return Err(ChunkIoError::ScientificModel(
                            ScientificModelError::InvalidBooleanValue,
                        ));
                    }
                    let encoded = lz4_flex::block::compress(&raw);
                    if encoded.len() as u64 > MAX_STORED_CHUNK_BYTES {
                        return Err(ChunkIoError::ChunkTooLarge);
                    }
                    counted.write_all(&encoded)?;
                }
                ChunkCodec::ZstdByteShuffle4
                | ChunkCodec::ZstdByteShuffle8
                | ChunkCodec::ZstdByteShuffle2 => {
                    let item_size = match chunk.codec {
                        ChunkCodec::ZstdByteShuffle4 => 4,
                        ChunkCodec::ZstdByteShuffle8 => 8,
                        ChunkCodec::ZstdByteShuffle2 => 2,
                        _ => unreachable!(),
                    };
                    let raw_size =
                        usize::try_from(chunk.raw_len).map_err(|_| ChunkIoError::ChunkTooLarge)?;
                    let mut raw = Vec::new();
                    raw.try_reserve_exact(raw_size)
                        .map_err(|_| ChunkIoError::AllocationFailed)?;
                    raw.resize(raw_size, 0);
                    chunk.reader.read_exact(&mut raw)?;
                    if validate_boolean && raw.iter().any(|value| *value > 1) {
                        return Err(ChunkIoError::ScientificModel(
                            ScientificModelError::InvalidBooleanValue,
                        ));
                    }
                    let shuffled = quantization::byte_shuffle(&raw, item_size)?;
                    let mut encoder = zstd::stream::write::Encoder::new(&mut counted, 3)
                        .map_err(|error| ChunkIoError::Compression(error.to_string()))?;
                    encoder
                        .write_all(&shuffled)
                        .map_err(|error| ChunkIoError::Compression(error.to_string()))?;
                    encoder
                        .finish()
                        .map_err(|error| ChunkIoError::Compression(error.to_string()))?;
                }
            }
            counted.flush()?;
            counted.result()
        };
        if stored_len > MAX_STORED_CHUNK_BYTES {
            return Err(ChunkIoError::ChunkTooLarge);
        }
        descriptors.push(ChunkDescriptor {
            id: chunk.id,
            offset,
            stored_len,
            raw_len: chunk.raw_len,
            crc32c: checksum,
            codec: chunk.codec,
            blake3: Some(digest),
        });
    }

    let directory_offset = writer.stream_position()?;
    let directory = encode_generation_directory(&descriptors, order)?;
    let directory_len = u64::try_from(directory.len()).map_err(|_| ChunkIoError::LengthOverflow)?;
    let directory_end = directory_offset
        .checked_add(directory_len)
        .ok_or(ChunkIoError::LengthOverflow)?;
    writer.write_all(&directory)?;
    hasher.update(&directory);
    Ok((descriptors, directory_offset, directory_end))
}

fn write_generation_records_new_from_readers<W, F1, F2>(
    writer: &mut W,
    input: ReaderGenerationRecordsInput<'_, '_>,
    before_footer_sync: &mut F1,
    after_footer_sync: &mut F2,
) -> Result<u64, ChunkIoError>
where
    W: std::io::Write + std::io::Seek,
    F1: FnMut(&mut W) -> std::io::Result<()>,
    F2: FnMut(&mut W) -> std::io::Result<()>,
{
    let ReaderGenerationRecordsInput {
        chunks,
        index_entries,
        order,
        commit,
        boolean_chunk_ids,
        stream_buffer,
    } = input;
    let actual_start = writer.stream_position()?;
    if actual_start != commit.generation_start {
        return Err(ChunkIoError::GenerationChainInvalid);
    }
    let mut hasher = blake3::Hasher::new();
    let (descriptors, directory_offset, directory_end) =
        write_generation_chunks_and_directory_from_readers(
            writer,
            chunks,
            order,
            &mut hasher,
            boolean_chunk_ids,
            stream_buffer,
        )?;
    let mut region = IndexPageRegion {
        start: directory_end,
        end: directory_end,
        generation: commit.generation,
    };
    let (catalog, catalog_offset) = build_initial_index_catalog(
        writer,
        &mut hasher,
        &descriptors,
        index_entries,
        commit.generation,
        order,
        &mut region,
    )?;
    let _ = catalog;
    let footer_offset = writer.stream_position()?;
    if catalog_offset.checked_add(INDEX_PAGE_SIZE as u64) != Some(footer_offset) {
        return Err(ChunkIoError::IndexCatalogInvalid);
    }
    let valid_end = footer_offset
        .checked_add(GENERATION_FOOTER_LEN)
        .ok_or(ChunkIoError::LengthOverflow)?;
    writer.flush()?;
    before_footer_sync(writer)?;
    let footer = GenerationFooter {
        generation: commit.generation,
        prev_footer_offset: commit.prev_footer_offset,
        directory_offset,
        index_catalog_offset: catalog_offset,
        digest: *hasher.finalize().as_bytes(),
    };
    writer.write_all(&encode_generation_footer(footer, order))?;
    writer.flush()?;
    after_footer_sync(writer)?;
    Ok(valid_end)
}

fn write_generation_records_append<W, F1, F2>(
    writer: &mut W,
    input: AppendGenerationRecords<'_, '_>,
    before_footer_sync: &mut F1,
    after_footer_sync: &mut F2,
) -> Result<u64, ChunkIoError>
where
    W: std::io::Read + std::io::Write + std::io::Seek,
    F1: FnMut(&mut W) -> std::io::Result<()>,
    F2: FnMut(&mut W) -> std::io::Result<()>,
{
    let AppendGenerationRecords {
        chunks,
        index_entries,
        existing_descriptors,
        prior_catalog,
        prior_regions,
        order,
        commit,
        manifest_page,
    } = input;
    let actual_start = writer.stream_position()?;
    if actual_start != commit.generation_start {
        return Err(ChunkIoError::GenerationChainInvalid);
    }
    let mut hasher = blake3::Hasher::new();
    let (new_descriptors, directory_offset, directory_end) =
        write_chunks_and_directory(writer, chunks, order, &mut hasher)?;
    let mut regions = prior_regions.to_vec();
    regions
        .try_reserve(1)
        .map_err(|_| ChunkIoError::IndexTreePageLimit)?;
    regions.push(IndexPageRegion {
        start: directory_end,
        end: directory_end,
        generation: commit.generation,
    });
    let mut page_count = 0u64;
    let mut cow_context = BTreeCowWriteContext {
        generation: commit.generation,
        order,
        regions: &mut regions,
        hasher: &mut hasher,
        page_count: &mut page_count,
    };
    let mut catalog = prior_catalog.to_vec();
    let all_ids: std::collections::BTreeSet<u64> = existing_descriptors
        .iter()
        .chain(new_descriptors.iter())
        .map(|descriptor| descriptor.id)
        .collect();

    let chunk_kind = LogicalIndexKind::ChunkId as u16;
    let chunk_root_index = catalog
        .iter()
        .position(|entry| entry.kind_id == chunk_kind)
        .ok_or(ChunkIoError::IndexCatalogInvalid)?;
    for descriptor in &new_descriptors {
        let entry = IndexCompositeKey {
            key: PersistentIndexEntry::key_u64(descriptor.id),
            chunk_id: descriptor.id,
        };
        let root = catalog[chunk_root_index].root_offset;
        let (new_root, new_level) = insert_btree_cow(writer, root, entry, &mut cow_context)?;
        catalog[chunk_root_index].root_offset = new_root;
        catalog[chunk_root_index].item_count = catalog[chunk_root_index]
            .item_count
            .checked_add(1)
            .ok_or(ChunkIoError::IndexTreePageLimit)?;
        catalog[chunk_root_index].level = new_level;
    }

    let mut updates = std::collections::BTreeMap::<u16, Vec<IndexCompositeKey>>::new();
    for entry in index_entries {
        if entry.kind == LogicalIndexKind::ChunkId {
            return Err(ChunkIoError::IndexDuplicateEntry);
        }
        if !all_ids.contains(&entry.chunk_id) {
            return Err(ChunkIoError::IndexChunkNotFound(entry.chunk_id));
        }
        updates
            .entry(entry.kind as u16)
            .or_default()
            .push(IndexCompositeKey {
                key: entry.key,
                chunk_id: entry.chunk_id,
            });
    }
    let new_kind_count = updates
        .keys()
        .filter(|kind_id| !catalog.iter().any(|entry| entry.kind_id == **kind_id))
        .count();
    if new_kind_count + catalog.len() > MAX_INDEX_CATALOG_ENTRIES {
        return Err(ChunkIoError::IndexCatalogInvalid);
    }
    for (kind_id, mut records) in updates {
        records.sort_unstable();
        if records.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ChunkIoError::IndexDuplicateEntry);
        }
        let existing_index = catalog.iter().position(|entry| entry.kind_id == kind_id);
        let (mut root, mut item_count, mut level) = if let Some(index) = existing_index {
            let info = catalog[index];
            (info.root_offset, info.item_count, info.level)
        } else {
            (0, 0, 0)
        };
        for record in records {
            let (new_root, new_level) = insert_btree_cow(writer, root, record, &mut cow_context)?;
            root = new_root;
            item_count = item_count
                .checked_add(1)
                .ok_or(ChunkIoError::IndexTreePageLimit)?;
            level = new_level;
        }
        if item_count > MAX_INDEX_TREE_ENTRIES {
            return Err(ChunkIoError::IndexTreePageLimit);
        }
        if let Some(index) = existing_index {
            catalog[index].root_offset = root;
            catalog[index].item_count = item_count;
            catalog[index].level = level;
        } else {
            catalog.push(IndexCatalogEntry {
                kind_id,
                flags: 0,
                root_offset: root,
                item_count,
                level,
            });
        }
    }
    if let Some(manifest) = manifest_page {
        let manifest_len = read_u16_order(&manifest[14..16], order)? as u64;
        let manifest_offset = append_index_page_to_region(
            writer,
            cow_context.hasher,
            &manifest,
            cow_context.regions,
            cow_context.page_count,
        )?;
        catalog.retain(|entry| entry.kind_id != LogicalIndexKind::ScientificManifest as u16);
        catalog.push(IndexCatalogEntry {
            kind_id: LogicalIndexKind::ScientificManifest as u16,
            flags: 1,
            root_offset: manifest_offset,
            item_count: manifest_len,
            level: 0,
        });
    }
    catalog.sort_by_key(|entry| entry.kind_id);
    let catalog_page = encode_index_catalog_page(&catalog, commit.generation, order)?;
    let catalog_offset = append_index_page_to_region(
        writer,
        cow_context.hasher,
        &catalog_page,
        cow_context.regions,
        cow_context.page_count,
    )?;
    let footer_offset = writer.stream_position()?;
    if catalog_offset.checked_add(INDEX_PAGE_SIZE as u64) != Some(footer_offset) {
        return Err(ChunkIoError::IndexCatalogInvalid);
    }
    let valid_end = footer_offset
        .checked_add(GENERATION_FOOTER_LEN)
        .ok_or(ChunkIoError::LengthOverflow)?;
    writer.flush()?;
    before_footer_sync(writer)?;
    let footer = GenerationFooter {
        generation: commit.generation,
        prev_footer_offset: commit.prev_footer_offset,
        directory_offset,
        index_catalog_offset: catalog_offset,
        digest: *cow_context.hasher.finalize().as_bytes(),
    };
    writer.write_all(&encode_generation_footer(footer, order))?;
    writer.flush()?;
    after_footer_sync(writer)?;
    Ok(valid_end)
}

/// Appends a new generation using a generic stream and automatically updates the chunk-ID index.
/// Secondary-index mappings are added with `append_generation_with_indexes`.
pub fn append_generation<W: std::io::Read + std::io::Write + std::io::Seek>(
    writer: &mut W,
    chunks: &[EncodedChunkInput<'_>],
) -> Result<u64, ChunkIoError> {
    append_generation_with_indexes(writer, chunks, &[])
}

/// Appends chunks and caller-supplied index mappings using path-copy insertion.
pub fn append_generation_with_indexes<W: std::io::Read + std::io::Write + std::io::Seek>(
    writer: &mut W,
    chunks: &[EncodedChunkInput<'_>],
    index_entries: &[PersistentIndexEntry],
) -> Result<u64, ChunkIoError> {
    append_generation_with_indexes_hooks(writer, chunks, index_entries, |_| Ok(()), |_| Ok(()))
}

fn append_generation_with_indexes_hooks<W, F1, F2>(
    writer: &mut W,
    chunks: &[EncodedChunkInput<'_>],
    index_entries: &[PersistentIndexEntry],
    mut before_footer_sync: F1,
    mut after_footer_sync: F2,
) -> Result<u64, ChunkIoError>
where
    W: std::io::Read + std::io::Write + std::io::Seek,
    F1: FnMut(&mut W) -> std::io::Result<()>,
    F2: FnMut(&mut W) -> std::io::Result<()>,
{
    use std::io::SeekFrom;
    let snapshot = open_generation_snapshot(writer)?;
    if !is_index_generation_minor(snapshot.profile_minor) {
        return Err(ChunkIoError::GenerationProfileCannotAppend);
    }
    if snapshot.metadata.scientific_model().is_some() {
        return Err(ChunkIoError::ScientificModelCannotAppend);
    }
    let file_len = writer.seek(SeekFrom::End(0))?;
    if file_len != snapshot.valid_end {
        return Err(ChunkIoError::UnrepairedTail);
    }
    let total_chunks = snapshot
        .descriptors
        .len()
        .checked_add(chunks.len())
        .ok_or(ChunkIoError::TooManyChunks)?;
    if total_chunks as u64 > MAX_CHUNKS
        || chunks.len() as u64 > MAX_CHUNKS
        || index_entries.len() as u64 > MAX_INDEX_TREE_ENTRIES
    {
        return Err(ChunkIoError::TooManyChunks);
    }
    let mut ids: std::collections::BTreeSet<u64> = snapshot
        .descriptors
        .iter()
        .map(|descriptor| descriptor.id)
        .collect();
    for chunk in chunks {
        if !ids.insert(chunk.id) {
            return Err(ChunkIoError::DirectoryDuplicateChunkId);
        }
        if chunk.data.len() as u64 > MAX_CHUNK_BYTES {
            return Err(ChunkIoError::ChunkTooLarge);
        }
    }
    for entry in index_entries {
        if entry.kind == LogicalIndexKind::ChunkId {
            return Err(ChunkIoError::IndexDuplicateEntry);
        }
        if !ids.contains(&entry.chunk_id) {
            return Err(ChunkIoError::IndexChunkNotFound(entry.chunk_id));
        }
    }
    let new_generation = snapshot
        .generation
        .checked_add(1)
        .ok_or(ChunkIoError::GenerationChainInvalid)?;
    writer.seek(SeekFrom::Start(file_len))?;
    write_generation_records_append(
        writer,
        AppendGenerationRecords {
            chunks,
            index_entries,
            existing_descriptors: snapshot.descriptors(),
            prior_catalog: &snapshot.index_catalog,
            prior_regions: &snapshot.index_page_regions,
            order: snapshot.byte_order,
            commit: GenerationCommitInfo {
                generation: new_generation,
                prev_footer_offset: snapshot.footer_offset,
                generation_start: file_len,
            },
            manifest_page: None,
        },
        &mut before_footer_sync,
        &mut after_footer_sync,
    )?;
    Ok(new_generation)
}

/// Appends a new generation to a scientific dataset, updating its metadata/model manifest.
pub fn append_scientific_generation<W: std::io::Read + std::io::Write + std::io::Seek>(
    writer: &mut W,
    chunks: &[EncodedChunkInput<'_>],
    updated_metadata: &Metadata,
) -> Result<u64, ChunkIoError> {
    append_scientific_generation_with_indexes(writer, chunks, &[], updated_metadata)
}

/// Appends chunks and caller-supplied index mappings to a scientific dataset, updating its manifest.
pub fn append_scientific_generation_with_indexes<
    W: std::io::Read + std::io::Write + std::io::Seek,
>(
    writer: &mut W,
    chunks: &[EncodedChunkInput<'_>],
    index_entries: &[PersistentIndexEntry],
    updated_metadata: &Metadata,
) -> Result<u64, ChunkIoError> {
    append_scientific_generation_with_indexes_hooks(
        writer,
        chunks,
        index_entries,
        updated_metadata,
        |_| Ok(()),
        |_| Ok(()),
    )
}

fn append_scientific_generation_with_indexes_hooks<W, F1, F2>(
    writer: &mut W,
    chunks: &[EncodedChunkInput<'_>],
    index_entries: &[PersistentIndexEntry],
    updated_metadata: &Metadata,
    mut before_footer_sync: F1,
    mut after_footer_sync: F2,
) -> Result<u64, ChunkIoError>
where
    W: std::io::Read + std::io::Write + std::io::Seek,
    F1: FnMut(&mut W) -> std::io::Result<()>,
    F2: FnMut(&mut W) -> std::io::Result<()>,
{
    use std::io::SeekFrom;
    let snapshot = open_generation_snapshot(writer)?;
    if !is_index_generation_minor(snapshot.profile_minor) {
        return Err(ChunkIoError::GenerationProfileCannotAppend);
    }
    if snapshot.metadata.scientific_model().is_none() {
        return Err(ChunkIoError::ScientificModel(
            ScientificModelError::MissingScientificModel,
        ));
    }
    let new_model = updated_metadata
        .scientific_model()
        .ok_or(ChunkIoError::ScientificModel(
            ScientificModelError::MissingScientificModel,
        ))?;
    new_model
        .validate()
        .map_err(ChunkIoError::ScientificModel)?;

    let file_len = writer.seek(SeekFrom::End(0))?;
    if file_len != snapshot.valid_end {
        return Err(ChunkIoError::UnrepairedTail);
    }
    let total_chunks = snapshot
        .descriptors
        .len()
        .checked_add(chunks.len())
        .ok_or(ChunkIoError::TooManyChunks)?;
    if total_chunks as u64 > MAX_CHUNKS
        || chunks.len() as u64 > MAX_CHUNKS
        || index_entries.len() as u64 > MAX_INDEX_TREE_ENTRIES
    {
        return Err(ChunkIoError::TooManyChunks);
    }
    let mut ids: std::collections::BTreeSet<u64> = snapshot
        .descriptors
        .iter()
        .map(|descriptor| descriptor.id)
        .collect();
    for chunk in chunks {
        if !ids.insert(chunk.id) {
            return Err(ChunkIoError::DirectoryDuplicateChunkId);
        }
        if chunk.data.len() as u64 > MAX_CHUNK_BYTES {
            return Err(ChunkIoError::ChunkTooLarge);
        }
    }
    for entry in index_entries {
        if entry.kind == LogicalIndexKind::ChunkId {
            return Err(ChunkIoError::IndexDuplicateEntry);
        }
        if !ids.contains(&entry.chunk_id) {
            return Err(ChunkIoError::IndexChunkNotFound(entry.chunk_id));
        }
    }

    let encoded_manifest = updated_metadata.to_cbor().map_err(ChunkIoError::Metadata)?;
    if encoded_manifest.len() > INDEX_PAGE_SIZE - INDEX_PAGE_HEADER_LEN {
        return Err(ChunkIoError::Metadata(MetadataError::TooLarge));
    }

    let new_generation = snapshot
        .generation
        .checked_add(1)
        .ok_or(ChunkIoError::GenerationChainInvalid)?;
    let manifest_page =
        encode_index_manifest_page(&encoded_manifest, new_generation, snapshot.byte_order)?;

    writer.seek(SeekFrom::Start(file_len))?;
    write_generation_records_append(
        writer,
        AppendGenerationRecords {
            chunks,
            index_entries,
            existing_descriptors: snapshot.descriptors(),
            prior_catalog: &snapshot.index_catalog,
            prior_regions: &snapshot.index_page_regions,
            order: snapshot.byte_order,
            commit: GenerationCommitInfo {
                generation: new_generation,
                prev_footer_offset: snapshot.footer_offset,
                generation_start: file_len,
            },
            manifest_page: Some(manifest_page),
        },
        &mut before_footer_sync,
        &mut after_footer_sync,
    )?;
    Ok(new_generation)
}

pub(crate) fn append_staged_scientific_generation_with_indexes_hooks<W, F1, F2>(
    writer: &mut W,
    staged_chunks: &[StagedChunk],
    index_entries: &[PersistentIndexEntry],
    updated_metadata: &Metadata,
    mut before_footer_sync: F1,
    mut after_footer_sync: F2,
) -> Result<u64, ChunkIoError>
where
    W: std::io::Read + std::io::Write + std::io::Seek,
    F1: FnMut(&mut W) -> std::io::Result<()>,
    F2: FnMut(&mut W) -> std::io::Result<()>,
{
    use std::io::SeekFrom;
    // Use lazy snapshot open to avoid re-reading all historical chunk payloads on every append step (O(1) vs O(t)).
    let snapshot = open_generation_snapshot_lazy(writer)?;
    if !is_index_generation_minor(snapshot.profile_minor) {
        return Err(ChunkIoError::GenerationProfileCannotAppend);
    }
    if snapshot.metadata.scientific_model().is_none() {
        return Err(ChunkIoError::ScientificModel(
            ScientificModelError::MissingScientificModel,
        ));
    }
    let new_model = updated_metadata
        .scientific_model()
        .ok_or(ChunkIoError::ScientificModel(
            ScientificModelError::MissingScientificModel,
        ))?;
    new_model
        .validate()
        .map_err(ChunkIoError::ScientificModel)?;

    let file_len = writer.seek(SeekFrom::End(0))?;
    if file_len != snapshot.valid_end {
        return Err(ChunkIoError::UnrepairedTail);
    }
    let total_chunks = snapshot
        .descriptors
        .len()
        .checked_add(staged_chunks.len())
        .ok_or(ChunkIoError::TooManyChunks)?;
    if total_chunks as u64 > MAX_CHUNKS
        || staged_chunks.len() as u64 > MAX_CHUNKS
        || index_entries.len() as u64 > MAX_INDEX_TREE_ENTRIES
    {
        return Err(ChunkIoError::TooManyChunks);
    }
    let mut ids: std::collections::BTreeSet<u64> = snapshot
        .descriptors
        .iter()
        .map(|descriptor| descriptor.id)
        .collect();
    let mut combined_descriptors = snapshot.descriptors.clone();
    combined_descriptors
        .try_reserve(staged_chunks.len())
        .map_err(|_| ChunkIoError::TooManyChunks)?;
    for chunk in staged_chunks {
        if !ids.insert(chunk.id) {
            return Err(ChunkIoError::DirectoryDuplicateChunkId);
        }
        if chunk.raw_len > MAX_CHUNK_BYTES
            || chunk.stored_bytes.len() as u64 > MAX_STORED_CHUNK_BYTES
        {
            return Err(ChunkIoError::ChunkTooLarge);
        }
        combined_descriptors.push(chunk.descriptor_at(0));
    }
    new_model
        .validate_descriptors(&combined_descriptors)
        .map_err(ChunkIoError::ScientificModel)?;

    for entry in index_entries {
        if entry.kind == LogicalIndexKind::ChunkId {
            return Err(ChunkIoError::IndexDuplicateEntry);
        }
        if !ids.contains(&entry.chunk_id) {
            return Err(ChunkIoError::IndexChunkNotFound(entry.chunk_id));
        }
    }

    let encoded_manifest = updated_metadata.to_cbor().map_err(ChunkIoError::Metadata)?;
    if encoded_manifest.len() > INDEX_PAGE_SIZE - INDEX_PAGE_HEADER_LEN {
        return Err(ChunkIoError::Metadata(MetadataError::TooLarge));
    }

    let new_generation = snapshot
        .generation
        .checked_add(1)
        .ok_or(ChunkIoError::GenerationChainInvalid)?;
    let manifest_page =
        encode_index_manifest_page(&encoded_manifest, new_generation, snapshot.byte_order)?;

    writer.seek(SeekFrom::Start(file_len))?;
    let mut hasher = blake3::Hasher::new();
    let (new_descriptors, directory_offset, directory_end) =
        write_staged_chunks_and_directory(writer, staged_chunks, snapshot.byte_order, &mut hasher)?;
    let mut regions = snapshot.index_page_regions.clone();
    regions
        .try_reserve(1)
        .map_err(|_| ChunkIoError::IndexTreePageLimit)?;
    regions.push(IndexPageRegion {
        start: directory_end,
        end: directory_end,
        generation: new_generation,
    });
    let mut page_count = 0u64;
    let mut cow_context = BTreeCowWriteContext {
        generation: new_generation,
        order: snapshot.byte_order,
        regions: &mut regions,
        hasher: &mut hasher,
        page_count: &mut page_count,
    };
    let mut catalog = snapshot.index_catalog.clone();
    let all_ids: std::collections::BTreeSet<u64> = snapshot
        .descriptors
        .iter()
        .chain(new_descriptors.iter())
        .map(|descriptor| descriptor.id)
        .collect();

    let chunk_kind = LogicalIndexKind::ChunkId as u16;
    let chunk_root_index = catalog
        .iter()
        .position(|entry| entry.kind_id == chunk_kind)
        .ok_or(ChunkIoError::IndexCatalogInvalid)?;
    for descriptor in &new_descriptors {
        let entry = IndexCompositeKey {
            key: PersistentIndexEntry::key_u64(descriptor.id),
            chunk_id: descriptor.id,
        };
        let root = catalog[chunk_root_index].root_offset;
        let (new_root, new_level) = insert_btree_cow(writer, root, entry, &mut cow_context)?;
        catalog[chunk_root_index].root_offset = new_root;
        catalog[chunk_root_index].item_count = catalog[chunk_root_index]
            .item_count
            .checked_add(1)
            .ok_or(ChunkIoError::IndexTreePageLimit)?;
        catalog[chunk_root_index].level = new_level;
    }

    let mut updates = std::collections::BTreeMap::<u16, Vec<IndexCompositeKey>>::new();
    for entry in index_entries {
        if !all_ids.contains(&entry.chunk_id) {
            return Err(ChunkIoError::IndexChunkNotFound(entry.chunk_id));
        }
        updates
            .entry(entry.kind as u16)
            .or_default()
            .push(IndexCompositeKey {
                key: entry.key,
                chunk_id: entry.chunk_id,
            });
    }
    let new_kind_count = updates
        .keys()
        .filter(|kind_id| !catalog.iter().any(|entry| entry.kind_id == **kind_id))
        .count();
    if new_kind_count + catalog.len() > MAX_INDEX_CATALOG_ENTRIES {
        return Err(ChunkIoError::IndexCatalogInvalid);
    }
    for (kind_id, mut records) in updates {
        records.sort_unstable();
        if records.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ChunkIoError::IndexDuplicateEntry);
        }
        let existing_index = catalog.iter().position(|entry| entry.kind_id == kind_id);
        let (mut root, mut item_count, mut level) = if let Some(index) = existing_index {
            let info = catalog[index];
            (info.root_offset, info.item_count, info.level)
        } else {
            (0, 0, 0)
        };
        for record in records {
            let (new_root, new_level) = insert_btree_cow(writer, root, record, &mut cow_context)?;
            root = new_root;
            item_count = item_count
                .checked_add(1)
                .ok_or(ChunkIoError::IndexTreePageLimit)?;
            level = new_level;
        }
        if item_count > MAX_INDEX_TREE_ENTRIES {
            return Err(ChunkIoError::IndexTreePageLimit);
        }
        if let Some(index) = existing_index {
            catalog[index].root_offset = root;
            catalog[index].item_count = item_count;
            catalog[index].level = level;
        } else {
            catalog.push(IndexCatalogEntry {
                kind_id,
                flags: 0,
                root_offset: root,
                item_count,
                level,
            });
        }
    }

    let manifest_len = read_u16_order(&manifest_page[14..16], snapshot.byte_order)? as u64;
    let manifest_offset = append_index_page_to_region(
        writer,
        cow_context.hasher,
        &manifest_page,
        cow_context.regions,
        cow_context.page_count,
    )?;
    catalog.retain(|entry| entry.kind_id != LogicalIndexKind::ScientificManifest as u16);
    catalog.push(IndexCatalogEntry {
        kind_id: LogicalIndexKind::ScientificManifest as u16,
        flags: 1,
        root_offset: manifest_offset,
        item_count: manifest_len,
        level: 0,
    });
    catalog.sort_by_key(|entry| entry.kind_id);
    let catalog_page = encode_index_catalog_page(&catalog, new_generation, snapshot.byte_order)?;
    let catalog_offset = append_index_page_to_region(
        writer,
        cow_context.hasher,
        &catalog_page,
        cow_context.regions,
        cow_context.page_count,
    )?;
    let footer_offset = writer.stream_position()?;
    if catalog_offset.checked_add(INDEX_PAGE_SIZE as u64) != Some(footer_offset) {
        return Err(ChunkIoError::IndexCatalogInvalid);
    }
    writer.flush()?;
    before_footer_sync(writer)?;
    let footer = GenerationFooter {
        generation: new_generation,
        prev_footer_offset: snapshot.footer_offset,
        directory_offset,
        index_catalog_offset: catalog_offset,
        digest: *hasher.finalize().as_bytes(),
    };
    writer.write_all(&encode_generation_footer(footer, snapshot.byte_order))?;
    writer.flush()?;
    after_footer_sync(writer)?;
    Ok(new_generation)
}

/// Creates and durably publishes a generation-profile container via a sibling temporary file.
pub fn write_generation_container_atomic(
    path: impl AsRef<std::path::Path>,
    metadata: &Metadata,
    chunks: &[EncodedChunkInput<'_>],
    order: HeaderByteOrder,
) -> Result<(), ChunkIoError> {
    write_generation_container_atomic_with_indexes(path, metadata, chunks, &[], order)
}

/// Creates and durably publishes a new generation container with persistent secondary indexes.
pub fn write_generation_container_atomic_with_indexes(
    path: impl AsRef<std::path::Path>,
    metadata: &Metadata,
    chunks: &[EncodedChunkInput<'_>],
    index_entries: &[PersistentIndexEntry],
    order: HeaderByteOrder,
) -> Result<(), ChunkIoError> {
    use std::io::Write;
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let name = path
        .file_name()
        .ok_or(ChunkIoError::DirectoryOffsetInvalid)?;
    let mut temp_name = name.to_os_string();
    temp_name.push(format!(".wx1-generation-tmp-{}", std::process::id()));
    let temp_path = parent.join(temp_name);
    let mut temp_created = false;
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        temp_created = true;
        write_generation_container_with_indexes_hooks(
            &mut file,
            metadata,
            chunks,
            index_entries,
            order,
            |file| {
                file.flush()?;
                file.sync_data()
            },
            |file| {
                file.flush()?;
                file.sync_all()
            },
        )?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp_path, path)?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok::<(), ChunkIoError>(())
    })();
    if result.is_err() && temp_created {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

/// Creates and durably publishes a generation-profile container from chunk readers.
pub fn write_generation_container_from_readers_atomic(
    path: impl AsRef<std::path::Path>,
    metadata: &Metadata,
    chunks: &mut [ChunkReaderInput<'_>],
    order: HeaderByteOrder,
    options: ChunkWriteOptions,
) -> Result<(), ChunkIoError> {
    write_generation_container_from_readers_atomic_with_indexes(
        path,
        metadata,
        chunks,
        &[],
        order,
        options,
    )
}

/// Atomic reader-based generation writer with persistent secondary-index mappings.
pub fn write_generation_container_from_readers_atomic_with_indexes(
    path: impl AsRef<std::path::Path>,
    metadata: &Metadata,
    chunks: &mut [ChunkReaderInput<'_>],
    index_entries: &[PersistentIndexEntry],
    order: HeaderByteOrder,
    options: ChunkWriteOptions,
) -> Result<(), ChunkIoError> {
    use std::io::Write;
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let name = path
        .file_name()
        .ok_or(ChunkIoError::DirectoryOffsetInvalid)?;
    let mut temp_name = name.to_os_string();
    temp_name.push(format!(".wx1-generation-stream-tmp-{}", std::process::id()));
    let temp_path = parent.join(temp_name);
    let mut temp_created = false;
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        temp_created = true;
        write_generation_container_from_readers_with_indexes_hooks(
            &mut file,
            ReaderGenerationWriteInput {
                metadata,
                chunks,
                index_entries,
                order,
                options,
            },
            |file| {
                file.flush()?;
                file.sync_data()
            },
            |file| {
                file.flush()?;
                file.sync_all()
            },
        )?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp_path, path)?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok::<(), ChunkIoError>(())
    })();
    if result.is_err() && temp_created {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

/// Appends a generation to a file, syncing payload/directory/index pages before the footer.
/// This function takes no cross-process lock; callers must serialize append and repair.
pub fn append_generation_file(
    path: impl AsRef<std::path::Path>,
    chunks: &[EncodedChunkInput<'_>],
) -> Result<u64, ChunkIoError> {
    append_generation_file_with_indexes(path, chunks, &[])
}

/// Appends chunks and persistent index mappings to a file with data/footer sync ordering.
pub fn append_generation_file_with_indexes(
    path: impl AsRef<std::path::Path>,
    chunks: &[EncodedChunkInput<'_>],
    index_entries: &[PersistentIndexEntry],
) -> Result<u64, ChunkIoError> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    append_generation_with_indexes_hooks(
        &mut file,
        chunks,
        index_entries,
        |file| {
            file.flush()?;
            file.sync_data()
        },
        |file| {
            file.flush()?;
            file.sync_all()
        },
    )
}

/// Appends a generation to a scientific dataset file, syncing payload/directory/index pages before the footer.
/// This function takes no cross-process lock; callers must serialize append and repair.
pub fn append_scientific_generation_file(
    path: impl AsRef<std::path::Path>,
    chunks: &[EncodedChunkInput<'_>],
    updated_metadata: &Metadata,
) -> Result<u64, ChunkIoError> {
    append_scientific_generation_file_with_indexes(path, chunks, &[], updated_metadata)
}

/// Appends chunks and persistent index mappings to a scientific dataset file with data/footer sync ordering.
pub fn append_scientific_generation_file_with_indexes(
    path: impl AsRef<std::path::Path>,
    chunks: &[EncodedChunkInput<'_>],
    index_entries: &[PersistentIndexEntry],
    updated_metadata: &Metadata,
) -> Result<u64, ChunkIoError> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    append_scientific_generation_with_indexes_hooks(
        &mut file,
        chunks,
        index_entries,
        updated_metadata,
        |file| {
            file.flush()?;
            file.sync_data()
        },
        |file| {
            file.flush()?;
            file.sync_all()
        },
    )
}

/// Explicitly truncates only bytes after the newest fully valid generation. If validation
/// fails, no file mutation occurs. Returns the original and retained lengths and generation.
/// Callers must ensure no concurrent writer or repair is operating on the same file.
pub fn repair_generation_file(
    path: impl AsRef<std::path::Path>,
) -> Result<RepairReport, ChunkIoError> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    let original_len = file.metadata()?.len();
    let snapshot = open_generation_snapshot(&mut file)?;
    let retained_len = snapshot.valid_end;
    if retained_len < original_len {
        file.set_len(retained_len)?;
        file.sync_all()?;
    }
    Ok(RepairReport {
        generation: snapshot.generation,
        original_len,
        retained_len,
    })
}

#[cfg(test)]
mod v038_generation_tests {
    use super::*;
    use std::io::{Cursor, Read, Seek, SeekFrom, Write};

    fn initial_generation(order: HeaderByteOrder) -> Cursor<Vec<u8>> {
        let metadata = Metadata::new("generation-test");
        let chunks = [
            EncodedChunkInput {
                id: 101,
                data: b"stored-alpha",
                codec: ChunkCodec::Raw,
            },
            EncodedChunkInput {
                id: 102,
                data: b"compressed-beta-beta-beta",
                codec: ChunkCodec::Zstandard,
            },
        ];
        let mut file = Cursor::new(Vec::new());
        write_generation_container(&mut file, &metadata, &chunks, order).unwrap();
        file
    }

    fn appended_chunks() -> [EncodedChunkInput<'static>; 1] {
        [EncodedChunkInput {
            id: 103,
            data: b"generation-two-payload",
            codec: ChunkCodec::Lz4,
        }]
    }

    pub(super) fn legacy_v038_generation() -> Cursor<Vec<u8>> {
        let metadata = Metadata::new("v038-compat");
        let metadata_bytes = metadata.to_cbor().unwrap();
        let order = HeaderByteOrder::LittleEndian;
        let payload = b"v038-compat";
        let generation_start = CONTAINER_HEADER_LEN as u64 + metadata_bytes.len() as u64;
        let mut file = Cursor::new(Vec::new());
        let header = ContainerHeader {
            major: 1,
            minor: 4,
            byte_order: order,
            flags: 0,
            metadata_len: metadata_bytes.len() as u64,
            directory_offset: 0,
        };
        file.write_all(&header.encode()).unwrap();
        file.write_all(&metadata_bytes).unwrap();
        let payload_offset = file.stream_position().unwrap();
        file.write_all(payload).unwrap();
        let descriptor = ChunkDescriptor {
            id: 900,
            offset: payload_offset,
            stored_len: payload.len() as u64,
            raw_len: payload.len() as u64,
            crc32c: crc32c(payload),
            codec: ChunkCodec::Raw,
            blake3: Some(*blake3::hash(payload).as_bytes()),
        };
        let directory_offset = file.stream_position().unwrap();
        file.write_all(&encode_generation_directory(&[descriptor], order).unwrap())
            .unwrap();
        let footer_offset = file.stream_position().unwrap();
        let digest = hash_file_range(
            &mut file,
            generation_start,
            footer_offset - generation_start,
        )
        .unwrap();
        let footer = encode_generation_footer_v038(
            GenerationFooter {
                generation: 1,
                prev_footer_offset: 0,
                directory_offset,
                index_catalog_offset: 0,
                digest,
            },
            order,
        );
        file.write_all(&footer).unwrap();
        file
    }

    #[test]
    fn generation_v2_roundtrips_little_and_big_endian() {
        for order in [HeaderByteOrder::LittleEndian, HeaderByteOrder::BigEndian] {
            let mut file = initial_generation(order);
            let snapshot = open_generation_snapshot(&mut file).unwrap();
            assert_eq!(snapshot.generation(), 1);
            assert_eq!(snapshot.descriptors().len(), 2);
            assert_eq!(
                snapshot.read_chunk(&mut file, 101).unwrap(),
                b"stored-alpha"
            );
            assert_eq!(
                snapshot.read_chunk(&mut file, 102).unwrap(),
                b"compressed-beta-beta-beta"
            );

            assert_eq!(append_generation(&mut file, &appended_chunks()).unwrap(), 2);
            let current = open_generation_snapshot(&mut file).unwrap();
            assert_eq!(current.generation(), 2);
            assert_eq!(current.descriptors().len(), 3);
            assert_eq!(
                current.read_chunk(&mut file, 103).unwrap(),
                b"generation-two-payload"
            );
        }
    }

    #[test]
    fn generation_directory_v2_persists_hash_of_exact_stored_bytes() {
        let mut file = initial_generation(HeaderByteOrder::LittleEndian);
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        let descriptor = snapshot.chunk(102).unwrap();
        let expected =
            hash_stored_chunk_blake3(&mut file, descriptor, ChunkReadOptions::default()).unwrap();
        assert_eq!(descriptor.blake3, Some(expected));
        file.seek(SeekFrom::Start(snapshot.directory_offset()))
            .unwrap();
        let mut directory_header = [0u8; 16];
        file.read_exact(&mut directory_header).unwrap();
        assert_eq!(
            u16::from_le_bytes([directory_header[4], directory_header[5]]),
            2
        );
        assert_eq!(
            u16::from_le_bytes([directory_header[6], directory_header[7]]),
            72
        );
    }

    #[test]
    fn generation_profile_keeps_root_pointer_out_of_mutable_header() {
        let mut file = initial_generation(HeaderByteOrder::LittleEndian);
        let header = ContainerHeader::decode(&file.get_ref()[..CONTAINER_HEADER_LEN]).unwrap();
        assert_eq!(header.minor, GENERATION_PROFILE_MINOR_VERSION);
        assert_eq!(header.directory_offset, 0);
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        let footer = snapshot.footer_offset() as usize;
        let footer_root =
            u64::from_le_bytes(file.get_ref()[footer + 24..footer + 32].try_into().unwrap());
        assert_eq!(snapshot.directory_offset(), footer_root);
    }

    #[test]
    fn footer_digest_covers_only_its_generation_payload_and_directory() {
        let mut file = initial_generation(HeaderByteOrder::LittleEndian);
        let header = ContainerHeader::decode(&file.get_ref()[..CONTAINER_HEADER_LEN]).unwrap();
        let generation_one_start = CONTAINER_HEADER_LEN as u64 + header.metadata_len;
        let first = open_generation_snapshot(&mut file).unwrap();
        let first_footer = first.footer_offset() as usize;
        let first_expected =
            blake3::hash(&file.get_ref()[generation_one_start as usize..first_footer]);
        // v0.3.9 footer v2 stores the index-catalog offset before the generation digest.
        assert_eq!(
            &file.get_ref()[first_footer + 40..first_footer + 72],
            first_expected.as_bytes()
        );

        append_generation(&mut file, &appended_chunks()).unwrap();
        let second = open_generation_snapshot(&mut file).unwrap();
        let second_footer = second.footer_offset() as usize;
        let second_start = first.valid_end() as usize;
        let second_expected = blake3::hash(&file.get_ref()[second_start..second_footer]);
        assert_eq!(
            &file.get_ref()[second_footer + 40..second_footer + 72],
            second_expected.as_bytes()
        );
    }

    #[test]
    fn generation_descriptors_flow_through_legacy_directory_api_without_losing_hash() {
        let mut file = initial_generation(HeaderByteOrder::LittleEndian);
        let (_, descriptors) = read_chunk_directory(&mut file).unwrap();
        let descriptor = descriptors
            .iter()
            .find(|item| item.id == 102)
            .copied()
            .unwrap();
        assert!(descriptor.blake3.is_some());
        assert_eq!(
            read_chunk_payload(&mut file, descriptor).unwrap(),
            b"compressed-beta-beta-beta"
        );
        let mut output = Vec::new();
        let copied = copy_chunk_payload_to_writer(
            &mut file,
            descriptor,
            &mut output,
            ChunkReadOptions::default(),
        )
        .unwrap();
        assert_eq!(copied, 25);
        assert_eq!(output, b"compressed-beta-beta-beta");
    }

    #[test]
    fn v1_directory_writer_output_remains_readable() {
        let chunks = [ChunkInput {
            id: 7,
            data: b"legacy-v1-payload",
        }];
        let mut file = Cursor::new(Vec::new());
        write_chunk_container(
            &mut file,
            &Metadata::new("legacy-v1"),
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let (metadata, descriptors) = read_chunk_directory(&mut file).unwrap();
        assert_eq!(metadata.dataset_id, "legacy-v1");
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].blake3, None);
        assert_eq!(read_chunk(&mut file, 7).unwrap(), b"legacy-v1-payload");
    }

    #[test]
    fn multiple_generations_keep_old_snapshot_pinned() {
        let mut file = initial_generation(HeaderByteOrder::LittleEndian);
        let old_snapshot = open_generation_snapshot(&mut file).unwrap();
        let old_end = old_snapshot.valid_end();
        append_generation(&mut file, &appended_chunks()).unwrap();
        let newer = [EncodedChunkInput {
            id: 104,
            data: b"third-generation",
            codec: ChunkCodec::Raw,
        }];
        append_generation(&mut file, &newer).unwrap();

        assert_eq!(old_snapshot.generation(), 1);
        assert_eq!(old_snapshot.valid_end(), old_end);
        assert_eq!(old_snapshot.descriptors().len(), 2);
        assert_eq!(
            old_snapshot.read_chunk(&mut file, 101).unwrap(),
            b"stored-alpha"
        );
        assert!(old_snapshot.chunk(103).is_none());

        let current = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(current.generation(), 3);
        assert_eq!(current.descriptors().len(), 4);
    }

    #[test]
    fn truncated_or_failed_append_recovers_previous_generation_without_mutation() {
        let mut file = initial_generation(HeaderByteOrder::LittleEndian);
        append_generation(&mut file, &appended_chunks()).unwrap();
        let committed_len = file.get_ref().len();
        file.get_mut().truncate(committed_len - 11);
        let before_open = file.get_ref().len();
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(snapshot.generation(), 1);
        assert_eq!(
            file.get_ref().len(),
            before_open,
            "opening must never truncate the tail"
        );
        assert!(matches!(
            append_generation(&mut file, &appended_chunks()),
            Err(ChunkIoError::UnrepairedTail)
        ));
        assert_eq!(
            file.get_ref().len(),
            before_open,
            "failed append must not rewrite the tail"
        );
    }

    struct FailAfter {
        inner: Cursor<Vec<u8>>,
        remaining: usize,
    }

    impl Read for FailAfter {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.inner.read(buf)
        }
    }
    impl Seek for FailAfter {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(pos)
        }
    }
    impl Write for FailAfter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::Error::other("injected writer failure"));
            }
            let allowed = buf.len().min(self.remaining);
            let written = self.inner.write(&buf[..allowed])?;
            self.remaining -= written;
            Ok(written)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.inner.flush()
        }
    }

    #[test]
    fn every_append_byte_failure_preserves_the_previous_valid_generation() {
        let base = initial_generation(HeaderByteOrder::LittleEndian).into_inner();
        let mut successful = Cursor::new(base.clone());
        append_generation(&mut successful, &appended_chunks()).unwrap();
        let append_bytes = successful.get_ref().len() - base.len();

        for fail_after in 0..append_bytes {
            let mut writer = FailAfter {
                inner: Cursor::new(base.clone()),
                remaining: fail_after,
            };
            assert!(
                append_generation(&mut writer, &appended_chunks()).is_err(),
                "fail_after={fail_after}"
            );
            let mut recovered_file = Cursor::new(writer.inner.into_inner());
            let snapshot = open_generation_snapshot(&mut recovered_file).unwrap_or_else(|error| {
                panic!("previous generation not recoverable at byte {fail_after}: {error}")
            });
            assert_eq!(snapshot.generation(), 1, "fail_after={fail_after}");
            assert_eq!(
                snapshot.read_chunk(&mut recovered_file, 101).unwrap(),
                b"stored-alpha"
            );
            assert!(snapshot.chunk(103).is_none());
        }
    }

    #[test]
    fn chunk_crc_corruption_is_reported_as_crc_not_a_generic_directory_error() {
        let mut file = initial_generation(HeaderByteOrder::LittleEndian);
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        let descriptor = snapshot.chunk(101).unwrap();
        file.get_mut()[descriptor.offset as usize] ^= 1;
        assert!(matches!(
            open_generation_snapshot(&mut file),
            Err(ChunkIoError::ChecksumMismatch(101))
        ));
    }

    #[test]
    fn chunk_digest_corruption_is_reported_separately_from_crc() {
        let mut file = initial_generation(HeaderByteOrder::LittleEndian);
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        let directory_hash_offset = snapshot.directory_offset() as usize + 16 + 40;
        file.get_mut()[directory_hash_offset] ^= 1;
        assert!(matches!(
            open_generation_snapshot(&mut file),
            Err(ChunkIoError::ChunkHashMismatch(101))
        ));
    }

    #[test]
    fn directory_corruption_is_reported_as_a_directory_error() {
        let mut file = initial_generation(HeaderByteOrder::LittleEndian);
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        file.get_mut()[snapshot.directory_offset() as usize] ^= 1;
        assert!(matches!(
            open_generation_snapshot(&mut file),
            Err(ChunkIoError::DirectoryMagicMismatch)
        ));
    }

    #[test]
    fn footer_and_generation_hash_corruption_have_specific_errors() {
        let mut footer_corrupt = initial_generation(HeaderByteOrder::LittleEndian);
        let snapshot = open_generation_snapshot(&mut footer_corrupt).unwrap();
        footer_corrupt.get_mut()[snapshot.footer_offset() as usize] ^= 1;
        assert!(matches!(
            open_generation_snapshot(&mut footer_corrupt),
            Err(ChunkIoError::GenerationFooterInvalid)
        ));

        let mut hash_corrupt = initial_generation(HeaderByteOrder::LittleEndian);
        let snapshot = open_generation_snapshot(&mut hash_corrupt).unwrap();
        // Footer v2: bytes 32..40 are the catalog pointer; the digest begins at byte 40.
        hash_corrupt.get_mut()[snapshot.footer_offset() as usize + 40] ^= 1;
        assert!(matches!(
            open_generation_snapshot(&mut hash_corrupt),
            Err(ChunkIoError::GenerationHashMismatch)
        ));
    }

    #[test]
    fn v038_footer_digest_corruption_is_reported_as_generation_hash_mismatch() {
        let mut file = legacy_v038_generation();
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(
            snapshot.profile_minor,
            LEGACY_GENERATION_PROFILE_MINOR_VERSION
        );
        // Footer v1 has no index-catalog pointer; its digest begins at byte 32.
        file.get_mut()[snapshot.footer_offset() as usize + 32] ^= 1;
        assert!(matches!(
            open_generation_snapshot(&mut file),
            Err(ChunkIoError::GenerationHashMismatch)
        ));
    }

    #[test]
    fn invalid_generation_root_offset_has_a_specific_footer_error() {
        let mut file = initial_generation(HeaderByteOrder::LittleEndian);
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        let root_field = snapshot.footer_offset() as usize + 24;
        file.get_mut()[root_field..root_field + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(
            open_generation_snapshot(&mut file),
            Err(ChunkIoError::GenerationFooterInvalid)
        ));
    }

    #[test]
    fn invalid_generation_chain_has_a_specific_error() {
        let mut file = initial_generation(HeaderByteOrder::LittleEndian);
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        let previous_footer_field = snapshot.footer_offset() as usize + 16;
        file.get_mut()[previous_footer_field..previous_footer_field + 8]
            .copy_from_slice(&1_u64.to_le_bytes());
        assert!(matches!(
            open_generation_snapshot(&mut file),
            Err(ChunkIoError::GenerationChainInvalid)
        ));
    }

    #[test]
    fn invalid_newer_generation_falls_back_and_explicit_repair_truncates_only_its_tail() {
        use std::fs::OpenOptions;
        use std::io::{Seek, SeekFrom, Write};

        let path = std::env::temp_dir().join(format!("wx1-v038-repair-{}.wx1", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let metadata = Metadata::new("repair-test");
        let initial = [EncodedChunkInput {
            id: 201,
            data: b"generation-one",
            codec: ChunkCodec::Raw,
        }];
        write_generation_container_atomic(
            &path,
            &metadata,
            &initial,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let second = appended_chunks();
        append_generation_file(&path, &second).unwrap();

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(snapshot.generation(), 2);
        let appended = snapshot.chunk(103).unwrap();
        file.seek(SeekFrom::Start(appended.offset)).unwrap();
        file.write_all(b"X").unwrap();
        file.sync_all().unwrap();
        let damaged_len = file.metadata().unwrap().len();

        let recovered = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(recovered.generation(), 1);
        assert_eq!(
            file.metadata().unwrap().len(),
            damaged_len,
            "open must not mutate the file"
        );
        assert_eq!(
            recovered.read_chunk(&mut file, 201).unwrap(),
            b"generation-one"
        );

        let report = repair_generation_file(&path).unwrap();
        assert_eq!(report.generation, 1);
        assert_eq!(report.original_len, damaged_len);
        assert_eq!(report.retained_len, recovered.valid_end());
        assert_eq!(std::fs::metadata(&path).unwrap().len(), report.retained_len);
        let mut repaired = std::fs::File::open(&path).unwrap();
        assert_eq!(
            open_generation_snapshot(&mut repaired)
                .unwrap()
                .generation(),
            1
        );
        drop(repaired);
        drop(file);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn rejects_overflowing_chunk_offsets_and_duplicate_ids_across_generations() {
        let mut offset_corrupt = initial_generation(HeaderByteOrder::LittleEndian);
        let snapshot = open_generation_snapshot(&mut offset_corrupt).unwrap();
        let field = snapshot.directory_offset() as usize + 16 + 8;
        offset_corrupt.get_mut()[field..field + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(
            open_generation_snapshot(&mut offset_corrupt),
            Err(ChunkIoError::LengthOverflow | ChunkIoError::DirectoryOffsetInvalid)
        ));

        let mut duplicate = initial_generation(HeaderByteOrder::LittleEndian);
        let repeated = [EncodedChunkInput {
            id: 101,
            data: b"shadow",
            codec: ChunkCodec::Raw,
        }];
        assert!(matches!(
            append_generation(&mut duplicate, &repeated),
            Err(ChunkIoError::DirectoryDuplicateChunkId)
        ));
    }

    #[test]
    fn a_file_without_any_valid_generation_has_a_specific_error() {
        assert!(matches!(
            open_generation_snapshot(&mut Cursor::new(Vec::<u8>::new())),
            Err(ChunkIoError::NoValidGeneration)
        ));
    }
}

#[cfg(test)]
mod v039_persistent_btree_tests {
    use super::*;
    use std::io::{Cursor, Read, Seek, SeekFrom, Write};

    fn one_byte_chunks(first_id: u64, count: usize) -> Vec<EncodedChunkInput<'static>> {
        let payload: &'static [u8] = b"x";
        (0..count)
            .map(|index| EncodedChunkInput {
                id: first_id + index as u64,
                data: payload,
                codec: ChunkCodec::Raw,
            })
            .collect()
    }

    fn u32_key(value: u32) -> [u8; 32] {
        PersistentIndexEntry::key_u32(value)
    }

    fn u64_key(value: u64) -> [u8; 32] {
        PersistentIndexEntry::key_u64(value)
    }

    fn indexed_initial(order: HeaderByteOrder) -> Cursor<Vec<u8>> {
        let chunks = one_byte_chunks(100, 3);
        let entries = [
            PersistentIndexEntry::new(LogicalIndexKind::Variable, u32_key(7), 100),
            PersistentIndexEntry::new(LogicalIndexKind::Variable, u32_key(7), 101),
            PersistentIndexEntry::new(LogicalIndexKind::Time, u64_key(42), 100),
            PersistentIndexEntry::new(LogicalIndexKind::Vertical, u32_key(850), 100),
            PersistentIndexEntry::new(LogicalIndexKind::Spatial, u64_key(12_345), 102),
        ];
        let mut file = Cursor::new(Vec::new());
        write_generation_container_with_indexes(
            &mut file,
            &Metadata::new("persistent-index-test"),
            &chunks,
            &entries,
            order,
        )
        .unwrap();
        file
    }

    // A v0.3.9 fixture has the same footer-v2/index layout, but the historical
    // header minor is 5. The current writer defaults to the v0.4.0 minor 6.
    fn indexed_initial_v039(order: HeaderByteOrder) -> Cursor<Vec<u8>> {
        let mut file = indexed_initial(order);
        let mut header = ContainerHeader::decode(&file.get_ref()[..CONTAINER_HEADER_LEN]).unwrap();
        header.minor = PERSISTENT_INDEX_PROFILE_MINOR_VERSION;
        file.get_mut()[..CONTAINER_HEADER_LEN].copy_from_slice(&header.encode());
        file
    }

    #[test]
    fn indexed_writer_uses_v040_profile_minor_six() {
        let mut file = indexed_initial(HeaderByteOrder::LittleEndian);
        let header = ContainerHeader::decode(&file.get_ref()[..CONTAINER_HEADER_LEN]).unwrap();
        assert_eq!(header.minor, GENERATION_PROFILE_MINOR_VERSION);
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(snapshot.profile_minor, GENERATION_PROFILE_MINOR_VERSION);
    }

    #[test]
    fn v039_footer_v2_catalog_and_chunk_id_tree_are_persisted() {
        let mut file = indexed_initial_v039(HeaderByteOrder::LittleEndian);
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        let header = ContainerHeader::decode(&file.get_ref()[..CONTAINER_HEADER_LEN]).unwrap();

        assert_eq!(header.minor, PERSISTENT_INDEX_PROFILE_MINOR_VERSION);
        assert_eq!(header.directory_offset, 0);
        assert_eq!(snapshot.generation(), 1);
        let footer_offset = snapshot.footer_offset() as usize;
        assert_eq!(&file.get_ref()[footer_offset..footer_offset + 4], b"WX1G");
        assert_eq!(
            u16::from_le_bytes(
                file.get_ref()[footer_offset + 4..footer_offset + 6]
                    .try_into()
                    .unwrap()
            ),
            2
        );
        assert_eq!(
            u16::from_le_bytes(
                file.get_ref()[footer_offset + 6..footer_offset + 8]
                    .try_into()
                    .unwrap()
            ),
            80
        );
        let catalog_offset = u64::from_le_bytes(
            file.get_ref()[footer_offset + 32..footer_offset + 40]
                .try_into()
                .unwrap(),
        );
        assert_eq!(
            catalog_offset + INDEX_PAGE_SIZE as u64,
            snapshot.footer_offset()
        );
        assert_eq!(
            snapshot
                .index_info(LogicalIndexKind::ChunkId)
                .unwrap()
                .item_count,
            3
        );
        assert_eq!(
            snapshot
                .index_info(LogicalIndexKind::Variable)
                .unwrap()
                .item_count,
            2
        );
        assert_eq!(
            snapshot
                .lookup_index_exact(&mut file, LogicalIndexKind::ChunkId, &u64_key(101))
                .unwrap(),
            vec![101]
        );
    }

    #[test]
    fn persistent_indexes_roundtrip_both_byte_orders() {
        for order in [HeaderByteOrder::LittleEndian, HeaderByteOrder::BigEndian] {
            let mut file = indexed_initial(order);
            let snapshot = open_generation_snapshot(&mut file).unwrap();
            assert_eq!(
                snapshot
                    .lookup_index_exact(&mut file, LogicalIndexKind::Variable, &u32_key(7))
                    .unwrap(),
                vec![100, 101]
            );
            assert_eq!(
                snapshot
                    .lookup_index_exact(&mut file, LogicalIndexKind::Time, &u64_key(42))
                    .unwrap(),
                vec![100]
            );
            assert_eq!(
                snapshot
                    .lookup_index_exact(&mut file, LogicalIndexKind::Vertical, &u32_key(850))
                    .unwrap(),
                vec![100]
            );
            assert_eq!(
                snapshot
                    .lookup_index_exact(&mut file, LogicalIndexKind::Spatial, &u64_key(12_345))
                    .unwrap(),
                vec![102]
            );
            assert_eq!(
                snapshot
                    .lookup_index_range(
                        &mut file,
                        LogicalIndexKind::Variable,
                        &u32_key(7),
                        &u32_key(7)
                    )
                    .unwrap(),
                vec![100, 101]
            );
        }
    }

    #[test]
    fn scientific_chunk_key_generates_explicit_secondary_index_entries() {
        let key = ScientificChunkKey {
            variable_id: 130,
            time_index: 12,
            level_index: 4,
            spatial_tile: 99,
        };
        let entries = key.persistent_entries(777);
        assert_eq!(entries.len(), 4);
        assert!(entries.iter().all(|entry| entry.chunk_id == 777));
        assert_eq!(entries[0].kind, LogicalIndexKind::Variable);
        assert_eq!(entries[1].kind, LogicalIndexKind::Time);
        assert_eq!(entries[2].kind, LogicalIndexKind::Vertical);
        assert_eq!(entries[3].kind, LogicalIndexKind::Spatial);
        assert_eq!(entries[0].key, u32_key(130));
        assert_eq!(entries[1].key, u64_key(12));
        assert_eq!(entries[2].key, u32_key(4));
        assert_eq!(entries[3].key, u64_key(99));
    }

    #[test]
    fn copy_on_write_append_keeps_old_snapshot_and_updates_only_new_root() {
        let mut file = indexed_initial(HeaderByteOrder::LittleEndian);
        let old = open_generation_snapshot(&mut file).unwrap();
        let old_root = old
            .index_info(LogicalIndexKind::Variable)
            .unwrap()
            .root_offset;
        let old_page =
            file.get_ref()[old_root as usize..old_root as usize + INDEX_PAGE_SIZE].to_vec();

        let chunks = one_byte_chunks(103, 1);
        let entries = [PersistentIndexEntry::new(
            LogicalIndexKind::Variable,
            u32_key(7),
            103,
        )];
        assert_eq!(
            append_generation_with_indexes(&mut file, &chunks, &entries).unwrap(),
            2
        );

        assert_eq!(
            old.lookup_index_exact(&mut file, LogicalIndexKind::Variable, &u32_key(7))
                .unwrap(),
            vec![100, 101]
        );
        assert_eq!(
            file.get_ref()[old_root as usize..old_root as usize + INDEX_PAGE_SIZE],
            old_page
        );

        let current = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(
            current
                .lookup_index_exact(&mut file, LogicalIndexKind::Variable, &u32_key(7))
                .unwrap(),
            vec![100, 101, 103]
        );
        assert_ne!(
            current
                .index_info(LogicalIndexKind::Variable)
                .unwrap()
                .root_offset,
            old_root
        );
        assert_eq!(
            current
                .lookup_index_exact(&mut file, LogicalIndexKind::ChunkId, &u64_key(103))
                .unwrap(),
            vec![103]
        );
    }

    #[test]
    fn btree_leaf_and_internal_splits_support_large_indexes() {
        let chunks = one_byte_chunks(1, 10_000);
        let entries = (0..10_000u32)
            .map(|value| {
                PersistentIndexEntry::new(
                    LogicalIndexKind::Variable,
                    u32_key(value / 10),
                    value as u64 + 1,
                )
            })
            .collect::<Vec<_>>();
        let mut file = Cursor::new(Vec::new());
        write_generation_container_with_indexes(
            &mut file,
            &Metadata::new("btree-split-test"),
            &chunks,
            &entries,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        let root = snapshot.index_info(LogicalIndexKind::Variable).unwrap();
        assert!(
            root.level >= 2,
            "expected a multi-level B+tree, got level {}",
            root.level
        );
        assert_eq!(
            snapshot
                .lookup_index_exact(&mut file, LogicalIndexKind::Variable, &u32_key(777))
                .unwrap()
                .len(),
            10
        );
        assert_eq!(
            snapshot
                .lookup_index_range(
                    &mut file,
                    LogicalIndexKind::Variable,
                    &u32_key(10),
                    &u32_key(12)
                )
                .unwrap()
                .len(),
            30
        );
    }

    #[test]
    fn copy_on_write_leaf_split_can_split_the_internal_root() {
        // 8,400 ordered entries form exactly 84 full leaves under one 84-child root.
        // Inserting into a full leaf creates child 85 and must split the internal root.
        let chunks = one_byte_chunks(1, 8_400);
        let entries = (0..8_400u64)
            .map(|value| {
                PersistentIndexEntry::new(LogicalIndexKind::Variable, u64_key(value), value + 1)
            })
            .collect::<Vec<_>>();
        let mut file = Cursor::new(Vec::new());
        write_generation_container_with_indexes(
            &mut file,
            &Metadata::new("cow-internal-split"),
            &chunks,
            &entries,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let before = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(
            before.index_info(LogicalIndexKind::Variable).unwrap().level,
            1
        );

        let appended = one_byte_chunks(8_401, 1);
        let extra = [PersistentIndexEntry::new(
            LogicalIndexKind::Variable,
            u64_key(500),
            8_401,
        )];
        assert_eq!(
            append_generation_with_indexes(&mut file, &appended, &extra).unwrap(),
            2
        );
        let after = open_generation_snapshot(&mut file).unwrap();
        let info = after.index_info(LogicalIndexKind::Variable).unwrap();
        assert_eq!(info.level, 2, "the full internal root must have split");
        assert_eq!(info.item_count, 8_401);
        assert_eq!(
            after
                .lookup_index_exact(&mut file, LogicalIndexKind::Variable, &u64_key(500))
                .unwrap(),
            vec![501, 8_401]
        );
        assert_eq!(
            before
                .lookup_index_exact(&mut file, LogicalIndexKind::Variable, &u64_key(500))
                .unwrap(),
            vec![501]
        );
    }

    #[test]
    fn duplicate_index_mapping_and_missing_chunk_references_are_rejected() {
        let chunks = one_byte_chunks(10, 2);
        let duplicate = [
            PersistentIndexEntry::new(LogicalIndexKind::Variable, u32_key(1), 10),
            PersistentIndexEntry::new(LogicalIndexKind::Variable, u32_key(1), 10),
        ];
        let mut file = Cursor::new(Vec::new());
        assert!(matches!(
            write_generation_container_with_indexes(
                &mut file,
                &Metadata::new("duplicate-index"),
                &chunks,
                &duplicate,
                HeaderByteOrder::LittleEndian
            ),
            Err(ChunkIoError::IndexDuplicateEntry)
        ));

        let missing = [PersistentIndexEntry::new(
            LogicalIndexKind::Variable,
            u32_key(1),
            999,
        )];
        let mut file = Cursor::new(Vec::new());
        assert!(matches!(
            write_generation_container_with_indexes(
                &mut file,
                &Metadata::new("missing-index-target"),
                &chunks,
                &missing,
                HeaderByteOrder::LittleEndian
            ),
            Err(ChunkIoError::IndexChunkNotFound(999))
        ));
    }

    #[test]
    fn absent_optional_index_is_distinct_from_corrupt_index() {
        let mut file = indexed_initial(HeaderByteOrder::LittleEndian);
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        assert!(matches!(
            snapshot.lookup_index_exact(&mut file, LogicalIndexKind::ModelProvider, &[0u8; 32]),
            Err(ChunkIoError::IndexNotPresent(_))
        ));
    }

    #[test]
    fn range_lookup_rejects_a_child_page_with_the_wrong_level() {
        let chunks = one_byte_chunks(1, 10_000);
        let entries = (0..10_000u32)
            .map(|value| {
                PersistentIndexEntry::new(
                    LogicalIndexKind::Variable,
                    u32_key(value),
                    value as u64 + 1,
                )
            })
            .collect::<Vec<_>>();
        let mut file = Cursor::new(Vec::new());
        write_generation_container_with_indexes(
            &mut file,
            &Metadata::new("range-child-level-validation"),
            &chunks,
            &entries,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();

        let snapshot = open_generation_snapshot(&mut file).unwrap();
        let info = snapshot.index_info(LogicalIndexKind::Variable).unwrap();
        assert!(info.level >= 2);

        let root = read_index_page(
            &mut file,
            info.root_offset,
            &snapshot.index_page_regions,
            snapshot.byte_order,
            snapshot.generation,
        )
        .unwrap();
        let child_offset = match root {
            DecodedIndexPage::Internal {
                level, children, ..
            } => {
                assert_eq!(level, info.level);
                children[0]
            }
            _ => panic!("expected an internal variable-index root"),
        };
        let child = read_index_page(
            &mut file,
            child_offset,
            &snapshot.index_page_regions,
            snapshot.byte_order,
            snapshot.generation,
        )
        .unwrap();
        let child_level = match child {
            DecodedIndexPage::Internal { level, .. } => level,
            _ => panic!("expected an internal child below the root"),
        };
        assert_eq!(child_level.checked_add(1), Some(info.level));

        let start = child_offset as usize;
        let page = &mut file.get_mut()[start..start + INDEX_PAGE_SIZE];
        page[12..14].copy_from_slice(&(child_level + 1).to_le_bytes());
        page[INDEX_PAGE_DIGEST_RANGE.clone()].fill(0);
        let digest = *blake3::hash(page).as_bytes();
        page[INDEX_PAGE_DIGEST_RANGE.clone()].copy_from_slice(&digest);

        assert!(matches!(
            snapshot.lookup_index_exact(&mut file, LogicalIndexKind::Variable, &u32_key(7)),
            Err(ChunkIoError::IndexTreeInvalid)
        ));
    }

    #[test]
    fn corrupted_index_page_is_reported_as_index_error_when_no_prior_commit_exists() {
        let mut file = indexed_initial(HeaderByteOrder::LittleEndian);
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        let root = snapshot
            .index_info(LogicalIndexKind::Variable)
            .unwrap()
            .root_offset;
        file.get_mut()[root as usize + 100] ^= 1;
        assert!(matches!(
            open_generation_snapshot(&mut file),
            Err(ChunkIoError::IndexPageHashMismatch(_)) | Err(ChunkIoError::NoValidGeneration)
        ));
    }

    #[test]
    fn corrupt_new_generation_index_falls_back_without_mutation() {
        let mut file = indexed_initial(HeaderByteOrder::LittleEndian);
        let first = open_generation_snapshot(&mut file).unwrap();
        let first_root = first
            .index_info(LogicalIndexKind::Variable)
            .unwrap()
            .root_offset;
        let chunks = one_byte_chunks(103, 1);
        let entries = [PersistentIndexEntry::new(
            LogicalIndexKind::Variable,
            u32_key(8),
            103,
        )];
        append_generation_with_indexes(&mut file, &chunks, &entries).unwrap();
        let latest = open_generation_snapshot(&mut file).unwrap();
        let changed_root = latest
            .index_info(LogicalIndexKind::Variable)
            .unwrap()
            .root_offset;
        assert_ne!(first_root, changed_root);
        file.get_mut()[changed_root as usize + 100] ^= 1;
        let before = file.get_ref().clone();
        let recovered = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(recovered.generation(), 1);
        assert_eq!(
            recovered
                .index_info(LogicalIndexKind::Variable)
                .unwrap()
                .root_offset,
            first_root
        );
        assert_eq!(
            file.get_ref(),
            &before,
            "opening must not repair or rewrite the damaged tail"
        );
    }

    #[test]
    fn chunk_id_index_must_match_the_primary_directory() {
        let mut file = indexed_initial(HeaderByteOrder::LittleEndian);
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        let root = snapshot
            .index_info(LogicalIndexKind::ChunkId)
            .unwrap()
            .root_offset;
        file.get_mut()[root as usize + 64 + 32] ^= 1;
        // The page digest and generation digest also cover this mutation; either the page
        // validator or generation recovery must reject the altered mandatory index.
        assert!(open_generation_snapshot(&mut file).is_err());
    }

    #[test]
    fn v038_generation_is_read_only_for_v039_index_appends() {
        let mut file = super::v038_generation_tests::legacy_v038_generation();
        let original = file.get_ref().clone();
        let chunks = one_byte_chunks(901, 1);
        assert!(matches!(
            append_generation(&mut file, &chunks),
            Err(ChunkIoError::GenerationProfileCannotAppend)
        ));
        assert_eq!(
            file.get_ref(),
            &original,
            "refusing an append must not mutate a v0.3.8 file"
        );
    }

    #[test]
    fn empty_v039_file_has_a_valid_empty_mandatory_chunk_id_index() {
        let mut file = Cursor::new(Vec::new());
        write_generation_container_with_indexes(
            &mut file,
            &Metadata::new("empty-indexed-dataset"),
            &[],
            &[],
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        let chunk_index = snapshot.index_info(LogicalIndexKind::ChunkId).unwrap();
        assert_eq!(chunk_index.item_count, 0);
        assert_eq!(chunk_index.root_offset, 0);
        assert!(snapshot
            .lookup_index_exact(&mut file, LogicalIndexKind::ChunkId, &[0; 32])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn v038_generation_footer_v1_remains_readable() {
        let mut file = super::v038_generation_tests::legacy_v038_generation();
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(snapshot.generation(), 1);
        assert_eq!(snapshot.descriptors().len(), 1);
        assert!(snapshot.index_info(LogicalIndexKind::ChunkId).is_none());
        assert_eq!(snapshot.read_chunk(&mut file, 900).unwrap(), b"v038-compat");
        let (metadata, descriptors) = read_chunk_directory(&mut file).unwrap();
        assert_eq!(metadata.dataset_id, "v038-compat");
        assert_eq!(descriptors.len(), 1);
        assert_eq!(
            read_chunk_payload(&mut file, descriptors[0]).unwrap(),
            b"v038-compat"
        );
    }

    #[test]
    fn append_byte_failure_at_every_byte_preserves_previous_index_root() {
        let base = indexed_initial(HeaderByteOrder::LittleEndian).into_inner();
        let chunks = one_byte_chunks(103, 1);
        let entries = [PersistentIndexEntry::new(
            LogicalIndexKind::Variable,
            u32_key(7),
            103,
        )];
        let mut successful = Cursor::new(base.clone());
        append_generation_with_indexes(&mut successful, &chunks, &entries).unwrap();
        let append_bytes = successful.get_ref().len() - base.len();

        for fail_after in 0..append_bytes {
            let mut writer = FailAfter {
                inner: Cursor::new(base.clone()),
                remaining: fail_after,
            };
            assert!(
                append_generation_with_indexes(&mut writer, &chunks, &entries).is_err(),
                "fail_after={fail_after}"
            );
            let mut recovered_file = Cursor::new(writer.inner.into_inner());
            let recovered = open_generation_snapshot(&mut recovered_file).unwrap_or_else(|error| {
                panic!("previous generation not recoverable at byte {fail_after}: {error}")
            });
            assert_eq!(recovered.generation(), 1, "fail_after={fail_after}");
            assert_eq!(
                recovered
                    .lookup_index_exact(
                        &mut recovered_file,
                        LogicalIndexKind::Variable,
                        &u32_key(7)
                    )
                    .unwrap(),
                vec![100, 101]
            );
        }
    }

    struct FailAfter {
        inner: Cursor<Vec<u8>>,
        remaining: usize,
    }
    impl Read for FailAfter {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            self.inner.read(output)
        }
    }
    impl Seek for FailAfter {
        fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(position)
        }
    }
    impl Write for FailAfter {
        fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::Error::other("injected writer failure"));
            }
            let allowed = input.len().min(self.remaining);
            let written = self.inner.write(&input[..allowed])?;
            self.remaining -= written;
            Ok(written)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.inner.flush()
        }
    }
}
