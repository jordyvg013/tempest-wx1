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
use std::fs::File;
use std::io::Cursor;
use std::path::PathBuf;

use wx1_format::{
    open_generation_snapshot, open_scientific_snapshot, write_generation_container, ArrayChunkRef,
    ArraySelection, AttributeValue, ChunkCodec, ChunkIoError, DataType, Dimension, DimensionRole,
    EncodedChunkInput, HeaderByteOrder, Metadata, PayloadByteOrder, ScientificDataset, Variable,
};

fn fixtures_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = manifest_dir.join("../../tests/fixtures/golden_vectors");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn build_gv01_bytes() -> Vec<u8> {
    let mut model = ScientificDataset::new();
    model.dimensions = vec![
        Dimension {
            name: "lat".to_owned(),
            length: 16,
            role: DimensionRole::Latitude,
        },
        Dimension {
            name: "lon".to_owned(),
            length: 16,
            role: DimensionRole::Longitude,
        },
    ];

    let mut attrs = BTreeMap::new();
    attrs.insert("units".to_owned(), AttributeValue::Text("K".to_owned()));
    attrs.insert(
        "standard_name".to_owned(),
        AttributeValue::Text("air_temperature".to_owned()),
    );

    let chunk_raw_f32: Vec<f32> = (0..256).map(|i| 273.15 + (i as f32) * 0.1).collect();
    let mut chunk_bytes = Vec::with_capacity(1024);
    for val in &chunk_raw_f32 {
        chunk_bytes.extend_from_slice(&val.to_le_bytes());
    }

    let chunk_refs = vec![ArrayChunkRef {
        chunk_id: 1,
        origin: vec![0, 0],
        shape: vec![16, 16],
    }];

    model.variables = vec![Variable {
        name: "t2m".to_owned(),
        group: "/".to_owned(),
        data_type: DataType::F32,
        byte_order: PayloadByteOrder::LittleEndian,
        dimensions: vec!["lat".to_owned(), "lon".to_owned()],
        chunk_shape: vec![16, 16],
        fill_value: None,
        missing_value: None,
        attributes: attrs,
        chunks: chunk_refs,
        grid_mapping: None,
    }];

    let metadata = Metadata::new("gv01").with_scientific_model(model);
    let chunks = [EncodedChunkInput {
        id: 1,
        data: &chunk_bytes,
        codec: ChunkCodec::Zstandard,
    }];

    let mut file = Cursor::new(Vec::new());
    write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
        .unwrap();

    file.into_inner()
}

#[test]
fn test_gv01_valid_scalar_2d_zstd() {
    let path = fixtures_dir().join("gv01_valid_scalar_2d_zstd.wx1");
    let bytes = build_gv01_bytes();
    std::fs::write(&path, &bytes).unwrap();

    // Verify reading
    let mut read_file = File::open(&path).unwrap();
    let snapshot = open_scientific_snapshot(&mut read_file).unwrap();
    assert_eq!(snapshot.dataset().variables.len(), 1);
    let var = &snapshot.dataset().variables[0];
    assert_eq!(var.name, "t2m");
    assert_eq!(var.dimensions, vec!["lat", "lon"]);

    let array = snapshot
        .read_array_slice(
            &mut read_file,
            "/t2m",
            &ArraySelection {
                start: vec![0, 0],
                count: vec![16, 16],
            },
        )
        .unwrap();
    assert_eq!(array.data.len(), 1024);
    let f32_vals: Vec<f32> = array
        .data
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    assert_eq!(f32_vals.len(), 256);
    assert!((f32_vals[0] - 273.15).abs() < 1e-5);
    assert!((f32_vals[255] - (273.15 + 25.5)).abs() < 1e-4);
}

#[test]
fn test_gv02_valid_volume_3d_lz4() {
    let path = fixtures_dir().join("gv02_valid_volume_3d_lz4.wx1");
    let mut file = Cursor::new(Vec::new());

    let mut model = ScientificDataset::new();
    model.dimensions = vec![
        Dimension {
            name: "plev".to_owned(),
            length: 4,
            role: DimensionRole::Vertical,
        },
        Dimension {
            name: "lat".to_owned(),
            length: 8,
            role: DimensionRole::Latitude,
        },
        Dimension {
            name: "lon".to_owned(),
            length: 8,
            role: DimensionRole::Longitude,
        },
    ];

    let count = 4 * 8 * 8; // 256 elements = 1024 bytes
    let raw_f32: Vec<f32> = (0..count).map(|i| 1000.0 - (i as f32)).collect();
    let mut raw_bytes = Vec::with_capacity(count * 4);
    for val in &raw_f32 {
        raw_bytes.extend_from_slice(&val.to_le_bytes());
    }

    let chunk_refs = vec![ArrayChunkRef {
        chunk_id: 10,
        origin: vec![0, 0, 0],
        shape: vec![4, 8, 8],
    }];

    model.variables = vec![Variable {
        name: "gh".to_owned(),
        group: "/".to_owned(),
        data_type: DataType::F32,
        byte_order: PayloadByteOrder::LittleEndian,
        dimensions: vec!["plev".to_owned(), "lat".to_owned(), "lon".to_owned()],
        chunk_shape: vec![4, 8, 8],
        fill_value: None,
        missing_value: None,
        attributes: BTreeMap::new(),
        chunks: chunk_refs,
        grid_mapping: None,
    }];

    let metadata = Metadata::new("gv02").with_scientific_model(model);
    let chunks = [EncodedChunkInput {
        id: 10,
        data: &raw_bytes,
        codec: ChunkCodec::Lz4,
    }];

    write_generation_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
        .unwrap();

    let bytes = file.into_inner();
    std::fs::write(&path, &bytes).unwrap();

    // Verify reading
    let mut read_file = File::open(&path).unwrap();
    let snapshot = open_scientific_snapshot(&mut read_file).unwrap();
    let array = snapshot
        .read_array_slice(
            &mut read_file,
            "/gh",
            &ArraySelection {
                start: vec![0, 0, 0],
                count: vec![4, 8, 8],
            },
        )
        .unwrap();
    assert_eq!(array.data.len(), 1024);
}

#[test]
fn test_gv03_valid_multigen_append() {
    let path = fixtures_dir().join("gv03_valid_multigen_append.wx1");
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }

    // Gen 1: time length 1
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
        attributes: BTreeMap::new(),
        grid_mapping: None,
        chunks: vec![ArrayChunkRef {
            chunk_id: 1,
            origin: vec![0, 0],
            shape: vec![1, 2],
        }],
    }];
    let metadata_g1 = Metadata::new("gv03").with_scientific_model(model_g1);
    let chunk1_bytes = [10.0f32, 20.0f32]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<u8>>();
    let chunks_g1 = [EncodedChunkInput {
        id: 1,
        data: &chunk1_bytes,
        codec: ChunkCodec::Zstandard,
    }];

    {
        let mut file = File::create(&path).unwrap();
        write_generation_container(
            &mut file,
            &metadata_g1,
            &chunks_g1,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
    }

    // Gen 2: time length 2
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
        attributes: BTreeMap::new(),
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
    let metadata_g2 = Metadata::new("gv03").with_scientific_model(model_g2);
    let chunk2_bytes = [30.0f32, 40.0f32]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<u8>>();
    let chunks_g2 = [EncodedChunkInput {
        id: 2,
        data: &chunk2_bytes,
        codec: ChunkCodec::Zstandard,
    }];
    {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        wx1_format::append_scientific_generation(&mut file, &chunks_g2, &metadata_g2).unwrap();
    }

    // Gen 3: time length 3
    let mut model_g3 = ScientificDataset::new();
    model_g3.dimensions = vec![
        Dimension {
            name: "time".to_owned(),
            length: 3,
            role: DimensionRole::Time,
        },
        Dimension {
            name: "x".to_owned(),
            length: 2,
            role: DimensionRole::Longitude,
        },
    ];
    model_g3.variables = vec![Variable {
        name: "temperature".to_owned(),
        group: "/".to_owned(),
        data_type: DataType::F32,
        byte_order: PayloadByteOrder::LittleEndian,
        dimensions: vec!["time".to_owned(), "x".to_owned()],
        chunk_shape: vec![1, 2],
        fill_value: None,
        missing_value: None,
        attributes: BTreeMap::new(),
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
            ArrayChunkRef {
                chunk_id: 3,
                origin: vec![2, 0],
                shape: vec![1, 2],
            },
        ],
    }];
    let metadata_g3 = Metadata::new("gv03").with_scientific_model(model_g3);
    let chunk3_bytes = [50.0f32, 60.0f32]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<u8>>();
    let chunks_g3 = [EncodedChunkInput {
        id: 3,
        data: &chunk3_bytes,
        codec: ChunkCodec::Zstandard,
    }];
    {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        wx1_format::append_scientific_generation(&mut file, &chunks_g3, &metadata_g3).unwrap();
    }

    // Verify all 3 generations
    let mut read_file = File::open(&path).unwrap();
    let snapshot = open_scientific_snapshot(&mut read_file).unwrap();
    assert_eq!(snapshot.generation(), 3);
    let array = snapshot
        .read_array_slice(
            &mut read_file,
            "/temperature",
            &ArraySelection {
                start: vec![0, 0],
                count: vec![3, 2],
            },
        )
        .unwrap();
    assert_eq!(array.data.len(), 3 * 2 * 4);
}

#[test]
fn test_gv04_valid_special_floats() {
    let path = fixtures_dir().join("gv04_valid_special_floats.wx1");
    let special_vals = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0, 0.0, 1.0];
    let mut raw_bytes = Vec::new();
    for v in &special_vals {
        raw_bytes.extend_from_slice(&v.to_le_bytes());
    }

    let chunk = EncodedChunkInput {
        id: 44,
        data: &raw_bytes,
        codec: ChunkCodec::Zstandard,
    };

    let mut model = ScientificDataset::new();
    model.dimensions = vec![Dimension {
        name: "index".to_owned(),
        length: 6,
        role: DimensionRole::Other,
    }];
    model.variables = vec![Variable {
        name: "special_floats".to_owned(),
        group: "/".to_owned(),
        data_type: DataType::F32,
        byte_order: PayloadByteOrder::LittleEndian,
        dimensions: vec!["index".to_owned()],
        chunk_shape: vec![6],
        fill_value: None,
        missing_value: None,
        attributes: BTreeMap::new(),
        chunks: vec![ArrayChunkRef {
            chunk_id: 44,
            origin: vec![0],
            shape: vec![6],
        }],
        grid_mapping: None,
    }];

    let metadata = Metadata::new("special-floats-dataset").with_scientific_model(model);

    let mut file = File::create(&path).unwrap();
    write_generation_container(
        &mut file,
        &metadata,
        &[chunk],
        HeaderByteOrder::LittleEndian,
    )
    .unwrap();

    let mut read_file = File::open(&path).unwrap();
    let snapshot = open_scientific_snapshot(&mut read_file).unwrap();
    let array = snapshot
        .read_array_slice(
            &mut read_file,
            "/special_floats",
            &ArraySelection {
                start: vec![0],
                count: vec![6],
            },
        )
        .unwrap();
    assert_eq!(array.data.len(), 24);
}

#[test]
fn test_gv05_corrupt_magic() {
    let path = fixtures_dir().join("gv05_corrupt_magic.wx1");
    let mut bytes = build_gv01_bytes();

    // Invalidate magic header bytes
    bytes[0] = b'B';
    bytes[1] = b'A';
    bytes[2] = b'D';
    bytes[3] = 0x00;
    std::fs::write(&path, &bytes).unwrap();

    let mut file = File::open(&path).unwrap();
    let res = open_generation_snapshot(&mut file);
    assert!(res.is_err());
}

#[test]
fn test_gv06_corrupt_footer_crc() {
    let path = fixtures_dir().join("gv06_corrupt_footer_crc.wx1");
    let mut bytes = build_gv01_bytes();

    let len = bytes.len();
    // Corrupt last 4 bytes of commit footer (CRC32C)
    bytes[len - 2] ^= 0xFF;
    std::fs::write(&path, &bytes).unwrap();

    let mut file = File::open(&path).unwrap();
    let res = open_generation_snapshot(&mut file);
    assert!(res.is_err());
}

#[test]
fn test_gv07_corrupt_chunk_blake3() {
    let path = fixtures_dir().join("gv07_corrupt_chunk_blake3.wx1");
    let mut bytes = build_gv01_bytes();

    // The container header is 64 bytes, followed by 112 bytes of CBOR metadata.
    // The chunk 1 payload begins at byte offset 176.
    // Flip bit inside chunk payload (offset 180)
    bytes[180] ^= 0x01;
    std::fs::write(&path, &bytes).unwrap();

    let mut file = File::open(&path).unwrap();
    let res = open_scientific_snapshot(&mut file);
    assert!(matches!(
        res,
        Err(ChunkIoError::ChecksumMismatch(1)) | Err(ChunkIoError::ChunkHashMismatch(1))
    ));
}

#[test]
fn test_gv08_corrupt_truncated_payload() {
    let path = fixtures_dir().join("gv08_corrupt_truncated_payload.wx1");
    let bytes = build_gv01_bytes();

    // Truncate by 50 bytes
    let truncated = &bytes[..bytes.len() - 50];
    std::fs::write(&path, truncated).unwrap();

    let mut file = File::open(&path).unwrap();
    let res = open_generation_snapshot(&mut file);
    assert!(res.is_err());
}

#[test]
fn test_gv09_corrupt_unsupported_codec() {
    let path = fixtures_dir().join("gv09_corrupt_unsupported_codec.wx1");
    let mut bytes = build_gv01_bytes();

    // The chunk descriptor directory contains chunk descriptors with codec byte.
    // Replace valid codec with unsupported 0x99
    let mut found = false;
    for i in 64..bytes.len() - 48 {
        if bytes[i] == 1 && bytes[i + 1] == 0 && bytes[i + 2] == 0 {
            bytes[i] = 0x99;
            found = true;
            break;
        }
    }
    assert!(found);
    std::fs::write(&path, &bytes).unwrap();

    let mut file = File::open(&path).unwrap();
    let res = open_generation_snapshot(&mut file);
    assert!(res.is_err());
}

#[test]
fn test_gv10_malicious_path_traversal() {
    let path = fixtures_dir().join("gv10_malicious_path_traversal.wx1");
    let mut bytes = build_gv01_bytes();

    // Replace the variable name "t2m" with malicious traversal "../"
    let t2m_pos = bytes
        .windows(3)
        .position(|w| w == b"t2m")
        .expect("variable name t2m should be present in metadata");
    bytes[t2m_pos..t2m_pos + 3].copy_from_slice(b"../");
    std::fs::write(&path, &bytes).unwrap();

    let mut file = File::open(&path).unwrap();
    let res = open_scientific_snapshot(&mut file);
    assert!(
        res.is_err(),
        "malicious variable name '../' must be rejected"
    );
}

#[test]
fn test_gv11_malicious_decompression_bomb() {
    let path = fixtures_dir().join("gv11_malicious_decompression_bomb.wx1");
    let mut bytes = build_gv01_bytes();

    // Find the raw_len 1024 (0x00000400u64) in the chunk descriptor and change to 100 MiB (0x06400000u64)
    // which exceeds MAX_CHUNK_BYTES (64 MiB).
    let target = 1024u64.to_le_bytes();
    let bomb_len = (100 * 1024 * 1024u64).to_le_bytes();
    let mut replaced = false;
    for i in (64..bytes.len() - 48).rev() {
        if bytes[i..i + 8] == target {
            bytes[i..i + 8].copy_from_slice(&bomb_len);
            replaced = true;
            break;
        }
    }
    assert!(replaced, "should find and replace chunk descriptor raw_len");
    std::fs::write(&path, &bytes).unwrap();

    let mut file = File::open(&path).unwrap();
    let res = open_generation_snapshot(&mut file);
    assert!(
        matches!(res, Err(ChunkIoError::ChunkTooLarge)),
        "100 MiB declared uncompressed chunk must be rejected with ChunkTooLarge"
    );
}
