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

use std::io::{Cursor, Seek, SeekFrom, Write};

use wx1_format::{
    append_generation, append_scientific_generation, open_generation_snapshot,
    open_scientific_snapshot, open_scientific_snapshot_lazy, repair_generation_file,
    write_chunk_container_from_readers, write_generation_container, ArrayChunkRef, ArraySelection,
    AttributeValue, ChunkCodec, ChunkIoError, ChunkReaderInput, Coordinate, CoordinateAxis,
    DataType, Dimension, DimensionRole, EncodedChunkInput, GridMapping, Group, HeaderByteOrder,
    Metadata, MetadataError, PayloadByteOrder, Provenance, ScalarValue, ScientificDataset,
    ScientificModelError, TimeCalendar, TimeCoordinate, TimePrecision, Variable,
    VerticalCoordinate, VerticalDirection, VerticalKind,
};

fn u16_bytes(values: &[u16], order: PayloadByteOrder) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 2);
    for value in values {
        bytes.extend_from_slice(&match order {
            PayloadByteOrder::LittleEndian => value.to_le_bytes(),
            PayloadByteOrder::BigEndian => value.to_be_bytes(),
        });
    }
    bytes
}

fn scientific_model() -> ScientificDataset {
    let mut group_attributes = std::collections::BTreeMap::new();
    group_attributes.insert(
        "title".to_owned(),
        AttributeValue::Text("Atmosphere".to_owned()),
    );

    let mut variable_attributes = std::collections::BTreeMap::new();
    variable_attributes.insert("units".to_owned(), AttributeValue::Text("K".to_owned()));
    variable_attributes.insert(
        "standard_name".to_owned(),
        AttributeValue::Text("air_temperature".to_owned()),
    );
    variable_attributes.insert(
        "long_name".to_owned(),
        AttributeValue::Text("air temperature".to_owned()),
    );

    let mut model = ScientificDataset::new();
    model.groups.push(Group {
        path: "/atmosphere".to_owned(),
        attributes: group_attributes,
    });
    model.dimensions = vec![
        Dimension {
            name: "x".to_owned(),
            length: 5,
            role: DimensionRole::Longitude,
        },
        Dimension {
            name: "y".to_owned(),
            length: 3,
            role: DimensionRole::Latitude,
        },
        Dimension {
            name: "time".to_owned(),
            length: 2,
            role: DimensionRole::Time,
        },
        Dimension {
            name: "level".to_owned(),
            length: 1,
            role: DimensionRole::Vertical,
        },
    ];
    model.variables = vec![
        Variable {
            name: "temperature".to_owned(),
            group: "/atmosphere".to_owned(),
            data_type: DataType::U16,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["y".to_owned(), "x".to_owned()],
            chunk_shape: vec![2, 3],
            fill_value: Some(ScalarValue::U16(u16::MAX)),
            missing_value: Some(ScalarValue::U16(u16::MAX - 1)),
            attributes: variable_attributes,
            grid_mapping: Some("crs".to_owned()),
            chunks: vec![
                ArrayChunkRef {
                    chunk_id: 1,
                    origin: vec![0, 0],
                    shape: vec![2, 3],
                },
                ArrayChunkRef {
                    chunk_id: 2,
                    origin: vec![0, 3],
                    shape: vec![2, 2],
                },
                ArrayChunkRef {
                    chunk_id: 3,
                    origin: vec![2, 0],
                    shape: vec![1, 3],
                },
                ArrayChunkRef {
                    chunk_id: 4,
                    origin: vec![2, 3],
                    shape: vec![1, 2],
                },
            ],
        },
        Variable {
            name: "level".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::F32,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["level".to_owned()],
            chunk_shape: vec![1],
            fill_value: None,
            missing_value: None,
            attributes: std::collections::BTreeMap::new(),
            grid_mapping: None,
            chunks: vec![ArrayChunkRef {
                chunk_id: 5,
                origin: vec![0],
                shape: vec![1],
            }],
        },
        Variable {
            name: "time".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::I64,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["time".to_owned()],
            chunk_shape: vec![2],
            fill_value: None,
            missing_value: None,
            attributes: std::collections::BTreeMap::new(),
            grid_mapping: None,
            chunks: vec![ArrayChunkRef {
                chunk_id: 6,
                origin: vec![0],
                shape: vec![2],
            }],
        },
    ];
    model.coordinates = vec![
        Coordinate {
            name: "time".to_owned(),
            variable: "/time".to_owned(),
            axis: CoordinateAxis::Time,
            time: Some(TimeCoordinate {
                calendar: TimeCalendar::ProlepticGregorian,
                units: "seconds since 2026-01-01T00:00:00Z".to_owned(),
                epoch: "2026-01-01T00:00:00Z".to_owned(),
                timezone: "UTC".to_owned(),
                precision: TimePrecision::Second,
            }),
            vertical: None,
            longitude_convention: None,
        },
        Coordinate {
            name: "level".to_owned(),
            variable: "/level".to_owned(),
            axis: CoordinateAxis::Vertical,
            time: None,
            vertical: Some(VerticalCoordinate {
                kind: VerticalKind::Pressure,
                units: "hPa".to_owned(),
                positive: VerticalDirection::Down,
            }),
            longitude_convention: None,
        },
    ];
    model.grid_mappings.push(GridMapping {
        name: "crs".to_owned(),
        mapping_name: "latitude_longitude".to_owned(),
        crs_wkt: Some("GEOGCRS[WGS 84]".to_owned()),
        epsg_code: Some("EPSG:4326".to_owned()),
        axis_order: vec!["y".to_owned(), "x".to_owned()],
        attributes: std::collections::BTreeMap::new(),
    });
    model.provenance.push(Provenance {
        source: "deterministic-test-fixture".to_owned(),
        provider: Some("WX-1 tests".to_owned()),
        product: Some("synthetic temperature".to_owned()),
        institution: None,
        license: Some("CC0-1.0".to_owned()),
        history: Some("created for test".to_owned()),
        reference_time: None,
    });
    model
}

fn payloads() -> Vec<Vec<u8>> {
    vec![
        u16_bytes(&[0, 1, 2, 5, 6, 7], PayloadByteOrder::LittleEndian),
        u16_bytes(&[3, 4, 8, 9], PayloadByteOrder::LittleEndian),
        u16_bytes(&[10, 11, 12], PayloadByteOrder::LittleEndian),
        u16_bytes(&[13, 14], PayloadByteOrder::LittleEndian),
        850.0f32.to_le_bytes().to_vec(),
        [0i64.to_le_bytes(), 3600i64.to_le_bytes()].concat(),
    ]
}

#[test]
fn metadata_schema_v2_roundtrips_scientific_model_and_v1_stays_unchanged() {
    let legacy = Metadata::new("schema-v1");
    let legacy_bytes = legacy.to_cbor().unwrap();
    assert_eq!(legacy_bytes[0], 0xa3);
    assert_eq!(Metadata::from_cbor(&legacy_bytes).unwrap(), legacy);

    let metadata = Metadata::new("scientific-v2").with_scientific_model(scientific_model());
    let encoded = metadata.to_cbor().unwrap();
    assert_eq!(encoded[0], 0xa4);
    assert_eq!(encoded[2], 2);
    let decoded = Metadata::from_cbor(&encoded).unwrap();
    assert_eq!(
        decoded.scientific_model().unwrap().to_cbor().unwrap(),
        metadata.scientific_model().unwrap().to_cbor().unwrap()
    );
    assert_eq!(decoded.attributes, metadata.attributes);
}

#[test]
fn multidimensional_slice_reads_only_intersecting_edge_chunks() {
    let raw = payloads();
    let chunks: Vec<_> = raw
        .iter()
        .enumerate()
        .map(|(index, bytes)| EncodedChunkInput {
            id: index as u64 + 1,
            data: bytes,
            codec: ChunkCodec::Raw,
        })
        .collect();
    let metadata = Metadata::new("array-roundtrip").with_scientific_model(scientific_model());
    let mut file = Cursor::new(Vec::new());
    write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
        .unwrap();

    let snapshot = open_scientific_snapshot(&mut file).unwrap();
    assert_eq!(snapshot.generation(), 1);
    assert_eq!(snapshot.dataset().variables.len(), 3);
    assert_eq!(snapshot.dataset().coordinates.len(), 2);
    let array = snapshot
        .read_array_slice(
            &mut file,
            "/atmosphere/temperature",
            &ArraySelection::new(vec![1, 1], vec![2, 3]),
        )
        .unwrap();
    assert_eq!(array.shape, vec![2, 3]);
    assert_eq!(array.data_type, DataType::U16);
    assert_eq!(array.byte_order, PayloadByteOrder::LittleEndian);
    assert_eq!(
        array.data,
        u16_bytes(&[6, 7, 8, 11, 12, 13], PayloadByteOrder::LittleEndian)
    );
}

#[test]
fn payload_endianness_is_explicit_and_independent_of_container_header() {
    let model = ScientificDataset {
        dimensions: vec![Dimension {
            name: "x".to_owned(),
            length: 2,
            role: DimensionRole::Other,
        }],
        variables: vec![Variable {
            name: "values".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::U16,
            byte_order: PayloadByteOrder::BigEndian,
            dimensions: vec!["x".to_owned()],
            chunk_shape: vec![2],
            fill_value: None,
            missing_value: None,
            attributes: std::collections::BTreeMap::new(),
            chunks: vec![ArrayChunkRef {
                chunk_id: 77,
                origin: vec![0],
                shape: vec![2],
            }],
            grid_mapping: None,
        }],
        ..ScientificDataset::default()
    };
    let payload = u16_bytes(&[0x1234, 0x5678], PayloadByteOrder::BigEndian);
    let chunks = [EncodedChunkInput {
        id: 77,
        data: &payload,
        codec: ChunkCodec::Raw,
    }];
    let metadata = Metadata::new("big-endian-array").with_scientific_model(model);
    let mut file = Cursor::new(Vec::new());
    write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
        .unwrap();
    let snapshot = open_scientific_snapshot(&mut file).unwrap();
    let array = snapshot
        .read_array_slice(&mut file, "/values", &ArraySelection::new(vec![0], vec![2]))
        .unwrap();
    assert_eq!(array.byte_order, PayloadByteOrder::BigEndian);
    assert_eq!(array.data, payload);
}

#[test]
fn rejects_invalid_chunk_coverage_and_overflowing_shapes() {
    let mut model = scientific_model();
    model.variables[0].chunks.pop();
    assert_eq!(model.validate(), Err(ScientificModelError::ChunkCoverage));

    let overflow = ScientificDataset {
        dimensions: vec![
            Dimension {
                name: "a".to_owned(),
                length: u64::MAX,
                role: DimensionRole::Other,
            },
            Dimension {
                name: "b".to_owned(),
                length: 2,
                role: DimensionRole::Other,
            },
        ],
        variables: vec![Variable {
            name: "bad".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::U8,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["a".to_owned(), "b".to_owned()],
            chunk_shape: vec![u64::MAX, 2],
            fill_value: None,
            missing_value: None,
            attributes: std::collections::BTreeMap::new(),
            chunks: vec![ArrayChunkRef {
                chunk_id: 1,
                origin: vec![0, 0],
                shape: vec![u64::MAX, 2],
            }],
            grid_mapping: None,
        }],
        ..ScientificDataset::default()
    };
    assert_eq!(
        overflow.validate(),
        Err(ScientificModelError::DimensionProductOverflow)
    );
}

#[test]
fn writer_rejects_missing_or_wrong_sized_array_chunks_before_writing() {
    let model = scientific_model();
    let metadata = Metadata::new("missing-array-chunk").with_scientific_model(model.clone());
    let mut file = Cursor::new(Vec::new());
    assert!(matches!(
        write_generation_container(&mut file, &metadata, &[], HeaderByteOrder::LittleEndian),
        Err(ChunkIoError::ScientificModel(
            ScientificModelError::MissingChunk
        ))
    ));
    assert!(file.get_ref().is_empty());

    let bad = vec![0u8; 1];
    let chunks = [EncodedChunkInput {
        id: 1,
        data: &bad,
        codec: ChunkCodec::Raw,
    }];
    assert!(matches!(
        write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian),
        Err(ChunkIoError::ScientificModel(
            ScientificModelError::ChunkLengthMismatch
        ))
    ));
    assert!(file.get_ref().is_empty());
}

#[test]
fn array_slice_bounds_are_checked() {
    let raw = payloads();
    let chunks: Vec<_> = raw
        .iter()
        .enumerate()
        .map(|(index, bytes)| EncodedChunkInput {
            id: index as u64 + 1,
            data: bytes,
            codec: ChunkCodec::Raw,
        })
        .collect();
    let metadata = Metadata::new("array-bounds").with_scientific_model(scientific_model());
    let mut file = Cursor::new(Vec::new());
    write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
        .unwrap();
    let snapshot = open_scientific_snapshot(&mut file).unwrap();
    assert!(matches!(
        snapshot.read_array_slice(
            &mut file,
            "/atmosphere/temperature",
            &ArraySelection::new(vec![2, 4], vec![2, 2])
        ),
        Err(ChunkIoError::ScientificModel(
            ScientificModelError::InvalidSelection
        ))
    ));
}

#[test]
fn scalar_and_zero_length_arrays_have_explicit_chunk_semantics() {
    let model = ScientificDataset {
        dimensions: vec![
            Dimension {
                name: "huge".to_owned(),
                length: u64::MAX,
                role: DimensionRole::Other,
            },
            Dimension {
                name: "two".to_owned(),
                length: 2,
                role: DimensionRole::Other,
            },
            Dimension {
                name: "empty".to_owned(),
                length: 0,
                role: DimensionRole::Other,
            },
        ],
        variables: vec![
            Variable {
                name: "constant".to_owned(),
                group: "/".to_owned(),
                data_type: DataType::I32,
                byte_order: PayloadByteOrder::LittleEndian,
                dimensions: vec![],
                chunk_shape: vec![],
                fill_value: Some(ScalarValue::I32(-1)),
                missing_value: None,
                attributes: std::collections::BTreeMap::new(),
                chunks: vec![ArrayChunkRef {
                    chunk_id: 90,
                    origin: vec![],
                    shape: vec![],
                }],
                grid_mapping: None,
            },
            Variable {
                name: "empty_array".to_owned(),
                group: "/".to_owned(),
                data_type: DataType::U8,
                byte_order: PayloadByteOrder::LittleEndian,
                dimensions: vec!["huge".to_owned(), "two".to_owned(), "empty".to_owned()],
                chunk_shape: vec![u64::MAX, 2, 1],
                fill_value: None,
                missing_value: None,
                attributes: std::collections::BTreeMap::new(),
                chunks: vec![],
                grid_mapping: None,
            },
        ],
        ..ScientificDataset::default()
    };
    assert_eq!(model.variables[1].element_count(&model).unwrap(), 0);
    model.validate().unwrap();

    let scalar_payload = (-42i32).to_le_bytes();
    let chunks = [EncodedChunkInput {
        id: 90,
        data: &scalar_payload,
        codec: ChunkCodec::Raw,
    }];
    let metadata = Metadata::new("scalar-and-empty").with_scientific_model(model);
    let mut file = Cursor::new(Vec::new());
    write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
        .unwrap();
    let snapshot = open_scientific_snapshot(&mut file).unwrap();

    let scalar = snapshot
        .read_array_slice(&mut file, "/constant", &ArraySelection::new(vec![], vec![]))
        .unwrap();
    assert!(scalar.shape.is_empty());
    assert_eq!(scalar.data, scalar_payload);

    let empty = snapshot
        .read_array_slice(
            &mut file,
            "/empty_array",
            &ArraySelection::new(vec![0, 0, 0], vec![0, 2, 0]),
        )
        .unwrap();
    assert_eq!(empty.shape, vec![0, 2, 0]);
    assert!(empty.data.is_empty());
}

#[test]
fn scientific_files_reject_legacy_writers_and_append_without_mutation() {
    let model = scientific_model();
    let metadata = Metadata::new("immutable-scientific-manifest").with_scientific_model(model);
    let mut file = Cursor::new(Vec::new());
    let raw = payloads();
    let chunks: Vec<_> = raw
        .iter()
        .enumerate()
        .map(|(index, bytes)| EncodedChunkInput {
            id: index as u64 + 1,
            data: bytes,
            codec: ChunkCodec::Raw,
        })
        .collect();
    write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
        .unwrap();

    let before_append = file.get_ref().clone();
    let appended_data = b"not-in-manifest";
    let appended = [EncodedChunkInput {
        id: 99,
        data: appended_data,
        codec: ChunkCodec::Raw,
    }];
    assert!(matches!(
        append_generation(&mut file, &appended),
        Err(ChunkIoError::ScientificModelCannotAppend)
    ));
    assert_eq!(file.get_ref(), &before_append);

    let mut payload_reader = Cursor::new(b"stream".to_vec());
    let mut reader_chunks = [ChunkReaderInput {
        id: 100,
        reader: &mut payload_reader,
        raw_len: 6,
        codec: ChunkCodec::Raw,
    }];
    let mut legacy_file = Cursor::new(Vec::new());
    assert!(matches!(
        write_chunk_container_from_readers(
            &mut legacy_file,
            &metadata,
            &mut reader_chunks,
            HeaderByteOrder::LittleEndian,
            4096,
        ),
        Err(ChunkIoError::Metadata(
            MetadataError::ScientificModelRequiresGeneration
        ))
    ));
    assert!(legacy_file.get_ref().is_empty());
}

#[test]
fn large_multidimensional_array_plans_only_intersecting_small_chunks() {
    const SIDE: u64 = 64;
    const TILE: u64 = 4;
    let mut payloads = Vec::new();
    let mut chunk_refs = Vec::new();

    for y_origin in (0..SIDE).step_by(TILE as usize) {
        for x_origin in (0..SIDE).step_by(TILE as usize) {
            let mut values = Vec::with_capacity((TILE * TILE) as usize);
            for y in y_origin..y_origin + TILE {
                for x in x_origin..x_origin + TILE {
                    values.push((y * SIDE + x) as u16);
                }
            }
            payloads.push(u16_bytes(&values, PayloadByteOrder::LittleEndian));
            let chunk_id = (y_origin / TILE) * (SIDE / TILE) + (x_origin / TILE) + 1;
            chunk_refs.push(ArrayChunkRef {
                chunk_id,
                origin: vec![y_origin, x_origin],
                shape: vec![TILE, TILE],
            });
        }
    }

    assert_eq!(payloads.len(), 256);
    let model = ScientificDataset {
        dimensions: vec![
            Dimension {
                name: "y".to_owned(),
                length: SIDE,
                role: DimensionRole::Latitude,
            },
            Dimension {
                name: "x".to_owned(),
                length: SIDE,
                role: DimensionRole::Longitude,
            },
        ],
        variables: vec![Variable {
            name: "temperature".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::U16,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["y".to_owned(), "x".to_owned()],
            chunk_shape: vec![TILE, TILE],
            fill_value: None,
            missing_value: None,
            attributes: std::collections::BTreeMap::new(),
            chunks: chunk_refs,
            grid_mapping: None,
        }],
        ..ScientificDataset::default()
    };
    let chunks: Vec<_> = payloads
        .iter()
        .enumerate()
        .map(|(index, bytes)| EncodedChunkInput {
            id: index as u64 + 1,
            data: bytes,
            codec: if index % 2 == 0 {
                ChunkCodec::Raw
            } else {
                ChunkCodec::Zstandard
            },
        })
        .collect();
    let metadata = Metadata::new("large-many-small-chunks").with_scientific_model(model);
    let mut file = Cursor::new(Vec::new());
    write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
        .unwrap();

    let snapshot = open_scientific_snapshot(&mut file).unwrap();
    let selection = ArraySelection::new(vec![3, 3], vec![3, 3]);
    let plan = snapshot
        .plan_array_slice("/temperature", &selection)
        .unwrap();
    assert_eq!(plan.start, vec![3, 3]);
    assert_eq!(plan.shape, vec![3, 3]);
    assert_eq!(plan.output_bytes, 18);
    assert_eq!(plan.chunk_ids, vec![1, 2, 17, 18]);

    let array = snapshot
        .read_array_slice(&mut file, "/temperature", &selection)
        .unwrap();
    assert_eq!(array.data_type, DataType::U16);
    assert_eq!(array.shape, vec![3, 3]);
    assert_eq!(
        array.data,
        u16_bytes(
            &[195, 196, 197, 259, 260, 261, 323, 324, 325],
            PayloadByteOrder::LittleEndian
        )
    );
}

#[test]
fn scientific_stored_byte_corruption_is_rejected_without_repair_mutation() {
    let path = std::env::temp_dir().join(format!(
        "wx1-scientific-corruption-{}.wx1",
        std::process::id(),
    ));
    let _ = std::fs::remove_file(&path);
    let metadata = Metadata::new("scientific-corruption").with_scientific_model(scientific_model());
    let payloads = payloads();
    let chunks: Vec<_> = payloads
        .iter()
        .enumerate()
        .map(|(index, bytes)| EncodedChunkInput {
            id: index as u64 + 1,
            data: bytes,
            codec: ChunkCodec::Raw,
        })
        .collect();
    let mut file = Cursor::new(Vec::new());
    write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
        .unwrap();

    let snapshot = open_generation_snapshot(&mut file).unwrap();
    let descriptor = snapshot.chunk(1).unwrap();
    let mut damaged = file.into_inner();
    damaged[descriptor.offset as usize] ^= 0x40;
    std::fs::write(&path, &damaged).unwrap();
    let before_repair = std::fs::read(&path).unwrap();

    let mut damaged_reader = std::fs::File::open(&path).unwrap();
    assert!(matches!(
        open_scientific_snapshot(&mut damaged_reader),
        Err(ChunkIoError::ChecksumMismatch(1))
    ));
    drop(damaged_reader);
    assert!(repair_generation_file(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before_repair);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn scientific_container_torn_tail_is_preserved_until_explicit_repair() {
    let path =
        std::env::temp_dir().join(format!("wx1-scientific-repair-{}.wx1", std::process::id(),));
    let _ = std::fs::remove_file(&path);
    let metadata = Metadata::new("scientific-torn-tail").with_scientific_model(scientific_model());
    let payloads = payloads();
    let chunks: Vec<_> = payloads
        .iter()
        .enumerate()
        .map(|(index, bytes)| EncodedChunkInput {
            id: index as u64 + 1,
            data: bytes,
            codec: ChunkCodec::Raw,
        })
        .collect();

    let mut file = std::fs::File::create(&path).unwrap();
    write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
        .unwrap();
    file.flush().unwrap();
    let committed_len = file.metadata().unwrap().len();
    file.seek(std::io::SeekFrom::End(0)).unwrap();
    file.write_all(b"WX1-torn-uncommitted-tail").unwrap();
    file.sync_all().unwrap();
    drop(file);

    let report = repair_generation_file(&path).unwrap();
    assert_eq!(report.generation, 1);
    assert_eq!(report.retained_len, committed_len);
    assert!(report.original_len > report.retained_len);
    assert_eq!(std::fs::metadata(&path).unwrap().len(), committed_len);

    let mut repaired = std::fs::File::open(&path).unwrap();
    let snapshot = open_scientific_snapshot(&mut repaired).unwrap();
    assert_eq!(snapshot.generation(), 1);
    drop(repaired);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn adr0034_boundary_rejects_max_chunks_plus_one_before_allocation() {
    use std::collections::BTreeMap;
    use wx1_format::{
        DataType, Dimension, DimensionRole, PayloadByteOrder, ScientificDataset, Variable,
        MAX_CHUNKS,
    };

    let side = MAX_CHUNKS + 1;
    let dataset = ScientificDataset {
        dimensions: vec![Dimension {
            name: "x".to_owned(),
            length: side,
            role: DimensionRole::Other,
        }],
        variables: vec![Variable {
            name: "oversized".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::F32,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["x".to_owned()],
            chunk_shape: vec![1],
            fill_value: None,
            missing_value: None,
            attributes: BTreeMap::new(),
            chunks: Vec::new(),
            grid_mapping: None,
        }],
        ..ScientificDataset::default()
    };
    assert!(dataset.validate().is_err());
}

#[test]
fn adr0034_metadata_size_limits_accept_above_16mib_and_reject_above_32mib() {
    use std::collections::BTreeMap;
    use wx1_format::{
        ArrayChunkRef, AttributeValue, DataType, Dimension, DimensionRole, PayloadByteOrder,
        ScientificDataset, Variable, MAX_METADATA_BYTES,
    };

    let mut attrs = BTreeMap::new();
    let large_text = "a".repeat(18 * 1024 * 1024);
    attrs.insert("large_desc".to_owned(), AttributeValue::Text(large_text));

    let dataset = ScientificDataset {
        dimensions: vec![Dimension {
            name: "x".to_owned(),
            length: 1,
            role: DimensionRole::Other,
        }],
        variables: vec![Variable {
            name: "var".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::F32,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["x".to_owned()],
            chunk_shape: vec![1],
            fill_value: None,
            missing_value: None,
            attributes: attrs,
            chunks: vec![ArrayChunkRef {
                chunk_id: 1,
                origin: vec![0],
                shape: vec![1],
            }],
            grid_mapping: None,
        }],
        ..ScientificDataset::default()
    };

    assert!(dataset.validate().is_ok());
    let encoded = dataset.to_cbor().unwrap();
    assert!(encoded.len() > 18 * 1024 * 1024);
    assert!(encoded.len() <= MAX_METADATA_BYTES as usize);

    let too_large_text = "b".repeat(33 * 1024 * 1024);
    let mut attrs_oversized = BTreeMap::new();
    attrs_oversized.insert("huge".to_owned(), AttributeValue::Text(too_large_text));
    let dataset_oversized = ScientificDataset {
        dimensions: vec![Dimension {
            name: "x".to_owned(),
            length: 1,
            role: DimensionRole::Other,
        }],
        variables: vec![Variable {
            name: "huge_var".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::F32,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["x".to_owned()],
            chunk_shape: vec![1],
            fill_value: None,
            missing_value: None,
            attributes: attrs_oversized,
            chunks: vec![ArrayChunkRef {
                chunk_id: 1,
                origin: vec![0],
                shape: vec![1],
            }],
            grid_mapping: None,
        }],
        ..ScientificDataset::default()
    };
    assert!(dataset_oversized.validate().is_err());
}

#[test]
fn adr0023_lazy_open_defers_payload_verification_and_validates_fully_on_demand() {
    let mut file = Cursor::new(Vec::new());
    let mut model = scientific_model();
    model.coordinates.clear();
    model.grid_mappings.clear();
    model.variables.truncate(1);
    model.variables[0].grid_mapping = None;
    model.variables[0].chunk_shape = vec![2, 3];
    model.variables[0].chunks = vec![ArrayChunkRef {
        chunk_id: 1,
        origin: vec![0, 0],
        shape: vec![2, 3],
    }];
    model.dimensions = vec![
        Dimension {
            name: "y".to_owned(),
            length: 2,
            role: DimensionRole::Latitude,
        },
        Dimension {
            name: "x".to_owned(),
            length: 3,
            role: DimensionRole::Longitude,
        },
    ];

    let metadata = Metadata::new("test-lazy-open").with_scientific_model(model);
    let payload = u16_bytes(&[1, 2, 3, 4, 5, 6], PayloadByteOrder::LittleEndian);
    let chunks = [EncodedChunkInput {
        id: 1,
        codec: ChunkCodec::Zstandard,
        data: &payload,
    }];
    write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
        .unwrap();

    // 1. Lazy open on valid file succeeds and validate_full succeeds
    file.seek(SeekFrom::Start(0)).unwrap();
    let snapshot = open_scientific_snapshot_lazy(&mut file).unwrap();
    assert_eq!(snapshot.generation(), 1);
    file.seek(SeekFrom::Start(0)).unwrap();
    assert!(snapshot.validate_full(&mut file).is_ok());

    // 2. Corrupt one byte inside chunk 1 stored payload
    let mut corrupted = file.into_inner();
    let desc = &snapshot.descriptors()[0];
    let corrupt_offset = (desc.offset + desc.stored_len / 2) as usize;
    corrupted[corrupt_offset] ^= 0xff;

    // 3. Eager open MUST fail with ChecksumMismatch
    let mut corrupt_cursor = Cursor::new(corrupted.clone());
    assert!(matches!(
        open_scientific_snapshot(&mut corrupt_cursor),
        Err(ChunkIoError::ChecksumMismatch(1))
    ));

    // 4. Lazy open MUST succeed without error (O(1) footer/dir check only)
    corrupt_cursor.seek(SeekFrom::Start(0)).unwrap();
    let lazy_snapshot = open_scientific_snapshot_lazy(&mut corrupt_cursor).unwrap();
    assert_eq!(lazy_snapshot.generation(), 1);

    // 5. Calling validate_full on the lazy snapshot MUST detect the corruption
    corrupt_cursor.seek(SeekFrom::Start(0)).unwrap();
    assert!(matches!(
        lazy_snapshot.validate_full(&mut corrupt_cursor),
        Err(ChunkIoError::ChecksumMismatch(1))
    ));

    // 6. Reading the corrupt array slice MUST detect the corruption at read time
    corrupt_cursor.seek(SeekFrom::Start(0)).unwrap();
    assert!(matches!(
        lazy_snapshot.read_array_slice(
            &mut corrupt_cursor,
            "/atmosphere/temperature",
            &ArraySelection::new(vec![0, 0], vec![2, 3]),
        ),
        Err(ChunkIoError::ChecksumMismatch(1))
    ));
}

#[test]
fn adr0022_append_scientific_generation_grows_time_axis_and_reads_across_generations() {
    let mut model_g1 = ScientificDataset::new();
    model_g1.dimensions = vec![
        Dimension {
            name: "time".to_owned(),
            length: 1,
            role: DimensionRole::Time,
        },
        Dimension {
            name: "x".to_owned(),
            length: 2,
            role: DimensionRole::Longitude,
        },
    ];
    model_g1.variables = vec![Variable {
        name: "temperature".to_owned(),
        group: "/".to_owned(),
        data_type: DataType::F32,
        byte_order: PayloadByteOrder::LittleEndian,
        dimensions: vec!["time".to_owned(), "x".to_owned()],
        chunk_shape: vec![1, 2],
        fill_value: None,
        missing_value: None,
        attributes: std::collections::BTreeMap::new(),
        grid_mapping: None,
        chunks: vec![ArrayChunkRef {
            chunk_id: 1,
            origin: vec![0, 0],
            shape: vec![1, 2],
        }],
    }];
    let metadata_g1 = Metadata::new("operational-nwp").with_scientific_model(model_g1);

    let chunk1_data = [10.0f32, 20.0f32]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<u8>>();
    let chunks_g1 = [EncodedChunkInput {
        id: 1,
        data: &chunk1_data,
        codec: ChunkCodec::Raw,
    }];

    let mut file = Cursor::new(Vec::new());
    write_generation_container(
        &mut file,
        &metadata_g1,
        &chunks_g1,
        HeaderByteOrder::LittleEndian,
    )
    .unwrap();

    // Verify Generation 1 snapshot
    file.seek(SeekFrom::Start(0)).unwrap();
    let snap_g1 = open_scientific_snapshot(&mut file).unwrap();
    assert_eq!(snap_g1.generation(), 1);
    file.seek(SeekFrom::Start(0)).unwrap();
    let slice_g1 = snap_g1
        .read_array_slice(
            &mut file,
            "/temperature",
            &ArraySelection::new(vec![0, 0], vec![1, 2]),
        )
        .unwrap();
    assert_eq!(slice_g1.shape, vec![1, 2]);
    assert_eq!(slice_g1.data, chunk1_data);

    // Prepare Generation 2: grow time axis to 2 steps, add chunk 2
    let mut model_g2 = ScientificDataset::new();
    model_g2.dimensions = vec![
        Dimension {
            name: "time".to_owned(),
            length: 2,
            role: DimensionRole::Time,
        },
        Dimension {
            name: "x".to_owned(),
            length: 2,
            role: DimensionRole::Longitude,
        },
    ];
    model_g2.variables = vec![Variable {
        name: "temperature".to_owned(),
        group: "/".to_owned(),
        data_type: DataType::F32,
        byte_order: PayloadByteOrder::LittleEndian,
        dimensions: vec!["time".to_owned(), "x".to_owned()],
        chunk_shape: vec![1, 2],
        fill_value: None,
        missing_value: None,
        attributes: std::collections::BTreeMap::new(),
        grid_mapping: None,
        chunks: vec![
            ArrayChunkRef {
                chunk_id: 1,
                origin: vec![0, 0],
                shape: vec![1, 2],
            },
            ArrayChunkRef {
                chunk_id: 2,
                origin: vec![1, 0],
                shape: vec![1, 2],
            },
        ],
    }];
    let metadata_g2 = Metadata::new("operational-nwp").with_scientific_model(model_g2);

    let chunk2_data = [30.0f32, 40.0f32]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<u8>>();
    let chunks_g2 = [EncodedChunkInput {
        id: 2,
        data: &chunk2_data,
        codec: ChunkCodec::Raw,
    }];

    let new_gen = append_scientific_generation(&mut file, &chunks_g2, &metadata_g2).unwrap();
    assert_eq!(new_gen, 2);

    // Open snapshot of Generation 2 (both lazy and eager)
    file.seek(SeekFrom::Start(0)).unwrap();
    let snap_g2 = open_scientific_snapshot_lazy(&mut file).unwrap();
    assert_eq!(snap_g2.generation(), 2);
    assert_eq!(snap_g2.descriptors().len(), 2);

    // Read across both generations in a single 2D selection [0..2, 0..2]
    file.seek(SeekFrom::Start(0)).unwrap();
    let slice_cross_gen = snap_g2
        .read_array_slice(
            &mut file,
            "/temperature",
            &ArraySelection::new(vec![0, 0], vec![2, 2]),
        )
        .unwrap();
    assert_eq!(slice_cross_gen.shape, vec![2, 2]);
    let expected_cross_gen = [10.0f32, 20.0f32, 30.0f32, 40.0f32]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<u8>>();
    assert_eq!(slice_cross_gen.data, expected_cross_gen);

    // Validate full verification across all generations
    file.seek(SeekFrom::Start(0)).unwrap();
    if let Err(err) = snap_g2.validate_full(&mut file) {
        panic!("validate_full failed with: {:?}", err);
    }
}

#[test]
fn adr0022_append_interruption_rolls_back_to_previous_generation_c1() {
    let mut model = ScientificDataset::new();
    model.dimensions = vec![
        Dimension {
            name: "time".to_owned(),
            length: 1,
            role: DimensionRole::Time,
        },
        Dimension {
            name: "x".to_owned(),
            length: 2,
            role: DimensionRole::Longitude,
        },
    ];
    model.variables = vec![Variable {
        name: "temperature".to_owned(),
        group: "/".to_owned(),
        data_type: DataType::F32,
        byte_order: PayloadByteOrder::LittleEndian,
        dimensions: vec!["time".to_owned(), "x".to_owned()],
        chunk_shape: vec![1, 2],
        fill_value: None,
        missing_value: None,
        attributes: std::collections::BTreeMap::new(),
        grid_mapping: None,
        chunks: vec![ArrayChunkRef {
            chunk_id: 1,
            origin: vec![0, 0],
            shape: vec![1, 2],
        }],
    }];
    let metadata = Metadata::new("operational-nwp").with_scientific_model(model);

    let chunk_data = [15.0f32, 25.0f32]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<u8>>();
    let chunks = [EncodedChunkInput {
        id: 1,
        data: &chunk_data,
        codec: ChunkCodec::Raw,
    }];

    let mut file = Cursor::new(Vec::new());
    write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
        .unwrap();

    let committed_len = file.get_ref().len();

    // Simulate an incomplete/crashed append: write partial chunk data without footer
    file.seek(SeekFrom::End(0)).unwrap();
    file.write_all(b"incomplete uncommitted generation chunk data...")
        .unwrap();

    // Reader must transparently roll back to generation 1 without errors
    file.seek(SeekFrom::Start(0)).unwrap();
    let snap = open_scientific_snapshot_lazy(&mut file).unwrap();
    assert_eq!(snap.generation(), 1);
    assert_eq!(snap.valid_end(), committed_len as u64);

    file.seek(SeekFrom::Start(0)).unwrap();
    let slice = snap
        .read_array_slice(
            &mut file,
            "/temperature",
            &ArraySelection::new(vec![0, 0], vec![1, 2]),
        )
        .unwrap();
    assert_eq!(slice.data, chunk_data);
}

#[test]
fn test_linear_planner_baseline_scaling_at_4k_65k_1m() {
    use std::time::Instant;

    println!("\n=== FORMAL LINEAR PLANNER BASELINE (ADR 0028 PRE-MEASUREMENT) ===");
    for &num_chunks in &[4096usize, 65536usize, 1_048_576usize] {
        // Construct regular 2D grid of chunks: side x side
        let side = (num_chunks as f64).sqrt().round() as u64;
        let actual_chunks = (side * side) as usize;
        let chunk_side = 10u64;

        let mut chunk_refs = Vec::with_capacity(actual_chunks);
        let mut id = 1u64;
        for r in 0..side {
            for c in 0..side {
                chunk_refs.push(ArrayChunkRef {
                    chunk_id: id,
                    origin: vec![r * chunk_side, c * chunk_side],
                    shape: vec![chunk_side, chunk_side],
                });
                id += 1;
            }
        }

        let dataset = ScientificDataset {
            dimensions: vec![
                Dimension {
                    name: "y".to_owned(),
                    length: side * chunk_side,
                    role: DimensionRole::Latitude,
                },
                Dimension {
                    name: "x".to_owned(),
                    length: side * chunk_side,
                    role: DimensionRole::Longitude,
                },
            ],
            variables: vec![Variable {
                name: "temperature".to_owned(),
                group: "/".to_owned(),
                data_type: DataType::F32,
                byte_order: PayloadByteOrder::LittleEndian,
                dimensions: vec!["y".to_owned(), "x".to_owned()],
                chunk_shape: vec![chunk_side, chunk_side],
                fill_value: None,
                missing_value: None,
                attributes: std::collections::BTreeMap::new(),
                chunks: chunk_refs,
                grid_mapping: None,
            }],
            ..ScientificDataset::default()
        };

        // Selection that intersects exactly 4 chunks near the middle
        let mid_r = (side / 2) * chunk_side - 5;
        let mid_c = (side / 2) * chunk_side - 5;
        let selection = ArraySelection::new(vec![mid_r, mid_c], vec![10, 10]);

        // Warmup
        let _ = dataset
            .plan_array_slice("/temperature", &selection)
            .unwrap();

        // 100 repetitions for fine nanosecond measurement
        let repetitions = 100;
        let mut times_ns = Vec::with_capacity(repetitions);
        for _ in 0..repetitions {
            let t0 = Instant::now();
            let plan = dataset
                .plan_array_slice("/temperature", &selection)
                .unwrap();
            let elapsed_ns = t0.elapsed().as_nanos() as f64;
            times_ns.push(elapsed_ns);
            assert_eq!(plan.chunk_ids.len(), 4);
        }
        times_ns.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median_ns = times_ns[times_ns.len() / 2];
        let p95_ns = times_ns[(times_ns.len() as f64 * 0.95) as usize];
        let min_ns = times_ns[0];

        println!(
            "Chunks: {:7} (grid {:4}x{:4}) | Plan time: median {:6.0} ns ({:6.3} µs) | p95 {:6.0} ns | min {:6.0} ns | selected: 4",
            actual_chunks,
            side,
            side,
            median_ns,
            median_ns / 1000.0,
            p95_ns,
            min_ns
        );
    }
}

#[test]
fn test_open_scaling_eager_vs_lazy_at_chunk_scales() {
    use std::time::Instant;

    for &num_chunks in &[64usize, 4096usize, 16384usize] {
        let chunk_payload = [1.0f32, 2.0f32]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<u8>>();
        let chunks: Vec<_> = (0..num_chunks)
            .map(|i| EncodedChunkInput {
                id: i as u64 + 1,
                data: &chunk_payload,
                codec: ChunkCodec::Raw,
            })
            .collect();
        let chunk_refs: Vec<_> = (0..num_chunks)
            .map(|i| ArrayChunkRef {
                chunk_id: i as u64 + 1,
                origin: vec![i as u64, 0],
                shape: vec![1, 2],
            })
            .collect();

        let model = ScientificDataset {
            dimensions: vec![
                Dimension {
                    name: "y".to_owned(),
                    length: num_chunks as u64,
                    role: DimensionRole::Latitude,
                },
                Dimension {
                    name: "x".to_owned(),
                    length: 2,
                    role: DimensionRole::Longitude,
                },
            ],
            variables: vec![Variable {
                name: "temp".to_owned(),
                group: "/".to_owned(),
                data_type: DataType::F32,
                byte_order: PayloadByteOrder::LittleEndian,
                dimensions: vec!["y".to_owned(), "x".to_owned()],
                chunk_shape: vec![1, 2],
                fill_value: None,
                missing_value: None,
                attributes: std::collections::BTreeMap::new(),
                chunks: chunk_refs,
                grid_mapping: None,
            }],
            ..ScientificDataset::default()
        };
        let metadata = Metadata::new("scaling-test").with_scientific_model(model);
        let mut file = Cursor::new(Vec::new());
        write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
            .unwrap();

        // 1. Measure Eager Open
        file.seek(SeekFrom::Start(0)).unwrap();
        let t0 = Instant::now();
        let _snap_eager = open_scientific_snapshot(&mut file).unwrap();
        let eager_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // 2. Measure Lazy Open
        file.seek(SeekFrom::Start(0)).unwrap();
        let t0 = Instant::now();
        let _snap_lazy = open_scientific_snapshot_lazy(&mut file).unwrap();
        let lazy_ms = t0.elapsed().as_secs_f64() * 1000.0;

        println!(
            "Chunks: {:6} | Eager open: {:7.3} ms | Lazy open: {:7.3} ms | Speedup: {:.1}x",
            num_chunks,
            eager_ms,
            lazy_ms,
            eager_ms / lazy_ms.max(0.001)
        );
    }
}

#[test]
fn adr0024_lossy_quantization_and_byte_shuffle_roundtrip_with_error_bound() {
    use std::io::{Cursor, Seek, SeekFrom};
    use wx1_format::{
        open_scientific_snapshot, quantize_f32_slice, write_generation_container, ArrayChunkRef,
        ArraySelection, ChunkCodec, DataType, Dimension, DimensionRole, EncodedChunkInput,
        HeaderByteOrder, Metadata, PayloadByteOrder, QuantizationConfig, ScientificDataset,
        Variable,
    };

    // 1. Create a realistic continuous 50x50 temperature array (2500 elements)
    let eps = 0.01f64; // declared bound: +/- 0.01 K
    let config = QuantizationConfig::absolute(eps).expect("valid config");

    let mut raw_floats = Vec::with_capacity(2500);
    for y in 0..50 {
        for x in 0..50 {
            let val = 273.15 + (y as f32 * 0.2).sin() * 10.0 + (x as f32 * 0.15).cos() * 5.0;
            raw_floats.push(val);
        }
    }

    // 2. Quantize array with guaranteed bound
    let (quantized_floats, report) =
        quantize_f32_slice(&raw_floats, &config, None).expect("quantization succeeds");
    assert!(
        report.max_measured_abs_error <= eps,
        "Measured error {} exceeded declared bound {}",
        report.max_measured_abs_error,
        eps
    );

    // Convert quantized floats to bytes (LittleEndian)
    let mut raw_bytes = Vec::with_capacity(2500 * 4);
    for &f in &quantized_floats {
        raw_bytes.extend_from_slice(&f.to_le_bytes());
    }

    // 3. Store in chunks using ChunkCodec::ZstdByteShuffle4
    let chunk_refs = vec![ArrayChunkRef {
        chunk_id: 1,
        origin: vec![0, 0],
        shape: vec![50, 50],
    }];
    let chunks = vec![EncodedChunkInput {
        id: 1,
        data: &raw_bytes,
        codec: ChunkCodec::ZstdByteShuffle4,
    }];

    let mut attributes = std::collections::BTreeMap::new();
    attributes.insert(
        "wx1:lossy".to_string(),
        AttributeValue::Text("true".to_string()),
    );
    attributes.insert(
        "wx1:error_bound".to_string(),
        AttributeValue::Text(format!("{eps}")),
    );
    attributes.insert(
        "wx1:error_metric".to_string(),
        AttributeValue::Text("max_abs_error".to_string()),
    );
    attributes.insert(
        "wx1:max_measured_abs_error".to_string(),
        AttributeValue::Text(format!("{}", report.max_measured_abs_error)),
    );

    let model = ScientificDataset {
        dimensions: vec![
            Dimension {
                name: "y".to_owned(),
                length: 50,
                role: DimensionRole::Latitude,
            },
            Dimension {
                name: "x".to_owned(),
                length: 50,
                role: DimensionRole::Longitude,
            },
        ],
        variables: vec![Variable {
            name: "t2m".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::F32,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["y".to_owned(), "x".to_owned()],
            chunk_shape: vec![50, 50],
            fill_value: None,
            missing_value: None,
            attributes,
            chunks: chunk_refs,
            grid_mapping: None,
        }],
        ..ScientificDataset::default()
    };

    let metadata = Metadata::new("lossy-adr0024-test").with_scientific_model(model);
    let mut container = Cursor::new(Vec::new());
    write_generation_container(
        &mut container,
        &metadata,
        &chunks,
        HeaderByteOrder::LittleEndian,
    )
    .expect("write container succeeds");

    // 4. Open snapshot and validate
    container.seek(SeekFrom::Start(0)).unwrap();
    let snapshot = open_scientific_snapshot(&mut container).expect("open snapshot succeeds");
    snapshot
        .validate_full(&mut container)
        .expect("validate_full passes on ZstdByteShuffle4 chunk");

    // 5. Read back entire array slice
    let selection = ArraySelection {
        start: vec![0, 0],
        count: vec![50, 50],
    };
    let array = snapshot
        .read_array_slice(&mut container, "/t2m", &selection)
        .expect("read slice succeeds");
    assert_eq!(array.data.len(), 2500 * 4);

    // 6. Verify that reconstructed values strictly satisfy |recon - orig| <= eps
    for (i, &orig) in raw_floats.iter().enumerate() {
        let b = &array.data[i * 4..(i + 1) * 4];
        let recon = f32::from_le_bytes(b.try_into().unwrap());
        let diff = (recon - orig).abs() as f64;
        assert!(
            diff <= eps,
            "Index {i}: recon {recon} diff from orig {orig} is {diff} > eps {eps}"
        );
    }
}

#[test]
fn adr0025_parallel_staging_is_bit_exact_across_worker_counts_and_shuffled_inputs() {
    use std::io::Cursor;
    use wx1_format::{
        stage_chunks_parallel, write_generation_container, write_staged_generation_container,
        ArrayChunkRef, ChunkCodec, DataType, Dimension, DimensionRole, EncodedChunkInput,
        HeaderByteOrder, Metadata, PayloadByteOrder, ScientificDataset, Variable,
    };

    let num_chunks = 32usize;
    let mut payloads = Vec::with_capacity(num_chunks);
    for c in 0..num_chunks {
        let mut buf = Vec::with_capacity(256 * 4);
        for i in 0..256 {
            let val = 270.0f32 + (c as f32) * 0.5 + (i as f32) * 0.03125;
            buf.extend_from_slice(&val.to_le_bytes());
        }
        payloads.push(buf);
    }

    let ordered_inputs: Vec<EncodedChunkInput<'_>> = (0..num_chunks)
        .map(|c| EncodedChunkInput {
            id: (c + 1) as u64,
            data: &payloads[c],
            codec: if c % 2 == 0 {
                ChunkCodec::ZstdByteShuffle4
            } else {
                ChunkCodec::Zstandard
            },
        })
        .collect();

    // Reverse/interleave input order to test canonical sorting invariance
    let mut shuffled_inputs = Vec::with_capacity(num_chunks);
    for c in (0..num_chunks).rev() {
        shuffled_inputs.push(ordered_inputs[c]);
    }

    let chunk_refs: Vec<ArrayChunkRef> = (0..num_chunks)
        .map(|c| ArrayChunkRef {
            chunk_id: (c + 1) as u64,
            origin: vec![c as u64, 0],
            shape: vec![1, 256],
        })
        .collect();

    let model = ScientificDataset {
        dimensions: vec![
            Dimension {
                name: "tile".to_owned(),
                length: num_chunks as u64,
                role: DimensionRole::Other,
            },
            Dimension {
                name: "x".to_owned(),
                length: 256,
                role: DimensionRole::Longitude,
            },
        ],
        variables: vec![Variable {
            name: "q".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::F32,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["tile".to_owned(), "x".to_owned()],
            chunk_shape: vec![1, 256],
            fill_value: None,
            missing_value: None,
            attributes: std::collections::BTreeMap::new(),
            chunks: chunk_refs,
            grid_mapping: None,
        }],
        ..ScientificDataset::default()
    };
    let metadata = Metadata::new("adr0025-bit-exact").with_scientific_model(model);

    let mut serial_cursor = Cursor::new(Vec::new());
    write_generation_container(
        &mut serial_cursor,
        &metadata,
        &ordered_inputs,
        HeaderByteOrder::LittleEndian,
    )
    .expect("serial container write");
    let baseline_bytes = serial_cursor.into_inner();

    for &workers in &[1usize, 2, 4, 8, 12] {
        let staged = stage_chunks_parallel(&shuffled_inputs, workers).expect("parallel staging");
        assert_eq!(staged.len(), num_chunks);
        for (idx, sc) in staged.iter().enumerate() {
            assert_eq!(sc.id, (idx + 1) as u64);
        }
        let mut staged_cursor = Cursor::new(Vec::new());
        write_staged_generation_container(
            &mut staged_cursor,
            &metadata,
            &staged,
            &[],
            HeaderByteOrder::LittleEndian,
        )
        .expect("staged container write");
        assert_eq!(
            staged_cursor.into_inner(),
            baseline_bytes,
            "Bit-exact mismatch for worker count {workers}"
        );
    }
}

#[test]
fn adr0025_async_scientific_writer_commits_generations_and_records_backpressure() {
    use std::fs::File;
    use wx1_format::{
        open_scientific_snapshot_lazy, repair_generation_file, ArrayChunkRef, ArraySelection,
        AsyncScientificWriter, AsyncWriterOptions, ChunkCodec, DataType, Dimension, DimensionRole,
        EncodedChunkInput, HeaderByteOrder, Metadata, PayloadByteOrder, ScientificDataset,
        Variable, WriteDurability,
    };

    let scratch_root = std::env::var("WX1_SCRATCH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("wx1_bench_scratch"));
    std::fs::create_dir_all(&scratch_root).expect("create scratch dir");
    let file_path = scratch_root.join("test_adr0025_async_writer.wx1");
    let _ = std::fs::remove_file(&file_path);

    let options = AsyncWriterOptions {
        max_pending_generations: 1,
        max_queued_bytes: 512, // Deliberately tiny to force backpressure when >1 generation is in flight
        staging_workers: 4,
        durability: WriteDurability::Buffered,
        byte_order: HeaderByteOrder::LittleEndian,
    };

    let writer = AsyncScientificWriter::open(&file_path, options).expect("open async writer");

    let mut step_payloads = Vec::new();
    for step in 0..4u64 {
        let mut bytes = Vec::with_capacity(256 * 4);
        for i in 0..256 {
            let v = 280.0f32 + (step as f32) * 1.5 + (i as f32) * 0.125;
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        step_payloads.push(bytes);
    }

    for step in 0..4u64 {
        let chunk_refs: Vec<ArrayChunkRef> = (0..=step)
            .map(|s| ArrayChunkRef {
                chunk_id: s + 1,
                origin: vec![s, 0],
                shape: vec![1, 256],
            })
            .collect();
        let model = ScientificDataset {
            dimensions: vec![
                Dimension {
                    name: "time".to_owned(),
                    length: step + 1,
                    role: DimensionRole::Time,
                },
                Dimension {
                    name: "x".to_owned(),
                    length: 256,
                    role: DimensionRole::Longitude,
                },
            ],
            variables: vec![Variable {
                name: "t2m".to_owned(),
                group: "/".to_owned(),
                data_type: DataType::F32,
                byte_order: PayloadByteOrder::LittleEndian,
                dimensions: vec!["time".to_owned(), "x".to_owned()],
                chunk_shape: vec![1, 256],
                fill_value: None,
                missing_value: None,
                attributes: std::collections::BTreeMap::new(),
                chunks: chunk_refs,
                grid_mapping: None,
            }],
            ..ScientificDataset::default()
        };
        let metadata = Metadata::new("nwp-async-run").with_scientific_model(model);
        let new_chunks = [EncodedChunkInput {
            id: step + 1,
            data: &step_payloads[step as usize],
            codec: ChunkCodec::ZstdByteShuffle4,
        }];
        writer
            .submit_generation(metadata, &new_chunks, &[])
            .expect("submit step");
    }

    let telemetry = writer.close().expect("close async writer");
    assert_eq!(telemetry.submitted_generations, 4);
    assert_eq!(telemetry.committed_generations, 4);
    assert!(telemetry.total_raw_bytes == 4 * 256 * 4);
    assert!(telemetry.total_staged_bytes > 0);

    // Open lazily and verify all 4 timesteps
    let mut f = File::open(&file_path).expect("open committed file");
    let snap = open_scientific_snapshot_lazy(&mut f).expect("open lazy snapshot");
    assert_eq!(snap.generation(), 4);
    snap.validate_full(&mut f).expect("validate full");

    let slice = snap
        .read_array_slice(
            &mut f,
            "/t2m",
            &ArraySelection::new(vec![0, 0], vec![4, 256]),
        )
        .expect("read all 4 steps");
    assert_eq!(slice.shape, vec![4, 256]);
    for (step, payload) in step_payloads.iter().enumerate() {
        let offset = step * 256 * 4;
        assert_eq!(&slice.data[offset..offset + 256 * 4], payload.as_slice());
    }
    drop(f);

    // Simulate a torn tail after generation 4 (C1 crash recovery) and verify repair
    {
        use std::io::Write;
        let mut f_append = std::fs::OpenOptions::new()
            .append(true)
            .open(&file_path)
            .unwrap();
        f_append.write_all(&[0xde, 0xad, 0xbe, 0xef, 0x00]).unwrap();
    }
    let repaired = repair_generation_file(&file_path).expect("repair torn tail");
    assert_eq!(repaired.generation, 4);

    let mut f2 = File::open(&file_path).expect("reopen repaired file");
    let snap2 = open_scientific_snapshot_lazy(&mut f2).expect("open after repair");
    assert_eq!(snap2.generation(), 4);
    let _ = std::fs::remove_file(&file_path);
}

#[test]
fn adr0026_cubed_sphere_topology_halos_arakawa_c_and_tile_reads() {
    use std::io::Cursor;
    use wx1_format::{
        open_scientific_snapshot_lazy, write_generation_container, ArrayChunkRef, ChunkCodec,
        CubedSphereTopology, DataType, Dimension, DimensionRole, EncodedChunkInput,
        GnomonicProjection, GridFamily, GridTopologyDescriptor, HaloConsistencyState, HaloSpec,
        HeaderByteOrder, HorizontalStagger, Metadata, NestingDescriptor, PanelEdge,
        PayloadByteOrder, ReducedGaussianTopology, ScientificDataset, Variable, VariableGridLayout,
        VerticalStagger,
    };

    // 1. Verify octahedral reduced Gaussian O96 = 40,320 points and O1280 = 6,599,680 points
    let rg_o96 = ReducedGaussianTopology::octahedral(96).expect("valid O96");
    assert_eq!(rg_o96.ring_point_counts.len(), 192);
    assert_eq!(rg_o96.ring_point_counts[0], 20); // first ring near N pole: 4*1 + 16 = 20
    assert_eq!(rg_o96.ring_point_counts[191], 20); // symmetric near S pole
    assert_eq!(rg_o96.total_points(), 40_320);

    let rg_o1280 = ReducedGaussianTopology::octahedral(1280).expect("valid O1280");
    assert_eq!(rg_o1280.ring_point_counts.len(), 2560);
    assert_eq!(rg_o1280.total_points(), 6_599_680);

    // Verify 6 * N^2 cells per level for C96, C192, and C384
    let cs_c96 =
        CubedSphereTopology::standard(96, GnomonicProjection::Equiangular).expect("valid C96");
    assert_eq!(cs_c96.total_cells_per_level().unwrap(), 6 * 96 * 96);
    assert_eq!(cs_c96.total_cells_per_level().unwrap(), 55_296);

    let cs_c192 =
        CubedSphereTopology::standard(192, GnomonicProjection::Equiangular).expect("valid C192");
    assert_eq!(cs_c192.total_cells_per_level().unwrap(), 6 * 192 * 192);
    assert_eq!(cs_c192.total_cells_per_level().unwrap(), 221_184);

    let cs_c384 =
        CubedSphereTopology::standard(384, GnomonicProjection::Equiangular).expect("valid C384");
    assert_eq!(cs_c384.total_cells_per_level().unwrap(), 6 * 384 * 384);
    assert_eq!(cs_c384.total_cells_per_level().unwrap(), 884_736);

    // 1b. Geometric verification of all 24 directed panel edges and halo cell mappings
    //     using 3D unit-sphere coordinates from the equiangular gnomonic projection.
    let n_geom = 8u64;
    let cs_geom =
        CubedSphereTopology::standard(n_geom, GnomonicProjection::Equiangular).expect("valid C8");
    let d_angle = std::f64::consts::FRAC_PI_2 / (n_geom as f64); // pi/2 / N
    let cell_angle =
        |idx: i64| -> f64 { -std::f64::consts::FRAC_PI_4 + ((idx as f64) + 0.5) * d_angle };
    let all_edges = [
        PanelEdge::West,
        PanelEdge::East,
        PanelEdge::South,
        PanelEdge::North,
    ];
    // First check exact boundary continuity along all 24 panel edges at alpha/beta = +/- pi/4
    for tile in 0..6u8 {
        for &edge in &all_edges {
            let nbr = cs_geom.panels[tile as usize].edge(edge);
            for t_idx in 0..n_geom {
                let tau = cell_angle(t_idx as i64);
                let boundary = std::f64::consts::FRAC_PI_4;
                let (alpha, beta) = match edge {
                    PanelEdge::West => (-boundary, tau),
                    PanelEdge::East => (boundary, tau),
                    PanelEdge::South => (tau, -boundary),
                    PanelEdge::North => (tau, boundary),
                };
                let xyz_self = CubedSphereTopology::gnomonic_xyz(tile, alpha, beta).unwrap();

                let t_nbr_idx = if nbr.reverse_index {
                    n_geom - 1 - t_idx
                } else {
                    t_idx
                };
                let tau_nbr = cell_angle(t_nbr_idx as i64);
                let (alpha_nbr, beta_nbr) = match nbr.target_edge {
                    PanelEdge::West => (-boundary, tau_nbr),
                    PanelEdge::East => (boundary, tau_nbr),
                    PanelEdge::South => (tau_nbr, -boundary),
                    PanelEdge::North => (tau_nbr, boundary),
                };
                let xyz_nbr =
                    CubedSphereTopology::gnomonic_xyz(nbr.target_tile, alpha_nbr, beta_nbr)
                        .unwrap();
                let dist = ((xyz_self[0] - xyz_nbr[0]).powi(2)
                    + (xyz_self[1] - xyz_nbr[1]).powi(2)
                    + (xyz_self[2] - xyz_nbr[2]).powi(2))
                .sqrt();
                assert!(
                    dist < 1e-14,
                    "Edge discontinuity on tile {tile} edge {edge:?} t={t_idx}: dist={dist}"
                );

                // Also verify that map_edge_halo_index(d=1) lands on the neighboring interior cell
                // whose 3D unit-sphere center is strictly closest to the boundary point!
                let (mapped_tile, my, mx) = cs_geom
                    .map_edge_halo_index(tile, edge, t_idx, 1)
                    .expect("map halo d=1");
                assert_eq!(mapped_tile, nbr.target_tile);
                let xyz_mapped_cell = CubedSphereTopology::gnomonic_xyz(
                    mapped_tile,
                    cell_angle(mx as i64),
                    cell_angle(my as i64),
                )
                .unwrap();
                let chord = ((xyz_self[0] - xyz_mapped_cell[0]).powi(2)
                    + (xyz_self[1] - xyz_mapped_cell[1]).powi(2)
                    + (xyz_self[2] - xyz_mapped_cell[2]).powi(2))
                .sqrt();
                assert!(
                    chord < d_angle,
                    "Mapped halo cell on tile {mapped_tile} ({my},{mx}) too far from edge: {chord}"
                );
            }
        }
    }

    let rg_desc = GridTopologyDescriptor {
        family: GridFamily::ReducedGaussian,
        cubed_sphere: None,
        reduced_gaussian: Some(rg_o96.clone()),
        nesting: None,
    };
    let rg_gm = rg_desc
        .to_grid_mapping("o96_grid", vec!["ring".to_owned()])
        .expect("serialize O96");
    let rg_parsed = GridTopologyDescriptor::from_grid_mapping(&rg_gm)
        .expect("parse O96")
        .expect("present");
    assert_eq!(rg_parsed, rg_desc);

    // 2. Build a C8 Cubed-Sphere dataset with 6 tiles, halo width 2 (allocated 12x12 for center,
    //    12x13 for Arakawa C EdgeU), and verify tile planning/reading (R9) with and without halos.
    let c_res = 8u64;
    let halo = HaloSpec::symmetric(2);
    let ny_center = c_res + 4; // 8 + 2 + 2 = 12
    let nx_center = c_res + 4; // 12
    let nx_edge_u = (c_res + 1) + 4; // 9 + 4 = 13

    let cs_topo =
        CubedSphereTopology::standard(c_res, GnomonicProjection::Equiangular).expect("valid C8");
    let cs_desc = GridTopologyDescriptor {
        family: GridFamily::CubedSphere,
        cubed_sphere: Some(cs_topo),
        reduced_gaussian: None,
        nesting: Some(NestingDescriptor {
            domain_id: "global_c8".to_owned(),
            parent_domain_id: None,
            refinement_ratio_y: 1,
            refinement_ratio_x: 1,
            parent_start_y: 0,
            parent_start_x: 0,
            two_way_feedback: false,
        }),
    };
    let cs_gm = cs_desc
        .to_grid_mapping(
            "cubed_sphere_c8",
            vec!["tile".to_owned(), "y".to_owned(), "x".to_owned()],
        )
        .expect("cs grid mapping");

    // Create 6 tiles of chunks for cell-center temperature `t` (shape [6, 12, 12], chunk [1, 12, 12])
    // and Arakawa C zonal velocity `u` (shape [6, 12, 13], chunk [1, 12, 13])
    let mut t_chunks = Vec::new();
    let mut u_chunks = Vec::new();
    let mut t_payloads = Vec::new();
    let mut u_payloads = Vec::new();

    for tile in 0..6u64 {
        t_chunks.push(ArrayChunkRef {
            chunk_id: tile + 1,
            origin: vec![tile, 0, 0],
            shape: vec![1, ny_center, nx_center],
        });
        let mut t_buf = Vec::with_capacity((ny_center * nx_center * 4) as usize);
        for y in 0..ny_center {
            for x in 0..nx_center {
                let is_halo = !(2..10).contains(&y) || !(2..10).contains(&x);
                let val = if is_halo {
                    -999.0f32
                } else {
                    (tile as f32) * 100.0 + ((y - 2) as f32) * 10.0 + ((x - 2) as f32)
                };
                t_buf.extend_from_slice(&val.to_le_bytes());
            }
        }
        t_payloads.push(t_buf);

        u_chunks.push(ArrayChunkRef {
            chunk_id: tile + 10,
            origin: vec![tile, 0, 0],
            shape: vec![1, ny_center, nx_edge_u],
        });
        let u_buf = vec![0u8; (ny_center * nx_edge_u * 4) as usize];
        u_payloads.push(u_buf);
    }

    let mut t_var = Variable {
        name: "t".to_owned(),
        group: "/".to_owned(),
        data_type: DataType::F32,
        byte_order: PayloadByteOrder::LittleEndian,
        dimensions: vec!["tile".to_owned(), "y".to_owned(), "x".to_owned()],
        chunk_shape: vec![1, ny_center, nx_center],
        fill_value: None,
        missing_value: None,
        attributes: std::collections::BTreeMap::new(),
        chunks: t_chunks,
        grid_mapping: Some("cubed_sphere_c8".to_owned()),
    };
    VariableGridLayout {
        horizontal_stagger: HorizontalStagger::Center,
        vertical_stagger: VerticalStagger::FullLevel,
        halo,
        halo_consistency: HaloConsistencyState::StaleOrUnfilled,
        tile_dimension: Some("tile".to_owned()),
    }
    .apply_to_variable(&mut t_var);

    let mut u_var = Variable {
        name: "u".to_owned(),
        group: "/".to_owned(),
        data_type: DataType::F32,
        byte_order: PayloadByteOrder::LittleEndian,
        dimensions: vec!["tile".to_owned(), "y".to_owned(), "x_u".to_owned()],
        chunk_shape: vec![1, ny_center, nx_edge_u],
        fill_value: None,
        missing_value: None,
        attributes: std::collections::BTreeMap::new(),
        chunks: u_chunks,
        grid_mapping: Some("cubed_sphere_c8".to_owned()),
    };
    VariableGridLayout {
        horizontal_stagger: HorizontalStagger::EdgeU,
        vertical_stagger: VerticalStagger::FullLevel,
        halo,
        halo_consistency: HaloConsistencyState::StaleOrUnfilled,
        tile_dimension: Some("tile".to_owned()),
    }
    .apply_to_variable(&mut u_var);

    let dataset = ScientificDataset {
        dimensions: vec![
            Dimension {
                name: "tile".to_owned(),
                length: 6,
                role: DimensionRole::Other,
            },
            Dimension {
                name: "y".to_owned(),
                length: ny_center,
                role: DimensionRole::Other,
            },
            Dimension {
                name: "x".to_owned(),
                length: nx_center,
                role: DimensionRole::Other,
            },
            Dimension {
                name: "x_u".to_owned(),
                length: nx_edge_u,
                role: DimensionRole::Other,
            },
        ],
        variables: vec![t_var, u_var],
        grid_mappings: vec![cs_gm],
        ..ScientificDataset::default()
    };
    dataset
        .validate()
        .expect("valid cubed-sphere + Arakawa C dataset");

    let mut encoded_inputs = Vec::new();
    for tile in 0..6usize {
        encoded_inputs.push(EncodedChunkInput {
            id: (tile + 1) as u64,
            data: &t_payloads[tile],
            codec: ChunkCodec::ZstdByteShuffle4,
        });
        encoded_inputs.push(EncodedChunkInput {
            id: (tile + 10) as u64,
            data: &u_payloads[tile],
            codec: ChunkCodec::ZstdByteShuffle4,
        });
    }

    let metadata = Metadata::new("adr0026-cs-test").with_scientific_model(dataset);
    let mut cursor = Cursor::new(Vec::new());
    write_generation_container(
        &mut cursor,
        &metadata,
        &encoded_inputs,
        HeaderByteOrder::LittleEndian,
    )
    .expect("write cubed-sphere container");

    cursor.set_position(0);
    let snap = open_scientific_snapshot_lazy(&mut cursor).expect("lazy open");

    // R9: Plan & read tile 3 WITHOUT halos (interior 8x8 only)
    let plan_t3 = snap.plan_tile_slice("/t", 3, false).expect("plan tile 3");
    assert_eq!(plan_t3.chunk_ids, vec![4]);
    assert_eq!(plan_t3.shape, vec![1, 8, 8]);

    let tile3_interior = snap
        .read_tile_slice(&mut cursor, "/t", 3, false)
        .expect("read tile 3 interior");
    assert_eq!(tile3_interior.shape, vec![1, 8, 8]);
    let first_val = f32::from_le_bytes(tile3_interior.data[0..4].try_into().unwrap());
    let last_val = f32::from_le_bytes(tile3_interior.data[63 * 4..64 * 4].try_into().unwrap());
    assert_eq!(first_val, 300.0);
    assert_eq!(last_val, 377.0);

    // R9: Read tile 3 WITH halos (full 12x12 allocated tile)
    let tile3_full = snap
        .read_tile_slice(&mut cursor, "/t", 3, true)
        .expect("read tile 3 with halos");
    assert_eq!(tile3_full.shape, vec![1, 12, 12]);
    let halo_corner = f32::from_le_bytes(tile3_full.data[0..4].try_into().unwrap());
    assert_eq!(halo_corner, -999.0);

    // Since halos were filled with -999.0 sentinel, verify_cubed_sphere_edge_halos must return false
    cursor.set_position(0);
    assert!(!snap
        .verify_cubed_sphere_edge_halos(&mut cursor, "/t")
        .expect("verify stale halos"));

    // Now perform an exact halo exchange across all 24 panel edges and verify that
    // verify_cubed_sphere_edge_halos returns true!
    let cs_for_exch =
        CubedSphereTopology::standard(c_res, GnomonicProjection::Equiangular).unwrap();
    let mut exchanged_payloads = t_payloads.clone();
    let get_int_val = |t: u8, iy: u64, ix: u64| -> [u8; 4] {
        let idx = (((iy + 2) * ny_center + (ix + 2)) * 4) as usize;
        t_payloads[t as usize][idx..idx + 4].try_into().unwrap()
    };
    for tile in 0..6u8 {
        for &edge in &all_edges {
            for t_idx in 0..c_res {
                for d in 1..=2u64 {
                    let (nbr_tile, nbr_iy, nbr_ix) = cs_for_exch
                        .map_edge_halo_index(tile, edge, t_idx, d)
                        .unwrap();
                    let val_bytes = get_int_val(nbr_tile, nbr_iy, nbr_ix);
                    let (hy, hx) = match edge {
                        PanelEdge::West => (2 + t_idx, 2 - d),
                        PanelEdge::East => (2 + t_idx, 2 + c_res - 1 + d),
                        PanelEdge::South => (2 - d, 2 + t_idx),
                        PanelEdge::North => (2 + c_res - 1 + d, 2 + t_idx),
                    };
                    let dst_idx = ((hy * ny_center + hx) * 4) as usize;
                    exchanged_payloads[tile as usize][dst_idx..dst_idx + 4]
                        .copy_from_slice(&val_bytes);
                }
            }
        }
    }
    let mut exch_inputs = Vec::new();
    for tile in 0..6usize {
        exch_inputs.push(EncodedChunkInput {
            id: (tile + 1) as u64,
            data: &exchanged_payloads[tile],
            codec: ChunkCodec::ZstdByteShuffle4,
        });
        exch_inputs.push(EncodedChunkInput {
            id: (tile + 10) as u64,
            data: &u_payloads[tile],
            codec: ChunkCodec::ZstdByteShuffle4,
        });
    }
    let mut exch_cursor = Cursor::new(Vec::new());
    write_generation_container(
        &mut exch_cursor,
        &metadata,
        &exch_inputs,
        HeaderByteOrder::LittleEndian,
    )
    .unwrap();
    exch_cursor.set_position(0);
    let exch_snap = open_scientific_snapshot_lazy(&mut exch_cursor).unwrap();
    assert!(exch_snap
        .verify_cubed_sphere_edge_halos(&mut exch_cursor, "/t")
        .expect("verify exchanged halos"));
}

#[test]
fn adr0027_variable_parameter_registry_and_provenance_integration() {
    let mut var = Variable {
        name: "temperature".to_owned(),
        group: "/".to_owned(),
        data_type: DataType::F32,
        byte_order: PayloadByteOrder::LittleEndian,
        dimensions: vec!["y".to_owned(), "x".to_owned()],
        chunk_shape: vec![2, 2],
        fill_value: None,
        missing_value: None,
        attributes: std::collections::BTreeMap::new(),
        chunks: vec![ArrayChunkRef {
            chunk_id: 1,
            origin: vec![0, 0],
            shape: vec![2, 2],
        }],
        grid_mapping: None,
    };

    var.set_registry_parameter(
        "air_temperature",
        Some("air_temperature"),
        "K",
        Some(("wmo.grib2", "4.2", "2025")),
    );

    assert_eq!(var.canonical_id(), "air_temperature");
    assert_eq!(var.standard_name(), Some("air_temperature"));
    assert_eq!(var.units(), Some("K"));

    let mut prov = Provenance {
        source: "ECMWF IFS Cycle 48r1".to_owned(),
        provider: Some("ECMWF".to_owned()),
        product: Some("oper".to_owned()),
        institution: Some("European Centre for Medium-Range Weather Forecasts".to_owned()),
        license: Some("CC-BY-4.0".to_owned()),
        history: None,
        reference_time: Some("2026-11-02T00:00:00Z".to_owned()),
    };

    prov.append_history("2026-11-02T00:05:00Z: Initial extraction from MARS");
    prov.append_history("2026-11-02T00:10:00Z: Converted to WX-1 container");

    let history = prov.history.as_ref().unwrap();
    assert!(history.contains("Initial extraction"));
    assert!(history.contains("Converted to WX-1 container"));
    assert_eq!(history.lines().count(), 2);
}

#[test]
fn adr0029_point_scatter_reads_batch_coordinates_across_chunks() {
    let mut model = ScientificDataset::new();
    model.dimensions = vec![
        Dimension {
            name: "y".to_owned(),
            length: 4,
            role: DimensionRole::Latitude,
        },
        Dimension {
            name: "x".to_owned(),
            length: 4,
            role: DimensionRole::Longitude,
        },
    ];

    // 4 chunks of 2x2:
    // Chunk 1: [0..2, 0..2] -> values 10, 11, 12, 13
    // Chunk 2: [0..2, 2..4] -> values 20, 21, 22, 23
    // Chunk 3: [2..4, 0..2] -> values 30, 31, 32, 33
    // Chunk 4: [2..4, 2..4] -> values 40, 41, 42, 43
    model.variables.push(Variable {
        name: "temperature".to_owned(),
        group: "/".to_owned(),
        data_type: DataType::U16,
        byte_order: PayloadByteOrder::LittleEndian,
        dimensions: vec!["y".to_owned(), "x".to_owned()],
        chunk_shape: vec![2, 2],
        fill_value: None,
        missing_value: None,
        attributes: std::collections::BTreeMap::new(),
        chunks: vec![
            ArrayChunkRef {
                chunk_id: 1,
                origin: vec![0, 0],
                shape: vec![2, 2],
            },
            ArrayChunkRef {
                chunk_id: 2,
                origin: vec![0, 2],
                shape: vec![2, 2],
            },
            ArrayChunkRef {
                chunk_id: 3,
                origin: vec![2, 0],
                shape: vec![2, 2],
            },
            ArrayChunkRef {
                chunk_id: 4,
                origin: vec![2, 2],
                shape: vec![2, 2],
            },
        ],
        grid_mapping: None,
    });

    let metadata = Metadata::new("point-scatter").with_scientific_model(model);

    let c1 = u16_bytes(&[10, 11, 12, 13], PayloadByteOrder::LittleEndian);
    let c2 = u16_bytes(&[20, 21, 22, 23], PayloadByteOrder::LittleEndian);
    let c3 = u16_bytes(&[30, 31, 32, 33], PayloadByteOrder::LittleEndian);
    let c4 = u16_bytes(&[40, 41, 42, 43], PayloadByteOrder::LittleEndian);

    let chunks = vec![
        EncodedChunkInput {
            id: 1,
            data: &c1,
            codec: ChunkCodec::Raw,
        },
        EncodedChunkInput {
            id: 2,
            data: &c2,
            codec: ChunkCodec::Raw,
        },
        EncodedChunkInput {
            id: 3,
            data: &c3,
            codec: ChunkCodec::Raw,
        },
        EncodedChunkInput {
            id: 4,
            data: &c4,
            codec: ChunkCodec::Raw,
        },
    ];

    let mut file = Cursor::new(Vec::new());
    write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
        .unwrap();

    let snapshot = open_scientific_snapshot(&mut file).unwrap();

    // Query 5 scattered points across all 4 chunks
    let query_points = vec![
        vec![0, 0], // Chunk 1, local 0 -> 10
        vec![3, 3], // Chunk 4, local 3 -> 43
        vec![0, 3], // Chunk 2, local 1 -> 21
        vec![2, 1], // Chunk 3, local 1 -> 31
        vec![1, 2], // Chunk 2, local 2 -> 22
    ];

    let result = snapshot
        .read_point_coordinates(&mut file, "/temperature", &query_points)
        .unwrap();

    assert_eq!(result.shape, vec![5]);
    assert_eq!(result.data_type, DataType::U16);

    let extracted: Vec<u16> = result
        .data
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();

    assert_eq!(extracted, vec![10, 43, 21, 31, 22]);

    // Out of bounds coordinate check
    let oob_query = vec![vec![4, 0]];
    assert!(snapshot
        .read_point_coordinates(&mut file, "/temperature", &oob_query)
        .is_err());

    // Rank mismatch check
    let rank_mismatch_query = vec![vec![0, 0, 0]];
    assert!(snapshot
        .read_point_coordinates(&mut file, "/temperature", &rank_mismatch_query)
        .is_err());
}
