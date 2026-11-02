use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyModule};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;

use wx1_format::{
    open_scientific_snapshot_lazy, stage_chunks_parallel,
    write_generation_container_from_readers_atomic, write_staged_generation_container_atomic,
    ArrayChunkRef, ArraySelection, AsyncScientificWriter, AsyncWriterOptions, AttributeValue,
    ChunkCodec, ChunkReaderInput, ChunkWriteOptions, CubedSphereTopology, DataType, Dimension,
    DimensionRole, EncodedChunkInput, ErrorMetric, GnomonicProjection, GridFamily,
    GridTopologyDescriptor, HaloSpec, HeaderByteOrder, HorizontalStagger, Metadata,
    PayloadByteOrder, QuantizationConfig, ReducedGaussianTopology, ScientificDataset, Variable,
    VariableGridLayout, VerticalStagger, WriteDurability, MAX_CHUNKS, MAX_CHUNK_BYTES,
    MAX_SCIENTIFIC_RANK,
};

fn io_error(error: impl std::fmt::Display) -> PyErr {
    PyIOError::new_err(error.to_string())
}

fn format_error(error: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn dtype_name(data_type: DataType) -> &'static str {
    match data_type {
        DataType::U8 => "uint8",
        DataType::I8 => "int8",
        DataType::U16 => "uint16",
        DataType::I16 => "int16",
        DataType::U32 => "uint32",
        DataType::I32 => "int32",
        DataType::U64 => "uint64",
        DataType::I64 => "int64",
        DataType::F32 => "float32",
        DataType::F64 => "float64",
        DataType::Bool8 => "bool",
    }
}

fn parse_dtype(value: &str) -> PyResult<DataType> {
    match value {
        "uint8" | "u1" => Ok(DataType::U8),
        "int8" | "i1" => Ok(DataType::I8),
        "uint16" | "u2" => Ok(DataType::U16),
        "int16" | "i2" => Ok(DataType::I16),
        "uint32" | "u4" => Ok(DataType::U32),
        "int32" | "i4" => Ok(DataType::I32),
        "uint64" | "u8" => Ok(DataType::U64),
        "int64" | "i8" => Ok(DataType::I64),
        "float32" | "f4" => Ok(DataType::F32),
        "float64" | "f8" => Ok(DataType::F64),
        "bool" | "bool8" => Ok(DataType::Bool8),
        _ => Err(PyValueError::new_err(format!(
            "unsupported WX-1 dtype: {value}"
        ))),
    }
}

fn parse_byte_order(value: &str) -> PyResult<PayloadByteOrder> {
    match value {
        "little" | "<" | "=" | "|" => Ok(PayloadByteOrder::LittleEndian),
        "big" | ">" => Ok(PayloadByteOrder::BigEndian),
        _ => Err(PyValueError::new_err(format!(
            "unsupported byte order: {value}"
        ))),
    }
}

fn dimension_role_name(value: DimensionRole) -> &'static str {
    match value {
        DimensionRole::Other => "other",
        DimensionRole::Time => "time",
        DimensionRole::ForecastReferenceTime => "forecast_reference_time",
        DimensionRole::Ensemble => "ensemble",
        DimensionRole::Vertical => "vertical",
        DimensionRole::Latitude => "latitude",
        DimensionRole::Longitude => "longitude",
        DimensionRole::Height => "height",
        DimensionRole::SoilLayer => "soil_layer",
    }
}

fn dimension_role(value: &str) -> PyResult<DimensionRole> {
    match value {
        "other" | "horizontal" => Ok(DimensionRole::Other),
        "time" => Ok(DimensionRole::Time),
        "forecast_reference_time" => Ok(DimensionRole::ForecastReferenceTime),
        "ensemble" => Ok(DimensionRole::Ensemble),
        "vertical" => Ok(DimensionRole::Vertical),
        "latitude" => Ok(DimensionRole::Latitude),
        "longitude" => Ok(DimensionRole::Longitude),
        "height" => Ok(DimensionRole::Height),
        "soil_layer" => Ok(DimensionRole::SoilLayer),
        _ => Err(PyValueError::new_err(format!(
            "unsupported dimension role: {value}"
        ))),
    }
}

#[pyclass(name = "Wx1File")]
struct PyWx1File {
    path: PathBuf,
    file: std::sync::Mutex<Option<File>>,
    snapshot: wx1_format::ScientificSnapshot,
}

#[pymethods]
impl PyWx1File {
    #[getter]
    fn dataset_id(&self) -> String {
        self.snapshot.metadata().dataset_id.clone()
    }

    #[getter]
    fn generation(&self) -> u64 {
        self.snapshot.generation()
    }

    fn attributes(&self) -> Vec<(String, String)> {
        self.snapshot
            .metadata()
            .attributes
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }

    fn dimensions(&self) -> Vec<(String, u64, String)> {
        self.snapshot
            .dataset()
            .dimensions
            .iter()
            .map(|dimension| {
                (
                    dimension.name.clone(),
                    dimension.length,
                    dimension_role_name(dimension.role).to_owned(),
                )
            })
            .collect()
    }

    fn coordinate_variables(&self) -> Vec<String> {
        self.snapshot
            .dataset()
            .coordinates
            .iter()
            .map(|coordinate| coordinate.variable.clone())
            .collect()
    }

    fn variables(&self, py: Python<'_>) -> PyResult<Py<PyList>> {
        let list = PyList::empty(py);
        for variable in &self.snapshot.dataset().variables {
            let item = PyDict::new(py);
            item.set_item("path", variable.path())?;
            item.set_item("name", &variable.name)?;
            item.set_item("group", &variable.group)?;
            item.set_item("dtype", dtype_name(variable.data_type))?;
            item.set_item(
                "byte_order",
                match variable.byte_order {
                    PayloadByteOrder::LittleEndian => "little",
                    PayloadByteOrder::BigEndian => "big",
                },
            )?;
            item.set_item("dimensions", &variable.dimensions)?;
            item.set_item("chunk_shape", &variable.chunk_shape)?;
            let shape = variable
                .dimensions
                .iter()
                .map(|name| {
                    self.snapshot
                        .dataset()
                        .dimensions
                        .iter()
                        .find(|dimension| dimension.name == *name)
                        .map(|dimension| dimension.length)
                        .ok_or_else(|| PyValueError::new_err("unknown variable dimension"))
                })
                .collect::<PyResult<Vec<_>>>()?;
            item.set_item("shape", shape)?;
            let attributes = PyDict::new(py);
            for (key, value) in &variable.attributes {
                match value {
                    AttributeValue::Text(value) => attributes.set_item(key, value)?,
                    AttributeValue::Integer(value) => attributes.set_item(key, value)?,
                    AttributeValue::Unsigned(value) => attributes.set_item(key, value)?,
                    AttributeValue::Float64Bits(bits) => {
                        attributes.set_item(key, f64::from_bits(*bits))?
                    }
                    AttributeValue::Boolean(value) => attributes.set_item(key, value)?,
                    AttributeValue::Bytes(value) => {
                        attributes.set_item(key, PyBytes::new(py, value))?
                    }
                }
            }
            item.set_item("attributes", attributes)?;
            list.append(item)?;
        }
        Ok(list.unbind())
    }

    /// Read one bounded row-major selection. The returned bytes preserve the variable's
    /// on-disk datatype and payload byte order; no numeric conversion is performed.
    fn read_array_slice(
        &self,
        py: Python<'_>,
        variable_path: String,
        start: Vec<u64>,
        count: Vec<u64>,
    ) -> PyResult<(Py<PyBytes>, String, String, Vec<u64>)> {
        let selection = ArraySelection::new(start, count);
        let result = py.allow_threads(|| {
            let mut guard = self
                .file
                .lock()
                .map_err(|_| (true, "WX-1 file handle mutex poisoned".to_owned()))?;
            if guard.is_none() {
                let reopened = File::open(&self.path).map_err(|error| (true, error.to_string()))?;
                *guard = Some(reopened);
            }
            let file = guard.as_mut().expect("file handle initialized");
            self.snapshot
                .read_array_slice(file, &variable_path, &selection)
                .map_err(|error| (false, error.to_string()))
        });
        let array = result.map_err(|(is_io, message)| {
            if is_io {
                PyIOError::new_err(message)
            } else {
                PyValueError::new_err(message)
            }
        })?;
        let order = match array.byte_order {
            PayloadByteOrder::LittleEndian => "little",
            PayloadByteOrder::BigEndian => "big",
        };
        Ok((
            PyBytes::new(py, &array.data).unbind(),
            dtype_name(array.data_type).to_owned(),
            order.to_owned(),
            array.shape,
        ))
    }

    /// Read discrete multidimensional point coordinates with chunk-grouped parallel batching (R8/R4).
    fn read_point_coordinates(
        &self,
        py: Python<'_>,
        variable_path: String,
        points: Vec<Vec<u64>>,
    ) -> PyResult<(Py<PyBytes>, String, String, Vec<u64>)> {
        let result = py.allow_threads(|| {
            let mut guard = self
                .file
                .lock()
                .map_err(|_| (true, "WX-1 file handle mutex poisoned".to_owned()))?;
            if guard.is_none() {
                let reopened = File::open(&self.path).map_err(|error| (true, error.to_string()))?;
                *guard = Some(reopened);
            }
            let file = guard.as_mut().expect("file handle initialized");
            self.snapshot
                .read_point_coordinates(file, &variable_path, &points)
                .map_err(|error| (false, error.to_string()))
        });
        let array = result.map_err(|(is_io, message)| {
            if is_io {
                PyIOError::new_err(message)
            } else {
                PyValueError::new_err(message)
            }
        })?;
        let order = match array.byte_order {
            PayloadByteOrder::LittleEndian => "little",
            PayloadByteOrder::BigEndian => "big",
        };
        Ok((
            PyBytes::new(py, &array.data).unbind(),
            dtype_name(array.data_type).to_owned(),
            order.to_owned(),
            array.shape,
        ))
    }

    fn plan_array_slice(
        &self,
        variable_path: String,
        start: Vec<u64>,
        count: Vec<u64>,
    ) -> PyResult<(Vec<u64>, u64)> {
        let plan = self
            .snapshot
            .plan_array_slice(&variable_path, &ArraySelection::new(start, count))
            .map_err(format_error)?;
        Ok((plan.chunk_ids, plan.output_bytes))
    }

    #[allow(clippy::type_complexity)]
    fn plan_tile_slice(
        &self,
        variable_path: String,
        tile_index: u64,
        include_halos: bool,
    ) -> PyResult<(Vec<u64>, u64, Vec<u64>, Vec<u64>)> {
        let sel = self
            .snapshot
            .dataset()
            .tile_selection(&variable_path, tile_index, include_halos)
            .map_err(format_error)?;
        let plan = self
            .snapshot
            .plan_array_slice(&variable_path, &sel)
            .map_err(format_error)?;
        Ok((plan.chunk_ids, plan.output_bytes, plan.start, plan.shape))
    }

    fn read_tile_slice(
        &self,
        py: Python<'_>,
        variable_path: String,
        tile_index: u64,
        include_halos: bool,
    ) -> PyResult<(Py<PyBytes>, String, String, Vec<u64>)> {
        let sel = self
            .snapshot
            .dataset()
            .tile_selection(&variable_path, tile_index, include_halos)
            .map_err(format_error)?;
        self.read_array_slice(py, variable_path, sel.start, sel.count)
    }

    fn grid_topologies(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let out = PyDict::new(py);
        for gm in &self.snapshot.dataset().grid_mappings {
            if let Some(desc) =
                GridTopologyDescriptor::from_grid_mapping(gm).map_err(format_error)?
            {
                let item = PyDict::new(py);
                item.set_item("family", desc.family.as_str())?;
                if let Some(cs) = desc.cubed_sphere {
                    item.set_item("c_resolution", cs.c_resolution)?;
                    item.set_item("projection", cs.projection.as_str())?;
                    item.set_item("num_tiles", 6)?;
                }
                if let Some(rg) = desc.reduced_gaussian {
                    item.set_item("gaussian_n", rg.gaussian_n)?;
                    item.set_item("is_octahedral", rg.is_octahedral)?;
                    item.set_item("total_points", rg.total_points())?;
                }
                out.set_item(&gm.name, item)?;
            }
        }
        Ok(out.unbind())
    }

    fn close(&self) -> PyResult<()> {
        let mut guard = self
            .file
            .lock()
            .map_err(|_| PyIOError::new_err("WX-1 file handle mutex poisoned"))?;
        *guard = None;
        Ok(())
    }

    fn validate(&self) -> PyResult<()> {
        let mut guard = self
            .file
            .lock()
            .map_err(|_| PyIOError::new_err("WX-1 file handle mutex poisoned"))?;
        if guard.is_none() {
            let reopened = File::open(&self.path).map_err(io_error)?;
            *guard = Some(reopened);
        }
        let file = guard.as_mut().expect("file handle initialized");
        self.snapshot.validate_full(file).map_err(format_error)
    }

    fn provenance(&self, py: Python<'_>) -> PyResult<Py<PyList>> {
        let list = PyList::empty(py);
        for prov in &self.snapshot.dataset().provenance {
            let item = PyDict::new(py);
            item.set_item("source", &prov.source)?;
            item.set_item("provider", prov.provider.as_deref())?;
            item.set_item("product", prov.product.as_deref())?;
            item.set_item("institution", prov.institution.as_deref())?;
            item.set_item("license", prov.license.as_deref())?;
            item.set_item("history", prov.history.as_deref())?;
            item.set_item("reference_time", prov.reference_time.as_deref())?;
            list.append(item)?;
        }
        Ok(list.unbind())
    }
}

#[pyfunction]
fn open_file(path: String) -> PyResult<PyWx1File> {
    let path = PathBuf::from(path);
    let mut file = File::open(&path).map_err(io_error)?;
    let snapshot = open_scientific_snapshot_lazy(&mut file).map_err(format_error)?;
    Ok(PyWx1File {
        path,
        file: std::sync::Mutex::new(Some(file)),
        snapshot,
    })
}

/// Reader over a rectangular tile of a contiguous row-major source array.
/// It emits contiguous last-axis rows to the bounded WX-1 writer without constructing
/// a second full copy of every chunk.
struct ArrayTileReader<'a> {
    source: &'a [u8],
    dataset_shape: Vec<u64>,
    origin: Vec<u64>,
    tile_shape: Vec<u64>,
    strides: Vec<u64>,
    item_size: u64,
    position: u64,
    total_bytes: u64,
    row_bytes: u64,
}

impl<'a> ArrayTileReader<'a> {
    fn new(
        source: &'a [u8],
        dataset_shape: &[u64],
        origin: &[u64],
        tile_shape: &[u64],
        item_size: u64,
    ) -> PyResult<Self> {
        let elements = tile_shape.iter().try_fold(1u64, |value, length| {
            value
                .checked_mul(*length)
                .ok_or_else(|| PyValueError::new_err("tile shape overflow"))
        })?;
        let total_bytes = elements
            .checked_mul(item_size)
            .ok_or_else(|| PyValueError::new_err("tile byte length overflow"))?;
        let row_elements = tile_shape.last().copied().unwrap_or(1);
        let row_bytes = row_elements
            .checked_mul(item_size)
            .ok_or_else(|| PyValueError::new_err("tile row length overflow"))?;
        let mut strides = vec![1u64; dataset_shape.len()];
        for axis in (0..dataset_shape.len().saturating_sub(1)).rev() {
            strides[axis] = strides[axis + 1]
                .checked_mul(dataset_shape[axis + 1])
                .ok_or_else(|| PyValueError::new_err("array stride overflow"))?;
        }
        Ok(Self {
            source,
            dataset_shape: dataset_shape.to_vec(),
            origin: origin.to_vec(),
            tile_shape: tile_shape.to_vec(),
            strides,
            item_size,
            position: 0,
            total_bytes,
            row_bytes,
        })
    }

    fn source_row_offset(&self, row_index: u64) -> io::Result<u64> {
        if self.dataset_shape.is_empty() {
            return Ok(0);
        }
        let rank = self.dataset_shape.len();
        let mut remainder = row_index;
        let mut element_offset = 0u64;
        for axis in (0..rank - 1).rev() {
            let axis_len = self.tile_shape[axis];
            if axis_len == 0 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "zero tile axis"));
            }
            let coordinate = remainder % axis_len;
            remainder /= axis_len;
            let absolute = self.origin[axis]
                .checked_add(coordinate)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "coordinate overflow"))?;
            element_offset = element_offset
                .checked_add(absolute.checked_mul(self.strides[axis]).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "source offset overflow")
                })?)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "source offset overflow")
                })?;
        }
        let last_axis = rank - 1;
        element_offset = element_offset
            .checked_add(self.origin[last_axis])
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "source offset overflow"))?;
        element_offset.checked_mul(self.item_size).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "source byte offset overflow")
        })
    }

    fn is_single_contiguous_slice(&self) -> bool {
        let rank = self.dataset_shape.len();
        if rank <= 1 {
            return true;
        }
        // Find the last axis from the left after which all trailing axes span the full dataset axis
        let mut full_suffix_start = rank;
        while full_suffix_start > 0 {
            let a = full_suffix_start - 1;
            if self.origin[a] == 0 && self.tile_shape[a] == self.dataset_shape[a] {
                full_suffix_start = a;
            } else {
                break;
            }
        }
        // All axes strictly before (full_suffix_start - 1) must have tile_shape[a] <= 1
        let split_axis = full_suffix_start.saturating_sub(1);
        self.tile_shape[..split_axis].iter().all(|&len| len <= 1)
    }

    fn extract_vec(&mut self) -> io::Result<Vec<u8>> {
        let total = usize::try_from(self.total_bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "tile too large"))?;
        if total == 0 {
            return Ok(Vec::new());
        }
        if self.is_single_contiguous_slice() {
            let start = usize::try_from(self.source_row_offset(0)?)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "offset overflow"))?;
            let end = start
                .checked_add(total)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "end overflow"))?;
            let slice = self.source.get(start..end).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "source array is shorter than declared",
                )
            })?;
            return Ok(slice.to_vec());
        }
        let mut out = vec![0u8; total];
        self.read_exact(&mut out)?;
        Ok(out)
    }
}

impl Read for ArrayTileReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() || self.position >= self.total_bytes {
            return Ok(0);
        }
        if self.row_bytes == 0 {
            return Ok(0);
        }
        let mut written = 0usize;
        while written < buffer.len() && self.position < self.total_bytes {
            let row_index = self.position / self.row_bytes;
            let row_offset = self.position % self.row_bytes;
            let source_start = self
                .source_row_offset(row_index)?
                .checked_add(row_offset)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "source offset overflow")
                })?;
            let available_in_row = self.row_bytes - row_offset;
            let available_in_output = u64::try_from(buffer.len() - written)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "buffer too large"))?;
            let count = available_in_row.min(available_in_output);
            let source_end = source_start
                .checked_add(count)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "source end overflow"))?;
            let source_start = usize::try_from(source_start).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "source offset too large")
            })?;
            let source_end = usize::try_from(source_end)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "source end too large"))?;
            let count = usize::try_from(count)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "copy length too large"))?;
            let source_slice = self.source.get(source_start..source_end).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "source array is shorter than declared",
                )
            })?;
            buffer[written..written + count].copy_from_slice(source_slice);
            written += count;
            self.position += count as u64;
        }
        Ok(written)
    }
}

fn chunk_references(shape: &[u64], chunk_shape: &[u64]) -> PyResult<Vec<ArrayChunkRef>> {
    if shape.len() != chunk_shape.len() || shape.len() > MAX_SCIENTIFIC_RANK {
        return Err(PyValueError::new_err(
            "shape, chunk_shape and dimension rank must match",
        ));
    }
    if shape.is_empty() {
        return Ok(vec![ArrayChunkRef {
            chunk_id: 1,
            origin: vec![],
            shape: vec![],
        }]);
    }
    if shape.contains(&0) {
        return Ok(Vec::new());
    }
    if chunk_shape.contains(&0) {
        return Err(PyValueError::new_err(
            "chunk_shape values must be greater than zero",
        ));
    }

    fn visit(
        axis: usize,
        shape: &[u64],
        chunk_shape: &[u64],
        origin: &mut Vec<u64>,
        tile_shape: &mut Vec<u64>,
        output: &mut Vec<ArrayChunkRef>,
    ) -> PyResult<()> {
        if axis == shape.len() {
            if output.len() as u64 >= MAX_CHUNKS {
                return Err(PyValueError::new_err(
                    "array would exceed the WX-1 chunk limit",
                ));
            }
            let id = output.len() as u64 + 1;
            output.push(ArrayChunkRef {
                chunk_id: id,
                origin: origin.clone(),
                shape: tile_shape.clone(),
            });
            return Ok(());
        }
        let mut start = 0u64;
        while start < shape[axis] {
            let length = chunk_shape[axis].min(shape[axis] - start);
            origin.push(start);
            tile_shape.push(length);
            visit(axis + 1, shape, chunk_shape, origin, tile_shape, output)?;
            origin.pop();
            tile_shape.pop();
            start = start
                .checked_add(length)
                .ok_or_else(|| PyValueError::new_err("chunk origin overflow"))?;
        }
        Ok(())
    }

    let mut output = Vec::new();
    visit(
        0,
        shape,
        chunk_shape,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut output,
    )?;
    Ok(output)
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn write_array(
    py: Python<'_>,
    path: String,
    dataset_id: String,
    variable_path: String,
    dimensions: Vec<String>,
    shape: Vec<u64>,
    chunk_shape: Vec<u64>,
    dtype: String,
    byte_order: String,
    data: &Bound<'_, PyBytes>,
    dimension_roles: Option<Vec<String>>,
    attributes: Option<HashMap<String, String>>,
    profile: Option<String>,
    lossy_error_bound: Option<f64>,
    lossy_metric: Option<String>,
    codec: Option<String>,
    workers: Option<usize>,
    durability: Option<String>,
    grid_family: Option<String>,
    c_resolution: Option<u64>,
    halo_width: Option<u64>,
    stagger_horizontal: Option<String>,
    stagger_vertical: Option<String>,
    tile_dimension: Option<String>,
) -> PyResult<()> {
    let path = PathBuf::from(path);
    let mut data = data.as_bytes().to_vec();
    if dimensions.len() != shape.len() || dimensions.len() > MAX_SCIENTIFIC_RANK {
        return Err(PyValueError::new_err(
            "dimensions and shape must have equal supported rank",
        ));
    }
    if dimensions.iter().any(|name| name.is_empty()) {
        return Err(PyValueError::new_err("dimension names cannot be empty"));
    }
    let name = variable_path
        .strip_prefix('/')
        .unwrap_or(variable_path.as_str());
    if name.is_empty() || name.contains('/') {
        return Err(PyValueError::new_err(
            "this initial writer accepts one root-level variable path",
        ));
    }
    let data_type = parse_dtype(&dtype)?;
    let payload_order = parse_byte_order(&byte_order)?;
    let expected_bytes = shape
        .iter()
        .try_fold(1u64, |value, length| {
            value
                .checked_mul(*length)
                .ok_or_else(|| PyValueError::new_err("array shape overflow"))
        })?
        .checked_mul(data_type.element_size())
        .ok_or_else(|| PyValueError::new_err("array byte length overflow"))?;
    if expected_bytes != data.len() as u64 {
        return Err(PyValueError::new_err(format!(
            "data length mismatch: expected {expected_bytes} bytes, got {}",
            data.len()
        )));
    }
    if data_type == DataType::Bool8 && data.iter().any(|value| *value > 1) {
        return Err(PyValueError::new_err("Bool8 data must contain only 0 or 1"));
    }

    let prof = profile.unwrap_or_else(|| "restart".to_string());
    let mut variable_attributes: std::collections::BTreeMap<String, AttributeValue> = attributes
        .unwrap_or_default()
        .into_iter()
        .map(|(key, value)| (key, AttributeValue::Text(value)))
        .collect();

    if prof == "restart" {
        if lossy_error_bound.is_some() {
            return Err(PyValueError::new_err(
                "lossy quantization is strictly forbidden in restart profile",
            ));
        }
        variable_attributes.insert(
            "wx1:lossy".to_string(),
            AttributeValue::Text("false".to_string()),
        );
        variable_attributes.insert(
            "wx1:profile".to_string(),
            AttributeValue::Text("restart".to_string()),
        );
    } else if prof == "output" {
        if let Some(bound) = lossy_error_bound {
            if bound <= 0.0 || bound.is_nan() || bound.is_infinite() {
                return Err(PyValueError::new_err(
                    "lossy_error_bound must be positive and finite",
                ));
            }
            let metric_name = lossy_metric.unwrap_or_else(|| "max_abs_error".to_string());
            let metric = match metric_name.as_str() {
                "relative" | "relative_error" => ErrorMetric::Relative,
                _ => ErrorMetric::Absolute,
            };
            let config = QuantizationConfig {
                error_bound: bound,
                metric,
                preserve_specials: true,
            };

            if data_type == DataType::F32 {
                let count = data.len() / 4;
                let mut floats = Vec::with_capacity(count);
                for chunk in data.chunks_exact(4) {
                    let val = match payload_order {
                        PayloadByteOrder::LittleEndian => {
                            f32::from_le_bytes(chunk.try_into().unwrap())
                        }
                        PayloadByteOrder::BigEndian => {
                            f32::from_be_bytes(chunk.try_into().unwrap())
                        }
                    };
                    floats.push(val);
                }
                let (quantized, report) = wx1_format::quantize_f32_slice(&floats, &config, None)
                    .map_err(|e| PyValueError::new_err(e.to_string()))?;

                let mut q_bytes = Vec::with_capacity(data.len());
                for &val in &quantized {
                    match payload_order {
                        PayloadByteOrder::LittleEndian => {
                            q_bytes.extend_from_slice(&val.to_le_bytes());
                        }
                        PayloadByteOrder::BigEndian => {
                            q_bytes.extend_from_slice(&val.to_be_bytes());
                        }
                    }
                }
                data = q_bytes;

                variable_attributes.insert(
                    "wx1:lossy".to_string(),
                    AttributeValue::Text("true".to_string()),
                );
                variable_attributes.insert(
                    "wx1:profile".to_string(),
                    AttributeValue::Text("output".to_string()),
                );
                variable_attributes.insert(
                    "wx1:quantization_type".to_string(),
                    AttributeValue::Text(match metric {
                        ErrorMetric::Absolute => "linear_absolute".to_string(),
                        ErrorMetric::Relative => "bit_round_relative".to_string(),
                    }),
                );
                variable_attributes.insert(
                    "wx1:error_bound".to_string(),
                    AttributeValue::Text(format!("{bound}")),
                );
                variable_attributes.insert(
                    "wx1:error_metric".to_string(),
                    AttributeValue::Text(metric.to_string()),
                );
                variable_attributes.insert(
                    "wx1:codec_version".to_string(),
                    AttributeValue::Text("1.0".to_string()),
                );
                variable_attributes.insert(
                    "wx1:max_measured_abs_error".to_string(),
                    AttributeValue::Text(format!("{}", report.max_measured_abs_error)),
                );
                variable_attributes.insert(
                    "wx1:measured_rmse".to_string(),
                    AttributeValue::Text(format!("{}", report.rmse)),
                );
                variable_attributes.insert(
                    "wx1:measured_bias".to_string(),
                    AttributeValue::Text(format!("{}", report.bias)),
                );
            }
        }
    } else {
        return Err(PyValueError::new_err(format!(
            "unknown profile '{prof}', expected 'restart' or 'output'"
        )));
    }

    let chunk_codec = match codec.as_deref().unwrap_or("zstd") {
        "raw" => ChunkCodec::Raw,
        "lz4" => ChunkCodec::Lz4,
        "zstd" => ChunkCodec::Zstandard,
        "zstd_shuffle" | "zstd_byteshuffle" | "byteshuffle" | "shuffle" => {
            match data_type.element_size() {
                4 => ChunkCodec::ZstdByteShuffle4,
                8 => ChunkCodec::ZstdByteShuffle8,
                2 => ChunkCodec::ZstdByteShuffle2,
                _ => ChunkCodec::Zstandard,
            }
        }
        other => return Err(PyValueError::new_err(format!("unknown codec: {other}"))),
    };

    let write_durability = match durability.as_deref().unwrap_or("durable") {
        "durable" => WriteDurability::Durable,
        "buffered" => WriteDurability::Buffered,
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown durability '{other}', expected 'durable' or 'buffered'"
            )))
        }
    };
    let num_workers = workers.unwrap_or(1).max(1);

    let roles = dimension_roles.unwrap_or_else(|| vec!["other".to_owned(); dimensions.len()]);
    if roles.len() != dimensions.len() {
        return Err(PyValueError::new_err(
            "dimension_roles must have one role per dimension",
        ));
    }
    let mut dataset = ScientificDataset::new();
    for ((name, length), role) in dimensions.iter().zip(shape.iter()).zip(roles.iter()) {
        dataset.dimensions.push(Dimension {
            name: name.clone(),
            length: *length,
            role: dimension_role(role)?,
        });
    }
    let refs = chunk_references(&shape, &chunk_shape)?;
    for chunk in &refs {
        let elements = chunk.shape.iter().try_fold(1u64, |value, length| {
            value
                .checked_mul(*length)
                .ok_or_else(|| PyValueError::new_err("chunk shape overflow"))
        })?;
        let raw_len = elements
            .checked_mul(data_type.element_size())
            .ok_or_else(|| PyValueError::new_err("chunk byte length overflow"))?;
        if raw_len > MAX_CHUNK_BYTES {
            return Err(PyValueError::new_err(
                "chunk exceeds the WX-1 64 MiB raw chunk limit",
            ));
        }
    }
    let mut var = Variable {
        name: name.to_owned(),
        group: "/".to_owned(),
        data_type,
        byte_order: payload_order,
        dimensions: dimensions.clone(),
        chunk_shape,
        fill_value: None,
        missing_value: None,
        attributes: variable_attributes.clone(),
        chunks: refs.clone(),
        grid_mapping: None,
    };

    if stagger_horizontal.is_some()
        || stagger_vertical.is_some()
        || halo_width.is_some()
        || tile_dimension.is_some()
        || grid_family.is_some()
    {
        let h_stag = HorizontalStagger::parse(stagger_horizontal.as_deref().unwrap_or("center"))
            .map_err(format_error)?;
        let v_stag = VerticalStagger::parse(stagger_vertical.as_deref().unwrap_or("full_level"))
            .map_err(format_error)?;
        let halo = HaloSpec::symmetric(halo_width.unwrap_or(0));
        let layout = VariableGridLayout {
            horizontal_stagger: h_stag,
            vertical_stagger: v_stag,
            halo,
            halo_consistency: wx1_format::HaloConsistencyState::Unverified,
            tile_dimension: tile_dimension.clone(),
        };
        layout.apply_to_variable(&mut var);
    }

    if let Some(ref fam_str) = grid_family {
        let fam = GridFamily::parse(fam_str).map_err(format_error)?;
        let cs_topo = if fam == GridFamily::CubedSphere {
            let cres = c_resolution.ok_or_else(|| {
                PyValueError::new_err("c_resolution is required when grid_family='cubed_sphere'")
            })?;
            Some(
                CubedSphereTopology::standard(cres, GnomonicProjection::Equiangular)
                    .map_err(format_error)?,
            )
        } else {
            None
        };
        let rg_topo = if fam == GridFamily::ReducedGaussian {
            let gn = c_resolution.ok_or_else(|| {
                PyValueError::new_err("c_resolution (gaussian_n) is required for reduced_gaussian")
            })?;
            Some(ReducedGaussianTopology::octahedral(gn).map_err(format_error)?)
        } else {
            None
        };
        let desc = GridTopologyDescriptor {
            family: fam,
            cubed_sphere: cs_topo,
            reduced_gaussian: rg_topo,
            nesting: None,
        };
        let gm = desc
            .to_grid_mapping("primary_grid", dimensions)
            .map_err(format_error)?;
        var.grid_mapping = Some("primary_grid".to_owned());
        dataset.grid_mappings.push(gm);
    }

    dataset.variables.push(var);
    dataset.validate().map_err(format_error)?;
    let mut metadata = Metadata::new(dataset_id).with_scientific_model(dataset);
    for (k, v) in &variable_attributes {
        if let AttributeValue::Text(s) = v {
            metadata.attributes.insert(k.clone(), s.clone());
        }
    }

    if num_workers > 1 || write_durability == WriteDurability::Buffered {
        // Extract contiguous tile slices and stage in parallel without holding the Python GIL
        let mut tile_buffers = Vec::with_capacity(refs.len());
        for chunk in &refs {
            let mut reader = ArrayTileReader::new(
                &data,
                &shape,
                &chunk.origin,
                &chunk.shape,
                data_type.element_size(),
            )?;
            let buf = reader.extract_vec().map_err(io_error)?;
            tile_buffers.push((chunk.chunk_id, buf));
        }
        py.allow_threads(|| {
            let encoded_inputs: Vec<EncodedChunkInput<'_>> = tile_buffers
                .iter()
                .map(|(id, buf)| EncodedChunkInput {
                    id: *id,
                    data: buf.as_slice(),
                    codec: chunk_codec,
                })
                .collect();
            let staged = stage_chunks_parallel(&encoded_inputs, num_workers)?;
            write_staged_generation_container_atomic(
                &path,
                &metadata,
                &staged,
                &[],
                HeaderByteOrder::LittleEndian,
                write_durability,
            )
        })
        .map_err(format_error)
    } else {
        let mut readers = refs
            .iter()
            .map(|chunk| {
                ArrayTileReader::new(
                    &data,
                    &shape,
                    &chunk.origin,
                    &chunk.shape,
                    data_type.element_size(),
                )
            })
            .collect::<PyResult<Vec<_>>>()?;
        let mut inputs = Vec::with_capacity(refs.len());
        for (reference, reader) in refs.iter().zip(readers.iter_mut()) {
            let raw_len = reference
                .shape
                .iter()
                .try_fold(1u64, |value, length| {
                    value
                        .checked_mul(*length)
                        .ok_or_else(|| PyValueError::new_err("chunk shape overflow"))
                })?
                .checked_mul(data_type.element_size())
                .ok_or_else(|| PyValueError::new_err("chunk byte length overflow"))?;
            inputs.push(ChunkReaderInput {
                id: reference.chunk_id,
                reader,
                raw_len,
                codec: chunk_codec,
            });
        }
        write_generation_container_from_readers_atomic(
            path,
            &metadata,
            &mut inputs,
            HeaderByteOrder::LittleEndian,
            ChunkWriteOptions::default(),
        )
        .map_err(format_error)
    }
}

struct AsyncWriterStepState {
    dataset_id: String,
    variable_name: String,
    spatial_dimensions: Vec<String>,
    spatial_roles: Vec<DimensionRole>,
    spatial_shape: Vec<u64>,
    spatial_chunk_shape: Vec<u64>,
    data_type: DataType,
    byte_order: PayloadByteOrder,
    codec: ChunkCodec,
    accumulated_chunks: Vec<ArrayChunkRef>,
    next_chunk_id: u64,
    next_step_index: u64,
}

#[pyclass(name = "AsyncWriter")]
struct PyAsyncWriter {
    writer: std::sync::Mutex<Option<AsyncScientificWriter>>,
    state: std::sync::Mutex<AsyncWriterStepState>,
}

#[pymethods]
impl PyAsyncWriter {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, dataset_id, variable_path, spatial_dimensions, spatial_shape, spatial_chunk_shape, dtype="float32", byte_order="little", codec="zstd_shuffle", workers=4, max_pending_generations=2, max_queued_bytes=268435456, durability="durable"))]
    fn new(
        path: String,
        dataset_id: String,
        variable_path: String,
        spatial_dimensions: Vec<String>,
        spatial_shape: Vec<u64>,
        spatial_chunk_shape: Vec<u64>,
        dtype: &str,
        byte_order: &str,
        codec: &str,
        workers: usize,
        max_pending_generations: usize,
        max_queued_bytes: u64,
        durability: &str,
    ) -> PyResult<Self> {
        if spatial_dimensions.len() != spatial_shape.len()
            || spatial_dimensions.len() != spatial_chunk_shape.len()
            || spatial_dimensions.len() + 1 > MAX_SCIENTIFIC_RANK
        {
            return Err(PyValueError::new_err(
                "spatial_dimensions, spatial_shape, and spatial_chunk_shape must have equal supported rank",
            ));
        }
        let name = variable_path
            .strip_prefix('/')
            .unwrap_or(variable_path.as_str())
            .to_owned();
        if name.is_empty() || name.contains('/') {
            return Err(PyValueError::new_err(
                "AsyncWriter requires one root-level variable path",
            ));
        }
        let data_type = parse_dtype(dtype)?;
        let payload_order = parse_byte_order(byte_order)?;
        let chunk_codec = match codec {
            "raw" => ChunkCodec::Raw,
            "lz4" => ChunkCodec::Lz4,
            "zstd" => ChunkCodec::Zstandard,
            "zstd_shuffle" | "zstd_byteshuffle" | "byteshuffle" | "shuffle" => {
                match data_type.element_size() {
                    4 => ChunkCodec::ZstdByteShuffle4,
                    8 => ChunkCodec::ZstdByteShuffle8,
                    2 => ChunkCodec::ZstdByteShuffle2,
                    _ => ChunkCodec::Zstandard,
                }
            }
            other => return Err(PyValueError::new_err(format!("unknown codec: {other}"))),
        };
        let write_durability = match durability {
            "durable" => WriteDurability::Durable,
            "buffered" => WriteDurability::Buffered,
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown durability '{other}', expected 'durable' or 'buffered'"
                )))
            }
        };
        let options = AsyncWriterOptions {
            max_pending_generations,
            max_queued_bytes,
            staging_workers: workers.max(1),
            durability: write_durability,
            byte_order: HeaderByteOrder::LittleEndian,
        };
        let writer = AsyncScientificWriter::open(&path, options).map_err(format_error)?;
        let spatial_roles = vec![DimensionRole::Other; spatial_dimensions.len()];
        Ok(Self {
            writer: std::sync::Mutex::new(Some(writer)),
            state: std::sync::Mutex::new(AsyncWriterStepState {
                dataset_id,
                variable_name: name,
                spatial_dimensions,
                spatial_roles,
                spatial_shape,
                spatial_chunk_shape,
                data_type,
                byte_order: payload_order,
                codec: chunk_codec,
                accumulated_chunks: Vec::new(),
                next_chunk_id: 1,
                next_step_index: 0,
            }),
        })
    }

    /// Stages and enqueues one NWP timestep array `(spatial_shape...)` into the bounded background queue.
    fn submit_step(&self, py: Python<'_>, data: &Bound<'_, PyBytes>) -> PyResult<()> {
        let raw_bytes = data.as_bytes();
        let (metadata, tile_buffers) = {
            let mut st = self
                .state
                .lock()
                .map_err(|_| PyIOError::new_err("AsyncWriter state mutex poisoned"))?;
            let expected_bytes = st
                .spatial_shape
                .iter()
                .try_fold(1u64, |v, l| {
                    v.checked_mul(*l)
                        .ok_or_else(|| PyValueError::new_err("spatial shape overflow"))
                })?
                .checked_mul(st.data_type.element_size())
                .ok_or_else(|| PyValueError::new_err("spatial byte length overflow"))?;
            if raw_bytes.len() as u64 != expected_bytes {
                return Err(PyValueError::new_err(format!(
                    "step data length mismatch: expected {expected_bytes} bytes, got {}",
                    raw_bytes.len()
                )));
            }

            let step_idx = st.next_step_index;
            let spatial_refs = chunk_references(&st.spatial_shape, &st.spatial_chunk_shape)?;
            let mut tile_buffers = Vec::with_capacity(spatial_refs.len());
            for sref in &spatial_refs {
                let cid = st.next_chunk_id;
                st.next_chunk_id += 1;
                let mut origin = Vec::with_capacity(1 + sref.origin.len());
                origin.push(step_idx);
                origin.extend_from_slice(&sref.origin);
                let mut shape = Vec::with_capacity(1 + sref.shape.len());
                shape.push(1);
                shape.extend_from_slice(&sref.shape);
                st.accumulated_chunks.push(ArrayChunkRef {
                    chunk_id: cid,
                    origin,
                    shape,
                });

                let mut reader = ArrayTileReader::new(
                    raw_bytes,
                    &st.spatial_shape,
                    &sref.origin,
                    &sref.shape,
                    st.data_type.element_size(),
                )?;
                let buf = reader.extract_vec().map_err(io_error)?;
                tile_buffers.push((cid, st.codec, buf));
            }
            st.next_step_index += 1;

            let mut dims = Vec::with_capacity(1 + st.spatial_dimensions.len());
            dims.push(Dimension {
                name: "time".to_owned(),
                length: st.next_step_index,
                role: DimensionRole::Time,
            });
            for ((dname, dlen), drole) in st
                .spatial_dimensions
                .iter()
                .zip(st.spatial_shape.iter())
                .zip(st.spatial_roles.iter())
            {
                dims.push(Dimension {
                    name: dname.clone(),
                    length: *dlen,
                    role: *drole,
                });
            }

            let mut var_dims = Vec::with_capacity(1 + st.spatial_dimensions.len());
            var_dims.push("time".to_owned());
            var_dims.extend(st.spatial_dimensions.iter().cloned());

            let mut var_chunk_shape = Vec::with_capacity(1 + st.spatial_chunk_shape.len());
            var_chunk_shape.push(1);
            var_chunk_shape.extend_from_slice(&st.spatial_chunk_shape);

            let mut dataset = ScientificDataset::new();
            dataset.dimensions = dims;
            dataset.variables.push(Variable {
                name: st.variable_name.clone(),
                group: "/".to_owned(),
                data_type: st.data_type,
                byte_order: st.byte_order,
                dimensions: var_dims,
                chunk_shape: var_chunk_shape,
                fill_value: None,
                missing_value: None,
                attributes: std::collections::BTreeMap::new(),
                chunks: st.accumulated_chunks.clone(),
                grid_mapping: None,
            });
            let metadata = Metadata::new(&st.dataset_id).with_scientific_model(dataset);
            (metadata, tile_buffers)
        };

        py.allow_threads(move || {
            let guard = self
                .writer
                .lock()
                .map_err(|_| "AsyncWriter mutex poisoned".to_owned())?;
            let writer = guard
                .as_ref()
                .ok_or_else(|| "AsyncWriter is closed".to_owned())?;
            writer
                .submit_raw_generation(metadata, tile_buffers, Vec::new())
                .map_err(|e| e.to_string())
        })
        .map_err(format_error)
    }

    fn flush(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let tel = py
            .allow_threads(|| {
                let guard = self
                    .writer
                    .lock()
                    .map_err(|_| "AsyncWriter mutex poisoned".to_owned())?;
                let writer = guard
                    .as_ref()
                    .ok_or_else(|| "AsyncWriter is closed".to_owned())?;
                writer.flush().map_err(|e| e.to_string())
            })
            .map_err(format_error)?;
        Self::telemetry_to_dict(py, &tel)
    }

    fn telemetry(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let guard = self
            .writer
            .lock()
            .map_err(|_| PyIOError::new_err("AsyncWriter mutex poisoned"))?;
        let writer = guard
            .as_ref()
            .ok_or_else(|| PyValueError::new_err("AsyncWriter is closed"))?;
        let tel = writer.telemetry().map_err(format_error)?;
        Self::telemetry_to_dict(py, &tel)
    }

    fn close(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let tel = py
            .allow_threads(|| {
                let mut guard = self
                    .writer
                    .lock()
                    .map_err(|_| "AsyncWriter mutex poisoned".to_owned())?;
                let writer = guard
                    .take()
                    .ok_or_else(|| "AsyncWriter is already closed".to_owned())?;
                writer.close().map_err(|e| e.to_string())
            })
            .map_err(format_error)?;
        Self::telemetry_to_dict(py, &tel)
    }
}

impl PyAsyncWriter {
    fn telemetry_to_dict(
        py: Python<'_>,
        tel: &wx1_format::AsyncWriterTelemetry,
    ) -> PyResult<Py<PyDict>> {
        let d = PyDict::new(py);
        d.set_item("submitted_generations", tel.submitted_generations)?;
        d.set_item("committed_generations", tel.committed_generations)?;
        d.set_item("total_raw_bytes", tel.total_raw_bytes)?;
        d.set_item("total_staged_bytes", tel.total_staged_bytes)?;
        d.set_item("backpressure_events", tel.backpressure_events)?;
        d.set_item("total_enqueue_stall_ns", tel.total_enqueue_stall_ns)?;
        d.set_item("max_enqueue_stall_ns", tel.max_enqueue_stall_ns)?;
        d.set_item("total_stage_duration_ns", tel.total_stage_duration_ns)?;
        d.set_item("total_commit_duration_ns", tel.total_commit_duration_ns)?;
        Ok(d.unbind())
    }
}

#[pyfunction]
fn unit_dimension(unit: String) -> Option<&'static str> {
    wx1_registry::unit_dimension(&unit)
}

#[pyfunction]
fn unit_conversion_factor(from: String, to: String) -> PyResult<(f64, f64)> {
    wx1_registry::unit_conversion_factor(&from, &to).map_err(format_error)
}

#[pyfunction]
fn resolve_cf_standard_name(py: Python<'_>, name: String) -> PyResult<Option<Py<PyDict>>> {
    if let Some(mapping) = wx1_registry::resolve_cf_standard_name(&name) {
        let dict = PyDict::new(py);
        dict.set_item("standard_name", mapping.standard_name)?;
        dict.set_item("canonical_id", mapping.canonical_id)?;
        dict.set_item("canonical_units", mapping.canonical_units)?;
        dict.set_item("physical_dimension", mapping.physical_dimension)?;
        Ok(Some(dict.unbind()))
    } else {
        Ok(None)
    }
}

#[pymodule]
fn _core(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyWx1File>()?;
    module.add_class::<PyAsyncWriter>()?;
    module.add_function(wrap_pyfunction!(open_file, module)?)?;
    module.add_function(wrap_pyfunction!(write_array, module)?)?;
    module.add_function(wrap_pyfunction!(unit_dimension, module)?)?;
    module.add_function(wrap_pyfunction!(unit_conversion_factor, module)?)?;
    module.add_function(wrap_pyfunction!(resolve_cf_standard_name, module)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_reader_emits_a_two_dimensional_edge_tile_in_row_major_order() {
        let source: Vec<u8> = (0..24).collect();
        let mut reader = ArrayTileReader::new(&source, &[4, 6], &[2, 3], &[2, 3], 1).unwrap();
        let mut output = Vec::new();
        reader.read_to_end(&mut output).unwrap();
        assert_eq!(output, vec![15, 16, 17, 21, 22, 23]);
    }

    #[test]
    fn tile_reader_emits_three_dimensional_subtiles_in_row_major_order() {
        let source: Vec<u8> = (0..24).collect();
        let mut reader =
            ArrayTileReader::new(&source, &[2, 3, 4], &[0, 1, 1], &[2, 2, 2], 1).unwrap();
        let mut output = Vec::new();
        reader.read_to_end(&mut output).unwrap();
        assert_eq!(output, vec![5, 6, 9, 10, 17, 18, 21, 22]);
    }

    #[test]
    fn chunk_reference_builder_covers_a_rectangular_array() {
        let references = chunk_references(&[4, 5], &[2, 3]).unwrap();
        assert_eq!(references.len(), 4);
        assert_eq!(references[0].origin, vec![0, 0]);
        assert_eq!(references[0].shape, vec![2, 3]);
        assert_eq!(references[1].origin, vec![0, 3]);
        assert_eq!(references[1].shape, vec![2, 2]);
        assert_eq!(references[3].origin, vec![2, 3]);
        assert_eq!(references[3].shape, vec![2, 2]);
    }

    #[test]
    fn chunk_reference_builder_handles_empty_dimensions_and_scalars() {
        assert!(chunk_references(&[0, 5], &[2, 3]).unwrap().is_empty());
        let scalar = chunk_references(&[], &[]).unwrap();
        assert_eq!(scalar.len(), 1);
        assert!(scalar[0].origin.is_empty());
        assert!(scalar[0].shape.is_empty());
    }
}
