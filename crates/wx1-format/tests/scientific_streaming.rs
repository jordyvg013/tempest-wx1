// Copyright 2026 Jordy van Gils
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use wx1_format::{
    open_scientific_snapshot, write_generation_container_from_readers, ArrayChunkRef,
    ArraySelection, AttributeValue, ChunkCodec, ChunkIoError, ChunkReaderInput, ChunkWriteOptions,
    DataType, Dimension, DimensionRole, GridMapping, HeaderByteOrder, Metadata, PayloadByteOrder,
    Provenance, ScientificDataset, ScientificModelError, Variable,
};

struct InterruptOnceReader {
    inner: Cursor<Vec<u8>>,
    interrupt_next_read: bool,
}

impl Read for InterruptOnceReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.interrupt_next_read {
            self.interrupt_next_read = false;
            Err(std::io::Error::from(std::io::ErrorKind::Interrupted))
        } else {
            self.inner.read(buffer)
        }
    }
}

fn scientific_model() -> ScientificDataset {
    let mut attributes = BTreeMap::new();
    attributes.insert("units".to_owned(), AttributeValue::Text("K".to_owned()));
    attributes.insert(
        "standard_name".to_owned(),
        AttributeValue::Text("air_temperature".to_owned()),
    );

    ScientificDataset {
        dimensions: vec![Dimension {
            name: "x".to_owned(),
            length: 6,
            role: DimensionRole::Longitude,
        }],
        variables: vec![Variable {
            name: "temperature".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::F32,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["x".to_owned()],
            chunk_shape: vec![2],
            fill_value: None,
            missing_value: None,
            attributes,
            chunks: vec![
                ArrayChunkRef {
                    chunk_id: 10,
                    origin: vec![0],
                    shape: vec![2],
                },
                ArrayChunkRef {
                    chunk_id: 11,
                    origin: vec![2],
                    shape: vec![2],
                },
                ArrayChunkRef {
                    chunk_id: 12,
                    origin: vec![4],
                    shape: vec![2],
                },
            ],
            grid_mapping: Some("crs".to_owned()),
        }],
        grid_mappings: vec![GridMapping {
            name: "crs".to_owned(),
            mapping_name: "latitude_longitude".to_owned(),
            crs_wkt: Some("GEOGCRS[WGS 84]".to_owned()),
            epsg_code: Some("EPSG:4326".to_owned()),
            axis_order: vec!["x".to_owned()],
            attributes: BTreeMap::new(),
        }],
        provenance: vec![Provenance {
            source: "scientific-streaming-test".to_owned(),
            provider: Some("WX-1".to_owned()),
            product: Some("synthetic temperature".to_owned()),
            institution: None,
            license: Some("CC0-1.0".to_owned()),
            history: None,
            reference_time: None,
        }],
        ..ScientificDataset::default()
    }
}

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

#[test]
fn generation_stream_writer_roundtrips_all_codecs_and_big_endian_header() {
    let first = [f32_bytes(&[1.0, 2.0]), vec![0xAA, 0xBB]].concat();
    let second = f32_bytes(&[3.0, 4.0]);
    let third = f32_bytes(&[5.0, 6.0]);
    let mut first_reader = Cursor::new(first);
    let mut second_reader = Cursor::new(second);
    let mut third_reader = Cursor::new(third);
    let metadata = Metadata::new("streaming-scientific").with_scientific_model(scientific_model());
    let mut file = Cursor::new(Vec::new());

    {
        let mut chunks = [
            ChunkReaderInput {
                id: 10,
                reader: &mut first_reader,
                raw_len: 8,
                codec: ChunkCodec::Raw,
            },
            ChunkReaderInput {
                id: 11,
                reader: &mut second_reader,
                raw_len: 8,
                codec: ChunkCodec::Zstandard,
            },
            ChunkReaderInput {
                id: 12,
                reader: &mut third_reader,
                raw_len: 8,
                codec: ChunkCodec::Lz4,
            },
        ];
        write_generation_container_from_readers(
            &mut file,
            &metadata,
            &mut chunks,
            HeaderByteOrder::BigEndian,
            ChunkWriteOptions { buffer_size: 3 },
        )
        .unwrap();
    }

    assert_eq!(first_reader.position(), 8);
    assert_eq!(second_reader.position(), 8);
    assert_eq!(third_reader.position(), 8);

    let snapshot = open_scientific_snapshot(&mut file).unwrap();
    assert_eq!(snapshot.generation(), 1);
    let array = snapshot
        .read_array_slice(
            &mut file,
            "/temperature",
            &ArraySelection::new(vec![1], vec![4]),
        )
        .unwrap();
    assert_eq!(array.shape, vec![4]);
    assert_eq!(array.data_type, DataType::F32);
    assert_eq!(array.byte_order, PayloadByteOrder::LittleEndian);
    assert_eq!(array.data, f32_bytes(&[2.0, 3.0, 4.0, 5.0]));
}

#[test]
fn stream_writer_rejects_scientific_length_mismatch_before_writing() {
    let payload = [1u8, 2, 3];
    let mut reader = Cursor::new(payload);
    let mut chunks = [ChunkReaderInput {
        id: 10,
        reader: &mut reader,
        raw_len: 3,
        codec: ChunkCodec::Raw,
    }];
    let metadata = Metadata::new("length-mismatch").with_scientific_model(scientific_model());
    let mut file = Cursor::new(Vec::new());

    let result = write_generation_container_from_readers(
        &mut file,
        &metadata,
        &mut chunks,
        HeaderByteOrder::LittleEndian,
        ChunkWriteOptions::default(),
    );
    assert!(matches!(
        result,
        Err(ChunkIoError::ScientificModel(
            ScientificModelError::ChunkLengthMismatch
        ))
    ));
    assert!(file.get_ref().is_empty());
}

#[test]
fn stream_writer_rejects_invalid_boolean_values_without_committing_footer() {
    let model = ScientificDataset {
        dimensions: vec![Dimension {
            name: "sample".to_owned(),
            length: 2,
            role: DimensionRole::Other,
        }],
        variables: vec![Variable {
            name: "mask".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::Bool8,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["sample".to_owned()],
            chunk_shape: vec![2],
            fill_value: None,
            missing_value: None,
            attributes: BTreeMap::new(),
            chunks: vec![ArrayChunkRef {
                chunk_id: 1,
                origin: vec![0],
                shape: vec![2],
            }],
            grid_mapping: None,
        }],
        ..ScientificDataset::default()
    };
    let mut reader = Cursor::new(vec![0u8, 2]);
    let mut chunks = [ChunkReaderInput {
        id: 1,
        reader: &mut reader,
        raw_len: 2,
        codec: ChunkCodec::Zstandard,
    }];
    let metadata = Metadata::new("invalid-bool").with_scientific_model(model);
    let mut file = Cursor::new(Vec::new());

    let result = write_generation_container_from_readers(
        &mut file,
        &metadata,
        &mut chunks,
        HeaderByteOrder::LittleEndian,
        ChunkWriteOptions { buffer_size: 1 },
    );
    assert!(matches!(
        result,
        Err(ChunkIoError::ScientificModel(
            ScientificModelError::InvalidBooleanValue
        ))
    ));
    assert!(!file.get_ref().windows(4).any(|bytes| bytes == b"WX1G"));
}

#[test]
fn stream_writer_rejects_invalid_buffer_before_touching_destination() {
    let payload = [1u8, 2];
    let mut reader = Cursor::new(payload);
    let mut file = Cursor::new(Vec::new());
    let result = {
        let mut chunks = [ChunkReaderInput {
            id: 1,
            reader: &mut reader,
            raw_len: 2,
            codec: ChunkCodec::Raw,
        }];
        write_generation_container_from_readers(
            &mut file,
            &Metadata::new("invalid-buffer"),
            &mut chunks,
            HeaderByteOrder::LittleEndian,
            ChunkWriteOptions { buffer_size: 0 },
        )
    };
    assert!(matches!(result, Err(ChunkIoError::InvalidStreamBufferSize)));
    assert!(ChunkWriteOptions {
        buffer_size: wx1_format::MAX_STREAM_WRITE_BUFFER_BYTES + 1,
    }
    .validate()
    .is_err());
    assert!(file.get_ref().is_empty());
    assert_eq!(reader.position(), 0);

    let mut oversized_reader = Cursor::new(Vec::<u8>::new());
    let oversized_result = {
        let mut oversized_chunks = [ChunkReaderInput {
            id: 2,
            reader: &mut oversized_reader,
            raw_len: wx1_format::MAX_CHUNK_BYTES + 1,
            codec: ChunkCodec::Raw,
        }];
        write_generation_container_from_readers(
            &mut file,
            &Metadata::new("oversized-chunk"),
            &mut oversized_chunks,
            HeaderByteOrder::LittleEndian,
            ChunkWriteOptions::default(),
        )
    };
    assert!(matches!(oversized_result, Err(ChunkIoError::ChunkTooLarge)));
    assert!(file.get_ref().is_empty());
    assert_eq!(oversized_reader.position(), 0);
}

#[test]
fn stream_writer_rejects_short_source_without_writing_a_commit_footer() {
    let mut reader = Cursor::new(vec![1u8, 2]);
    let mut chunks = [ChunkReaderInput {
        id: 1,
        reader: &mut reader,
        raw_len: 3,
        codec: ChunkCodec::Raw,
    }];
    let mut file = Cursor::new(Vec::new());
    let result = write_generation_container_from_readers(
        &mut file,
        &Metadata::new("short-input"),
        &mut chunks,
        HeaderByteOrder::LittleEndian,
        ChunkWriteOptions { buffer_size: 1 },
    );
    assert!(result.is_err());
    assert!(!file.get_ref().windows(4).any(|bytes| bytes == b"WX1G"));
}

#[test]
fn reader_writer_retries_interrupted_source_reads() {
    let payload = b"interrupted once".to_vec();
    let mut reader = InterruptOnceReader {
        inner: Cursor::new(payload.clone()),
        interrupt_next_read: true,
    };
    let mut file = Cursor::new(Vec::new());
    {
        let mut chunks = [ChunkReaderInput {
            id: 44,
            reader: &mut reader,
            raw_len: payload.len() as u64,
            codec: ChunkCodec::Raw,
        }];
        write_generation_container_from_readers(
            &mut file,
            &Metadata::new("interrupted-read"),
            &mut chunks,
            HeaderByteOrder::LittleEndian,
            ChunkWriteOptions { buffer_size: 4 },
        )
        .unwrap();
    }
    let snapshot = wx1_format::open_generation_snapshot(&mut file).unwrap();
    assert_eq!(snapshot.read_chunk(&mut file, 44).unwrap(), payload);
}

#[test]
fn atomic_reader_writer_removes_temporary_file_on_invalid_scientific_payload() {
    let output = std::env::temp_dir().join(format!(
        "wx1-stream-atomic-failure-{}.wx1",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    let temp = output.with_file_name(format!(
        "{}.wx1-generation-stream-tmp-{}",
        output.file_name().unwrap().to_string_lossy(),
        std::process::id()
    ));
    let _ = std::fs::remove_file(&temp);

    let model = ScientificDataset {
        dimensions: vec![Dimension {
            name: "sample".to_owned(),
            length: 2,
            role: DimensionRole::Other,
        }],
        variables: vec![Variable {
            name: "mask".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::Bool8,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["sample".to_owned()],
            chunk_shape: vec![2],
            fill_value: None,
            missing_value: None,
            attributes: BTreeMap::new(),
            chunks: vec![ArrayChunkRef {
                chunk_id: 1,
                origin: vec![0],
                shape: vec![2],
            }],
            grid_mapping: None,
        }],
        ..ScientificDataset::default()
    };
    let mut reader = Cursor::new(vec![0u8, 2]);
    let mut chunks = [ChunkReaderInput {
        id: 1,
        reader: &mut reader,
        raw_len: 2,
        codec: ChunkCodec::Raw,
    }];
    let metadata = Metadata::new("atomic-invalid-bool").with_scientific_model(model);

    let result = wx1_format::write_generation_container_from_readers_atomic(
        &output,
        &metadata,
        &mut chunks,
        HeaderByteOrder::LittleEndian,
        ChunkWriteOptions::default(),
    );
    assert!(matches!(
        result,
        Err(ChunkIoError::ScientificModel(
            ScientificModelError::InvalidBooleanValue
        ))
    ));
    assert!(!output.exists());
    assert!(!temp.exists());
}
