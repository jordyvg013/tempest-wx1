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

use std::env;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::process::ExitCode;
use wx1_format::{
    append_generation_file, append_generation_file_with_indexes, copy_chunk_payload_to_writer,
    inspect_legacy_prefix, open_generation_snapshot, open_scientific_snapshot,
    read_chunk_directory, read_container, repair_generation_file, write_chunk_container,
    write_chunk_container_with_codecs, write_committed_chunk_container, write_container,
    write_generation_container_atomic_with_indexes, write_generation_container_from_readers_atomic,
    ArrayChunkRef, ArraySelection, AttributeValue, ChunkCodec, ChunkInput, ChunkReadOptions,
    ChunkReaderInput, ChunkWriteOptions, ContainerHeader, Coordinate, CoordinateAxis, DataType,
    Dimension, DimensionRole, EncodedChunkInput, GridMapping, HeaderByteOrder, LogicalIndexKind,
    LongitudeConvention, Metadata, PayloadByteOrder, Provenance, ScalarValue, ScientificChunkKey,
    ScientificDataset, Variable, CONTAINER_HEADER_LEN, GENERATION_PROFILE_MINOR_VERSION,
    LEGACY_GENERATION_PROFILE_MINOR_VERSION, LEGACY_PREFIX_LEN,
    PERSISTENT_INDEX_PROFILE_MINOR_VERSION,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("wx1: {message}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args_os();
    let _program = args.next();
    let Some(command) = args.next() else {
        return Err(usage());
    };
    if command == "--help" || command == "-h" {
        println!("{}", usage());
        return Ok(());
    }

    if command == "create-demo" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        let Some(dataset_id) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        let dataset_id = utf8_arg(dataset_id, "dataset ID")?;
        let mut metadata = Metadata::new(dataset_id);
        metadata
            .attributes
            .insert("created_by".into(), "wx1-cli".into());
        metadata
            .attributes
            .insert("profile".into(), "experimental-metadata-only".into());
        let mut file = File::create(&path).map_err(|e| format!("cannot create file: {e}"))?;
        write_container(&mut file, &metadata, HeaderByteOrder::LittleEndian)
            .map_err(|e| format!("cannot write WX-1 container: {e}"))?;
        println!(
            "Created experimental WX-1 metadata container: {}",
            path.to_string_lossy()
        );
        println!("Dataset ID: {}", metadata.dataset_id);
        Ok(())
    } else if command == "create-chunk-demo" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        let Some(dataset_id) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        let dataset_id = utf8_arg(dataset_id, "dataset ID")?;
        let mut metadata = Metadata::new(dataset_id);
        metadata
            .attributes
            .insert("created_by".into(), "wx1-cli".into());
        metadata
            .attributes
            .insert("profile".into(), "experimental-raw-chunks".into());
        let chunks = [
            ChunkInput {
                id: 1,
                data: b"WX-1 demo temperature chunk\n",
            },
            ChunkInput {
                id: 2,
                data: b"WX-1 demo wind chunk\n",
            },
        ];
        let mut file = File::create(&path).map_err(|e| format!("cannot create file: {e}"))?;
        write_chunk_container(&mut file, &metadata, &chunks, HeaderByteOrder::LittleEndian)
            .map_err(|e| format!("cannot write WX-1 chunk container: {e}"))?;
        println!(
            "Created experimental WX-1 chunk container: {}",
            path.to_string_lossy()
        );
        println!("Dataset ID: {}", metadata.dataset_id);
        println!("Chunks: {}", chunks.len());
        Ok(())
    } else if command == "create-compressed-demo" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        let Some(dataset_id) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        let dataset_id = utf8_arg(dataset_id, "dataset ID")?;
        let mut metadata = Metadata::new(dataset_id);
        metadata
            .attributes
            .insert("created_by".into(), "wx1-cli".into());
        metadata
            .attributes
            .insert("profile".into(), "experimental-lossless-codecs".into());
        let temperature = vec![b'T'; 256 * 1024];
        let wind = (0..128 * 1024)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let chunks = [
            EncodedChunkInput {
                id: 1,
                data: &temperature,
                codec: ChunkCodec::Zstandard,
            },
            EncodedChunkInput {
                id: 2,
                data: &wind,
                codec: ChunkCodec::Lz4,
            },
            EncodedChunkInput {
                id: 3,
                data: b"uncompressed metadata sample",
                codec: ChunkCodec::Raw,
            },
        ];
        let mut file = File::create(&path).map_err(|e| format!("cannot create file: {e}"))?;
        write_chunk_container_with_codecs(
            &mut file,
            &metadata,
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .map_err(|e| format!("cannot write compressed WX-1 container: {e}"))?;
        println!(
            "Created experimental WX-1 compressed chunk container: {}",
            path.to_string_lossy()
        );
        println!("Dataset ID: {}", metadata.dataset_id);
        println!("Chunks: {} (Zstandard, LZ4, raw)", chunks.len());
        Ok(())
    } else if command == "create-committed-demo" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        let Some(dataset_id) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        let dataset_id = utf8_arg(dataset_id, "dataset ID")?;
        let mut metadata = Metadata::new(dataset_id);
        metadata
            .attributes
            .insert("created_by".into(), "wx1-cli".into());
        metadata
            .attributes
            .insert("profile".into(), "experimental-blake3-committed".into());
        let payload = vec![b'C'; 256 * 1024];
        let chunks = [EncodedChunkInput {
            id: 1,
            data: &payload,
            codec: ChunkCodec::Zstandard,
        }];
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| format!("cannot create file: {e}"))?;
        write_committed_chunk_container(
            &mut file,
            &metadata,
            &chunks,
            HeaderByteOrder::LittleEndian,
        )
        .map_err(|e| format!("cannot write committed WX-1 container: {e}"))?;
        println!(
            "Created experimental WX-1 container with BLAKE3 commit footer: {}",
            path.to_string_lossy()
        );
        println!("Dataset ID: {}", metadata.dataset_id);
        println!("Chunks: 1");
        Ok(())
    } else if command == "create-generation-demo" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        let Some(dataset_id) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        let dataset_id = utf8_arg(dataset_id, "dataset ID")?;
        let mut metadata = Metadata::new(dataset_id);
        metadata
            .attributes
            .insert("created_by".into(), "wx1-cli".into());
        metadata.attributes.insert(
            "profile".into(),
            "directory-v2-append-only-generations".into(),
        );
        let temperature = vec![b'T'; 64 * 1024];
        let wind = vec![b'W'; 32 * 1024];
        let chunks = [
            EncodedChunkInput {
                id: 1,
                data: &temperature,
                codec: ChunkCodec::Zstandard,
            },
            EncodedChunkInput {
                id: 2,
                data: &wind,
                codec: ChunkCodec::Raw,
            },
        ];
        let temperature_key = ScientificChunkKey {
            variable_id: 1,
            time_index: 0,
            level_index: 850,
            spatial_tile: 10,
        };
        let wind_key = ScientificChunkKey {
            variable_id: 2,
            time_index: 0,
            level_index: 10,
            spatial_tile: 10,
        };
        let index_entries = [
            temperature_key.persistent_entries(1),
            wind_key.persistent_entries(2),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        write_generation_container_atomic_with_indexes(
            &path,
            &metadata,
            &chunks,
            &index_entries,
            HeaderByteOrder::LittleEndian,
        )
        .map_err(|e| format!("cannot publish WX-1 generation container: {e}"))?;
        println!(
            "Created durable WX-1 generation container: {}",
            path.to_string_lossy()
        );
        println!("Dataset ID: {}", metadata.dataset_id);
        println!("Generation: 1");
        println!("Chunks: {}", chunks.len());
        Ok(())
    } else if command == "create-scientific-demo" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        let Some(dataset_id) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        create_scientific_demo(&path, &utf8_arg(dataset_id, "dataset ID")?)
    } else if command == "validate-scientific" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        validate_scientific(&path)
    } else if command == "read-array-slice" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        let Some(variable) = args.next() else {
            return Err(usage());
        };
        let Some(start) = args.next() else {
            return Err(usage());
        };
        let Some(count) = args.next() else {
            return Err(usage());
        };
        let Some(output) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        read_array_slice_command(
            &path,
            &utf8_arg(variable, "variable path")?,
            &utf8_arg(start, "slice start")?,
            &utf8_arg(count, "slice count")?,
            &output,
        )
    } else if command == "append-generation" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        let Some(chunk_id) = args.next() else {
            return Err(usage());
        };
        let Some(payload) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        let chunk_id = utf8_arg(chunk_id, "chunk ID")?
            .parse::<u64>()
            .map_err(|_| "chunk ID must be an unsigned 64-bit integer".to_owned())?;
        let payload = utf8_arg(payload, "payload")?;
        let chunks = [EncodedChunkInput {
            id: chunk_id,
            data: payload.as_bytes(),
            codec: ChunkCodec::Raw,
        }];
        let generation = append_generation_file(&path, &chunks)
            .map_err(|e| format!("cannot append WX-1 generation: {e}"))?;
        println!(
            "Committed WX-1 generation {}: {}",
            generation,
            path.to_string_lossy()
        );
        println!(
            "Appended chunk: {chunk_id} ({} stored bytes)",
            payload.len()
        );
        Ok(())
    } else if command == "append-indexed-demo" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        let Some(chunk_id) = args.next() else {
            return Err(usage());
        };
        let Some(payload) = args.next() else {
            return Err(usage());
        };
        let Some(variable_id) = args.next() else {
            return Err(usage());
        };
        let Some(time_index) = args.next() else {
            return Err(usage());
        };
        let Some(level_index) = args.next() else {
            return Err(usage());
        };
        let Some(spatial_tile) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        let chunk_id = parse_u64_arg(chunk_id, "chunk ID")?;
        let payload = utf8_arg(payload, "payload")?;
        let key = ScientificChunkKey {
            variable_id: parse_u32_arg(variable_id, "variable ID")?,
            time_index: parse_u64_arg(time_index, "time index")?,
            level_index: parse_u32_arg(level_index, "level index")?,
            spatial_tile: parse_u64_arg(spatial_tile, "spatial tile")?,
        };
        let chunks = [EncodedChunkInput {
            id: chunk_id,
            data: payload.as_bytes(),
            codec: ChunkCodec::Raw,
        }];
        let entries = key.persistent_entries(chunk_id);
        let generation = append_generation_file_with_indexes(&path, &chunks, &entries)
            .map_err(|e| format!("cannot append indexed WX-1 generation: {e}"))?;
        println!(
            "Committed indexed WX-1 generation {generation}: {}",
            path.to_string_lossy()
        );
        println!(
            "Chunk: {chunk_id}; variable={}, time-index={}, level-index={}, spatial-tile={}",
            key.variable_id, key.time_index, key.level_index, key.spatial_tile
        );
        Ok(())
    } else if command == "index-query" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        let Some(kind) = args.next() else {
            return Err(usage());
        };
        let Some(key_hex) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        let kind = parse_index_kind(&utf8_arg(kind, "index kind")?)?;
        let key = parse_index_key(&utf8_arg(key_hex, "index key")?)?;
        let mut file = File::open(&path).map_err(|e| format!("cannot open file: {e}"))?;
        let snapshot = open_generation_snapshot(&mut file)
            .map_err(|e| format!("invalid WX-1 generation container: {e}"))?;
        let matches = snapshot
            .lookup_index_exact(&mut file, kind, &key)
            .map_err(|e| format!("index query failed: {e}"))?;
        println!("Generation: {}", snapshot.generation());
        println!("Index: {}", index_kind_name(kind));
        println!("Matches: {}", matches.len());
        for chunk_id in matches {
            println!("  chunk_id={chunk_id}");
        }
        Ok(())
    } else if command == "inspect-indexes" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        inspect_indexes(&path)
    } else if command == "repair" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        let report = repair_generation_file(&path)
            .map_err(|e| format!("cannot repair WX-1 generation tail: {e}"))?;
        println!(
            "WX-1 repair complete: retained generation {}",
            report.generation
        );
        println!("Original length: {} bytes", report.original_len);
        println!("Retained length: {} bytes", report.retained_len);
        println!(
            "Discarded tail: {} bytes",
            report.original_len - report.retained_len
        );
        Ok(())
    } else if command == "inspect-chunks" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        inspect_chunks(&path)
    } else if command == "inspect" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        inspect(&path)
    } else if command == "inspect-legacy-prefix" {
        let Some(path) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        let mut file = File::open(&path).map_err(|error| format!("cannot open file: {error}"))?;
        let mut prefix = [0_u8; LEGACY_PREFIX_LEN];
        file.read_exact(&mut prefix)
            .map_err(|error| format!("cannot read legacy prefix: {error}"))?;
        let info = inspect_legacy_prefix(&prefix).map_err(|error| error.to_string())?;
        println!("Legacy experimental WX-1 prefix detected");
        println!("Header length: {} bytes", info.header_len);
        println!("This is not full-file validation and not a stable WX-1 format.");
        Ok(())
    } else {
        Err(usage())
    }
}

fn create_scientific_demo(path: &std::ffi::OsStr, dataset_id: &str) -> Result<(), String> {
    let output_path = std::path::Path::new(path);
    if output_path.exists() {
        return Err(
            "destination already exists; remove it before creating a new scientific demo"
                .to_owned(),
        );
    }

    let mut temperature_attributes = std::collections::BTreeMap::new();
    temperature_attributes.insert("units".to_owned(), AttributeValue::Text("K".to_owned()));
    temperature_attributes.insert(
        "standard_name".to_owned(),
        AttributeValue::Text("air_temperature".to_owned()),
    );
    temperature_attributes.insert(
        "long_name".to_owned(),
        AttributeValue::Text("synthetic near-surface air temperature".to_owned()),
    );

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
    model.variables = vec![
        Variable {
            name: "temperature".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::F32,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["y".to_owned(), "x".to_owned()],
            chunk_shape: vec![2, 2],
            fill_value: Some(ScalarValue::F32Bits(f32::NAN.to_bits())),
            missing_value: None,
            attributes: temperature_attributes,
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
            grid_mapping: Some("crs".to_owned()),
        },
        Variable {
            name: "latitude".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::F64,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["y".to_owned()],
            chunk_shape: vec![4],
            fill_value: None,
            missing_value: None,
            attributes: std::collections::BTreeMap::from([(
                "units".to_owned(),
                AttributeValue::Text("degrees_north".to_owned()),
            )]),
            chunks: vec![ArrayChunkRef {
                chunk_id: 5,
                origin: vec![0],
                shape: vec![4],
            }],
            grid_mapping: Some("crs".to_owned()),
        },
        Variable {
            name: "longitude".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::F64,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["x".to_owned()],
            chunk_shape: vec![4],
            fill_value: None,
            missing_value: None,
            attributes: std::collections::BTreeMap::from([(
                "units".to_owned(),
                AttributeValue::Text("degrees_east".to_owned()),
            )]),
            chunks: vec![ArrayChunkRef {
                chunk_id: 6,
                origin: vec![0],
                shape: vec![4],
            }],
            grid_mapping: Some("crs".to_owned()),
        },
    ];
    model.coordinates = vec![
        Coordinate {
            name: "latitude".to_owned(),
            variable: "/latitude".to_owned(),
            axis: CoordinateAxis::Latitude,
            time: None,
            vertical: None,
            longitude_convention: None,
        },
        Coordinate {
            name: "longitude".to_owned(),
            variable: "/longitude".to_owned(),
            axis: CoordinateAxis::Longitude,
            time: None,
            vertical: None,
            longitude_convention: Some(LongitudeConvention::Minus180To180),
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
        source: "wx1-cli synthetic example".to_owned(),
        provider: Some("WX-1".to_owned()),
        product: Some("4 by 4 temperature field".to_owned()),
        institution: None,
        license: Some("CC0-1.0".to_owned()),
        history: Some("generated deterministically by create-scientific-demo".to_owned()),
        reference_time: None,
    });

    let mut metadata = Metadata::new(dataset_id).with_scientific_model(model);
    metadata
        .attributes
        .insert("created_by".to_owned(), "wx1-cli".to_owned());
    metadata.attributes.insert(
        "profile".to_owned(),
        "scientific-schema-v2-streaming".to_owned(),
    );

    let values = [
        [280.0f32, 281.0, 284.0, 285.0],
        [282.0f32, 283.0, 286.0, 287.0],
        [288.0f32, 289.0, 292.0, 293.0],
        [290.0f32, 291.0, 294.0, 295.0],
    ];
    let temperature_chunks: Vec<Vec<u8>> = values
        .iter()
        .map(|row| row.iter().flat_map(|value| value.to_le_bytes()).collect())
        .collect();
    let latitude_bytes: Vec<u8> = [51.0f64, 51.1, 51.2, 51.3]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    let longitude_bytes: Vec<u8> = [4.0f64, 4.1, 4.2, 4.3]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();

    let mut chunk1 = std::io::Cursor::new(temperature_chunks[0].clone());
    let mut chunk2 = std::io::Cursor::new(temperature_chunks[1].clone());
    let mut chunk3 = std::io::Cursor::new(temperature_chunks[2].clone());
    let mut chunk4 = std::io::Cursor::new(temperature_chunks[3].clone());
    let mut chunk5 = std::io::Cursor::new(latitude_bytes);
    let mut chunk6 = std::io::Cursor::new(longitude_bytes);
    let mut chunks = [
        ChunkReaderInput {
            id: 1,
            reader: &mut chunk1,
            raw_len: 16,
            codec: ChunkCodec::Zstandard,
        },
        ChunkReaderInput {
            id: 2,
            reader: &mut chunk2,
            raw_len: 16,
            codec: ChunkCodec::Raw,
        },
        ChunkReaderInput {
            id: 3,
            reader: &mut chunk3,
            raw_len: 16,
            codec: ChunkCodec::Lz4,
        },
        ChunkReaderInput {
            id: 4,
            reader: &mut chunk4,
            raw_len: 16,
            codec: ChunkCodec::Zstandard,
        },
        ChunkReaderInput {
            id: 5,
            reader: &mut chunk5,
            raw_len: 32,
            codec: ChunkCodec::Zstandard,
        },
        ChunkReaderInput {
            id: 6,
            reader: &mut chunk6,
            raw_len: 32,
            codec: ChunkCodec::Raw,
        },
    ];

    write_generation_container_from_readers_atomic(
        path,
        &metadata,
        &mut chunks,
        HeaderByteOrder::LittleEndian,
        ChunkWriteOptions::default(),
    )
    .map_err(|error| format!("cannot publish scientific WX-1 file: {error}"))?;
    println!(
        "Created scientific WX-1 dataset: {}",
        path.to_string_lossy()
    );
    println!("Dataset ID: {}", metadata.dataset_id);
    println!("Variable: /temperature (F32, shape 4x4, chunks 2x2)");
    println!("Coordinates: /latitude, /longitude (EPSG:4326)");
    println!("Chunks: 6 (Zstandard, raw and LZ4 block)");
    println!("Generation: 1; header minor: {GENERATION_PROFILE_MINOR_VERSION}");
    Ok(())
}

fn validate_scientific(path: &std::ffi::OsStr) -> Result<(), String> {
    let mut file = File::open(path).map_err(|error| format!("cannot open file: {error}"))?;
    let snapshot = open_generation_snapshot(&mut file)
        .map_err(|error| format!("invalid WX-1 generation container: {error}"))?;
    let model = snapshot
        .metadata()
        .scientific_model()
        .ok_or_else(|| "WX-1 file does not contain a scientific schema-v2 model".to_owned())?;
    let mut verified = 0usize;
    for descriptor in snapshot.descriptors() {
        snapshot
            .read_chunk(&mut file, descriptor.id)
            .map_err(|error| {
                format!("cannot decode scientific chunk {}: {error}", descriptor.id)
            })?;
        verified += 1;
    }
    println!("WX-1 scientific dataset validated");
    println!("Dataset ID: {}", snapshot.metadata().dataset_id);
    println!("Generation: {}", snapshot.generation());
    println!("Variables: {}", model.variables.len());
    println!("Chunks verified: {verified}");
    println!("Metadata, chunk map, stored-byte CRC32C/BLAKE3 and decoded lengths are valid.");
    Ok(())
}

fn read_array_slice_command(
    input: &std::ffi::OsStr,
    variable_path: &str,
    start_text: &str,
    count_text: &str,
    output: &std::ffi::OsStr,
) -> Result<(), String> {
    let input_path = std::path::Path::new(input);
    let output_path = std::path::Path::new(output);
    if input_path == output_path {
        return Err("array output path must differ from the input WX-1 file".to_owned());
    }
    if output_path.exists() {
        return Err("array output already exists; remove it before writing a new slice".to_owned());
    }
    let start = parse_csv_u64(start_text, "slice start")?;
    let count = parse_csv_u64(count_text, "slice count")?;
    let mut file =
        File::open(input_path).map_err(|error| format!("cannot open WX-1 file: {error}"))?;
    let snapshot = open_scientific_snapshot(&mut file)
        .map_err(|error| format!("cannot open scientific WX-1 dataset: {error}"))?;
    let array = snapshot
        .read_array_slice(&mut file, variable_path, &ArraySelection::new(start, count))
        .map_err(|error| format!("cannot read scientific array slice: {error}"))?;

    let mut output_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)
        .map_err(|error| format!("cannot create array output: {error}"))?;
    let write_result = (|| {
        output_file.write_all(&array.data)?;
        output_file.flush()?;
        output_file.sync_all()
    })();
    if let Err(error) = write_result {
        drop(output_file);
        let _ = std::fs::remove_file(output_path);
        return Err(format!("cannot write array output: {error}"));
    }
    println!("Array slice written: {}", output_path.to_string_lossy());
    println!("Variable: {variable_path}");
    println!(
        "Datatype: {:?}; byte order: {:?}",
        array.data_type, array.byte_order
    );
    println!(
        "Shape: [{}]",
        array
            .shape
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    println!(
        "Bytes: {} (raw array payload; no file header)",
        array.data.len()
    );
    Ok(())
}

fn parse_csv_u64(value: &str, label: &str) -> Result<Vec<u64>, String> {
    if value.is_empty() {
        return Err(format!(
            "{label} must contain comma-separated unsigned integers"
        ));
    }
    value
        .split(',')
        .map(|part| {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                return Err(format!("{label} contains an empty dimension value"));
            }
            trimmed.parse::<u64>().map_err(|_| {
                format!("{label} contains a value that is not an unsigned 64-bit integer")
            })
        })
        .collect()
}

fn inspect(path: &std::ffi::OsStr) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| format!("cannot open file: {e}"))?;
    let header = read_header(&mut file)?;
    if header.minor > GENERATION_PROFILE_MINOR_VERSION {
        return Err(format!(
            "unsupported WX-1 container minor version {}",
            header.minor
        ));
    }
    if is_generation_profile_minor(header.minor) {
        let snapshot = open_generation_snapshot(&mut file)
            .map_err(|e| format!("invalid WX-1 generation container: {e}"))?;
        // Decode every chunk before emitting any header/metadata output.
        for descriptor in snapshot.descriptors() {
            snapshot
                .read_chunk(&mut file, descriptor.id)
                .map_err(|e| format!("cannot validate chunk {}: {e}", descriptor.id))?;
        }
        print_container_metadata("WX-1 append-only generation container", snapshot.metadata());
        println!("Generation: {}", snapshot.generation());
        println!("Directory offset: {}", snapshot.directory_offset());
        println!("Visible chunks: {}", snapshot.descriptors().len());
        print_index_catalog(&snapshot);
        println!("All visible stored-byte CRC32C, BLAKE3 digests and decoded lengths validated.");
    } else {
        let metadata =
            read_container(&mut file).map_err(|e| format!("invalid WX-1 container: {e}"))?;
        print_container_metadata("WX-1 experimental container", &metadata);
        println!("Metadata-only container validated; use inspect-chunks for containers with chunk payloads.");
    }
    Ok(())
}

fn inspect_chunks(path: &std::ffi::OsStr) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| format!("cannot open file: {e}"))?;
    let header = read_header(&mut file)?;
    let mut validated = Vec::<(wx1_format::ChunkDescriptor, u64)>::new();
    let (metadata, generation, index_lines) = if is_generation_profile_minor(header.minor) {
        let snapshot = open_generation_snapshot(&mut file)
            .map_err(|e| format!("invalid WX-1 generation container: {e}"))?;
        validated
            .try_reserve_exact(snapshot.descriptors().len())
            .map_err(|_| "not enough memory for validation report".to_owned())?;
        for descriptor in snapshot.descriptors() {
            let decoded = snapshot
                .read_chunk(&mut file, descriptor.id)
                .map_err(|e| format!("cannot validate chunk {}: {e}", descriptor.id))?;
            validated.push((*descriptor, decoded.len() as u64));
        }
        (
            snapshot.metadata().clone(),
            Some(snapshot.generation()),
            collect_index_lines(&snapshot),
        )
    } else {
        let (metadata, descriptors) = read_chunk_directory(&mut file)
            .map_err(|e| format!("invalid WX-1 chunk container: {e}"))?;
        validated
            .try_reserve_exact(descriptors.len())
            .map_err(|_| "not enough memory for validation report".to_owned())?;
        for descriptor in descriptors {
            let verified = copy_chunk_payload_to_writer(
                &mut file,
                descriptor,
                &mut std::io::sink(),
                ChunkReadOptions::default(),
            )
            .map_err(|e| format!("cannot validate chunk {}: {e}", descriptor.id))?;
            validated.push((descriptor, verified));
        }
        (metadata, None, Vec::new())
    };

    // Do not print a success header or partial report until every chunk has passed.
    println!("WX-1 chunk container");
    println!("Dataset ID: {}", metadata.dataset_id);
    if let Some(generation) = generation {
        println!("Generation: {generation}");
    }
    println!("Chunks: {}", validated.len());
    for line in &index_lines {
        println!("{line}");
    }
    for (descriptor, verified_bytes) in &validated {
        let digest = descriptor
            .blake3
            .map(hex_digest)
            .unwrap_or_else(|| "not-present (directory v1)".to_owned());
        println!(
            "  id={} codec={:?} offset={} stored={} raw={} crc32c={:08x} blake3={} verified_bytes={}",
            descriptor.id, descriptor.codec, descriptor.offset, descriptor.stored_len,
            descriptor.raw_len, descriptor.crc32c, digest, verified_bytes,
        );
    }
    if generation.is_some() {
        println!("Generation chain, directories, stored-byte CRC32C, per-chunk BLAKE3 and decoded lengths validated.");
    } else {
        println!("Legacy directory validated; stored-byte CRC32C and decoded lengths verified. Optional v1 WX1C footer is checked when present.");
    }
    Ok(())
}

fn is_generation_profile_minor(minor: u16) -> bool {
    minor == LEGACY_GENERATION_PROFILE_MINOR_VERSION
        || minor == PERSISTENT_INDEX_PROFILE_MINOR_VERSION
        || minor == GENERATION_PROFILE_MINOR_VERSION
}

fn parse_u64_arg(value: std::ffi::OsString, label: &str) -> Result<u64, String> {
    utf8_arg(value, label)?
        .parse::<u64>()
        .map_err(|_| format!("{label} must be an unsigned 64-bit integer"))
}

fn parse_u32_arg(value: std::ffi::OsString, label: &str) -> Result<u32, String> {
    utf8_arg(value, label)?
        .parse::<u32>()
        .map_err(|_| format!("{label} must be an unsigned 32-bit integer"))
}

fn parse_index_key(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 {
        return Err("index key must be exactly 64 hexadecimal characters".to_owned());
    }
    let mut key = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0])
            .ok_or_else(|| "index key contains non-hexadecimal characters".to_owned())?;
        let low = hex_nibble(pair[1])
            .ok_or_else(|| "index key contains non-hexadecimal characters".to_owned())?;
        key[index] = (high << 4) | low;
    }
    Ok(key)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_index_kind(value: &str) -> Result<LogicalIndexKind, String> {
    match value {
        "chunk-id" => Ok(LogicalIndexKind::ChunkId),
        "variable" => Ok(LogicalIndexKind::Variable),
        "time" => Ok(LogicalIndexKind::Time),
        "vertical" => Ok(LogicalIndexKind::Vertical),
        "spatial" => Ok(LogicalIndexKind::Spatial),
        "model-provider" => Ok(LogicalIndexKind::ModelProvider),
        "scientific-manifest" => Ok(LogicalIndexKind::ScientificManifest),
        _ => Err(
            "index kind must be chunk-id, variable, time, vertical, spatial, or model-provider"
                .to_owned(),
        ),
    }
}

fn index_kind_name(kind: LogicalIndexKind) -> &'static str {
    match kind {
        LogicalIndexKind::ChunkId => "chunk-id",
        LogicalIndexKind::Variable => "variable",
        LogicalIndexKind::Time => "time",
        LogicalIndexKind::Vertical => "vertical",
        LogicalIndexKind::Spatial => "spatial",
        LogicalIndexKind::ModelProvider => "model-provider",
        LogicalIndexKind::ScientificManifest => "scientific-manifest",
    }
}

fn collect_index_lines(snapshot: &wx1_format::GenerationSnapshot) -> Vec<String> {
    [
        LogicalIndexKind::ChunkId,
        LogicalIndexKind::Variable,
        LogicalIndexKind::Time,
        LogicalIndexKind::Vertical,
        LogicalIndexKind::Spatial,
        LogicalIndexKind::ModelProvider,
        LogicalIndexKind::ScientificManifest,
    ]
    .into_iter()
    .filter_map(|kind| {
        snapshot.index_info(kind).map(|info| {
            format!(
                "  index={} entries={} level={} root={} required={}",
                index_kind_name(kind),
                info.item_count,
                info.level,
                info.root_offset,
                info.required,
            )
        })
    })
    .collect()
}

fn print_index_catalog(snapshot: &wx1_format::GenerationSnapshot) {
    for line in collect_index_lines(snapshot) {
        println!("{line}");
    }
}

fn inspect_indexes(path: &std::ffi::OsStr) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| format!("cannot open file: {e}"))?;
    let snapshot = open_generation_snapshot(&mut file)
        .map_err(|e| format!("invalid WX-1 generation container: {e}"))?;
    for descriptor in snapshot.descriptors() {
        snapshot
            .read_chunk(&mut file, descriptor.id)
            .map_err(|e| format!("cannot validate chunk {}: {e}", descriptor.id))?;
    }
    let lines = collect_index_lines(&snapshot);
    println!("WX-1 persistent indexes");
    println!("Generation: {}", snapshot.generation());
    if lines.is_empty() {
        println!("No persistent indexes are present in this generation profile.");
    } else {
        for line in lines {
            println!("{line}");
        }
    }
    Ok(())
}

fn print_container_metadata(title: &str, metadata: &Metadata) {
    println!("{title}");
    println!("Dataset ID: {}", metadata.dataset_id);
    println!("Attributes: {}", metadata.attributes.len());
    for (key, value) in &metadata.attributes {
        println!("  {key}: {value}");
    }
    if let Some(model) = metadata.scientific_model() {
        println!(
            "Scientific model schema: {}",
            wx1_format::SCIENTIFIC_SCHEMA_VERSION
        );
        println!("Groups: {}", model.groups.len());
        println!("Dimensions: {}", model.dimensions.len());
        for dimension in &model.dimensions {
            println!(
                "  {}: {} ({:?})",
                dimension.name, dimension.length, dimension.role
            );
        }
        println!("Variables: {}", model.variables.len());
        for variable in &model.variables {
            let dimensions = variable.dimensions.join(",");
            println!(
                "  {}: {:?} [{}], chunks={}",
                variable.path(),
                variable.data_type,
                dimensions,
                variable.chunks.len()
            );
        }
        println!("Coordinates: {}", model.coordinates.len());
        println!("Grid mappings: {}", model.grid_mappings.len());
        println!("Provenance records: {}", model.provenance.len());
    }
}

fn read_header(file: &mut File) -> Result<ContainerHeader, String> {
    let file_len = file
        .metadata()
        .map_err(|e| format!("cannot stat file: {e}"))?
        .len();
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("cannot seek to header: {e}"))?;
    let mut bytes = [0u8; CONTAINER_HEADER_LEN];
    file.read_exact(&mut bytes)
        .map_err(|e| format!("cannot read WX-1 header: {e}"))?;
    let header =
        ContainerHeader::decode(&bytes).map_err(|e| format!("invalid WX-1 header: {e}"))?;
    header
        .validate_file_bounds(file_len)
        .map_err(|e| format!("invalid WX-1 header bounds: {e}"))?;
    Ok(header)
}

fn utf8_arg(value: std::ffi::OsString, label: &str) -> Result<String, String> {
    value
        .into_string()
        .map_err(|_| format!("{label} must be valid UTF-8"))
}

fn hex_digest(bytes: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn usage() -> String {
    "WX-1 standalone tools (experimental)\n\nUsage:\n  wx1 --help\n  wx1 create-demo <file> <dataset-id>\n  wx1 inspect <file>\n  wx1 create-chunk-demo <file> <dataset-id>\n  wx1 create-compressed-demo <file> <dataset-id>\n  wx1 create-committed-demo <file> <dataset-id>\n  wx1 create-generation-demo <file> <dataset-id>\n  wx1 create-scientific-demo <file> <dataset-id>\n  wx1 validate-scientific <file>\n  wx1 read-array-slice <file> <variable-path> <start-csv> <count-csv> <output.raw>\n  wx1 append-generation <file> <chunk-id> <payload-text>\n  wx1 append-indexed-demo <file> <chunk-id> <payload-text> <variable-id> <time-index> <level-index> <spatial-tile>\n  wx1 inspect-indexes <file>\n  wx1 index-query <file> <kind> <64-hex-byte-key>\n  wx1 inspect-chunks <file>\n  wx1 repair <file>\n  wx1 inspect-legacy-prefix <file>\n\ncreate-generation-demo writes generation 1 with persistent chunk-ID, variable, time, vertical and spatial B+tree indexes. append-indexed-demo appends one chunk and explicit scientific ordinal mappings using copy-on-write B+tree updates. index-query performs an exact lookup with a 32-byte key encoded as 64 hexadecimal characters. inspect and inspect-chunks validate the complete generation and its indexes before printing. v0.3.8 (minor 4), v0.3.9 (minor 5) and v0.4.x (minor 6) generation files are supported. v0.4.x uses header minor 6 and metadata schema v2 for scientific arrays; inspect prints the scientific model summary. create-scientific-demo writes a synthetic 4x4 temperature dataset through the bounded reader-based codec pipeline. validate-scientific validates all scientific chunks; read-array-slice writes a bounded row-major raw payload without converting datatype or byte order. Scientific-model files are immutable at the metadata-manifest level and cannot be appended. repair explicitly truncates only bytes after the newest fully valid generation. Only one writer per file is supported; serialize append/repair externally.\n".to_owned()
}
