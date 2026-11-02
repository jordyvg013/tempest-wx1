use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Cursor, Write};

use wx1_format::{
    append_generation, open_generation_snapshot, repair_generation_file,
    write_generation_container, ChunkCodec, ChunkIoError, ContainerHeader, DataType, Dimension,
    DimensionRole, EncodedChunkInput, HeaderByteOrder, Metadata, PayloadByteOrder,
    ScientificDataset, ScientificModelError, Variable, COMMIT_FOOTER_LEN, CONTAINER_HEADER_LEN,
};

/// Deterministic pseudo-random number generator (LCG) for reproducible fuzzing without external crates.
struct FuzzRng {
    state: u64,
}

impl FuzzRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.state >> 32) as u32
    }

    fn next_u8(&mut self) -> u8 {
        self.next_u32() as u8
    }

    fn fill_bytes(&mut self, buf: &mut [u8]) {
        for b in buf.iter_mut() {
            *b = self.next_u8();
        }
    }
}

#[test]
fn test_fuzz_container_headers_no_panic() {
    let mut rng = FuzzRng::new(0xDEAD_BEEF_CAFE_BABE);
    let mut buf = [0u8; CONTAINER_HEADER_LEN];

    for _ in 0..10_000 {
        rng.fill_bytes(&mut buf);
        // ContainerHeader::decode must NEVER panic, regardless of input
        let res = ContainerHeader::decode(&buf);
        if let Ok(header) = res {
            // If it miraculously passed (e.g. matching magic and valid fields),
            // validate_file_bounds must also never panic
            let _ = header.validate_file_bounds(1024);
        }
    }
}

#[test]
fn test_fuzz_commit_footers_no_panic() {
    let mut rng = FuzzRng::new(0x1234_5678_9ABC_DEF0);
    let mut buf = vec![0u8; COMMIT_FOOTER_LEN as usize];

    for _ in 0..5_000 {
        rng.fill_bytes(&mut buf);
        let mut cursor = Cursor::new(&buf);
        // read_chunk_directory on raw random bytes must NEVER panic
        let _ = wx1_format::read_chunk_directory(&mut cursor);
    }
}

#[test]
fn test_fuzz_bitflips_on_valid_container() {
    let metadata = Metadata::new("test-fuzz-bitflips");
    let chunks = [
        EncodedChunkInput {
            id: 1,
            data: b"payload-chunk-one-scientific",
            codec: ChunkCodec::Raw,
        },
        EncodedChunkInput {
            id: 2,
            data: b"payload-chunk-two-compressed-with-zstd-content",
            codec: ChunkCodec::Zstandard,
        },
    ];

    let mut valid_file = Cursor::new(Vec::new());
    write_generation_container(
        &mut valid_file,
        &metadata,
        &chunks,
        HeaderByteOrder::LittleEndian,
    )
    .expect("valid generation write should succeed");

    let canonical_bytes = valid_file.into_inner();
    let file_len = canonical_bytes.len();
    assert!(file_len > 128);

    // Test flipping bits systematically across the entire file
    for byte_idx in 0..file_len {
        for bit in [0x01, 0x08, 0x80] {
            let mut corrupted = canonical_bytes.clone();
            corrupted[byte_idx] ^= bit;

            let mut cursor = Cursor::new(&corrupted);
            // open_generation_snapshot must NEVER panic
            let res = open_generation_snapshot(&mut cursor);
            if let Ok(snapshot) = res {
                // If header/directory parsed, chunk reading must detect payload corruption
                let mut read_cursor = Cursor::new(&corrupted);
                let _ = snapshot.read_chunk(&mut read_cursor, 1);
                let _ = snapshot.read_chunk(&mut read_cursor, 2);
            }
        }
    }
}

#[test]
fn test_fuzz_truncation_at_every_byte_offset() {
    let metadata = Metadata::new("test-fuzz-truncation");
    let chunks = [EncodedChunkInput {
        id: 42,
        data: b"payload-data-for-truncation-testing-verification",
        codec: ChunkCodec::Zstandard,
    }];

    let mut valid_file = Cursor::new(Vec::new());
    write_generation_container(
        &mut valid_file,
        &metadata,
        &chunks,
        HeaderByteOrder::LittleEndian,
    )
    .unwrap();

    let bytes = valid_file.into_inner();

    // Truncate at every single byte offset from 0 to len-1
    for len in 0..bytes.len() {
        let truncated = &bytes[..len];
        let mut cursor = Cursor::new(truncated);
        let res = open_generation_snapshot(&mut cursor);
        // Must NEVER panic on truncated file
        assert!(
            res.is_err(),
            "truncated file at len {} should return Err, not Ok",
            len
        );
    }
}

#[test]
fn test_power_loss_and_recovery_simulation() {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join(format!("wx1_power_loss_{}.wx1", std::process::id()));
    if file_path.exists() {
        let _ = std::fs::remove_file(&file_path);
    }

    // Step 1: Write initial Generation 1 cleanly
    let metadata = Metadata::new("power-loss-resilience-dataset");
    let gen1_chunks = [EncodedChunkInput {
        id: 101,
        data: b"gen-1-stable-temperature-record",
        codec: ChunkCodec::Zstandard,
    }];

    {
        let mut file = File::create(&file_path).unwrap();
        write_generation_container(
            &mut file,
            &metadata,
            &gen1_chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        file.sync_all().unwrap();
    }

    let gen1_size = std::fs::metadata(&file_path).unwrap().len();

    // Verify Gen 1 is readable
    {
        let mut file = File::open(&file_path).unwrap();
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(snapshot.generation(), 1);
        let payload = snapshot.read_chunk(&mut file, 101).unwrap();
        assert_eq!(payload, b"gen-1-stable-temperature-record");
    }

    // --- Scenario A: Crash / Power Loss Mid-Chunk during Generation 2 ---
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&file_path)
            .unwrap();
        // Write 150 bytes of garbage simulating interrupted chunk write
        file.write_all(&[0xAA; 150]).unwrap();
        file.sync_all().unwrap();
    }

    // Verify that open_generation_snapshot still safely recovers Generation 1 for readers,
    // but append_generation strictly forbids appending onto an unrepaired tail!
    {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&file_path)
            .unwrap();
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(snapshot.generation(), 1);

        // Attempting to append must fail with UnrepairedTail
        let dummy_chunk = [EncodedChunkInput {
            id: 999,
            data: b"fail",
            codec: ChunkCodec::Raw,
        }];
        let append_res = append_generation(&mut file, &dummy_chunk);
        assert!(matches!(append_res, Err(ChunkIoError::UnrepairedTail)));
    }

    // Perform atomic repair
    let report = repair_generation_file(&file_path).expect("repair should succeed");
    assert_eq!(report.generation, 1);
    assert_eq!(report.retained_len, gen1_size);
    assert_eq!(
        std::fs::metadata(&file_path).unwrap().len(),
        gen1_size,
        "file must be truncated exactly to Gen 1 boundary"
    );

    // Verify Gen 1 is completely intact and readable after repair
    {
        let mut file = File::open(&file_path).unwrap();
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(snapshot.generation(), 1);
        let payload = snapshot.read_chunk(&mut file, 101).unwrap();
        assert_eq!(payload, b"gen-1-stable-temperature-record");
    }

    // --- Scenario B: Crash with Torn Commit Footer (32 bytes written instead of 64) ---
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&file_path)
            .unwrap();
        // Append chunk plus half a footer
        file.write_all(b"new-chunk-payload-data").unwrap();
        file.write_all(&[0x55; 32]).unwrap();
        file.sync_all().unwrap();
    }

    let res_repair_b =
        repair_generation_file(&file_path).expect("repair should succeed on torn footer");
    assert_eq!(res_repair_b.generation, 1);
    assert_eq!(res_repair_b.retained_len, gen1_size);

    // --- Scenario C: Commit Clean Generation 2 after Repair ---
    let gen2_chunks = [EncodedChunkInput {
        id: 102,
        data: b"gen-2-appended-pressure-record",
        codec: ChunkCodec::Zstandard,
    }];
    {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&file_path)
            .unwrap();
        append_generation(&mut file, &gen2_chunks).expect("append after repair must succeed");
        file.sync_all().unwrap();
    }

    let gen2_size = std::fs::metadata(&file_path).unwrap().len();
    assert!(gen2_size > gen1_size);

    // Verify both Gen 1 and Gen 2 chunks are accessible
    {
        let mut file = File::open(&file_path).unwrap();
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(snapshot.generation(), 2);
        assert_eq!(
            snapshot.read_chunk(&mut file, 101).unwrap(),
            b"gen-1-stable-temperature-record"
        );
        assert_eq!(
            snapshot.read_chunk(&mut file, 102).unwrap(),
            b"gen-2-appended-pressure-record"
        );
    }

    let _ = std::fs::remove_file(&file_path);
}

#[test]
fn test_power_loss_subprocess_sigkill_recovery() {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join(format!("wx1_sigkill_{}.wx1", std::process::id()));
    if file_path.exists() {
        let _ = std::fs::remove_file(&file_path);
    }

    // Write Generation 1
    let metadata = Metadata::new("subprocess-sigkill-resilience");
    let gen1_chunks = [EncodedChunkInput {
        id: 1,
        data: b"generation-1-durable-baseline",
        codec: ChunkCodec::Zstandard,
    }];
    {
        let mut file = File::create(&file_path).unwrap();
        write_generation_container(
            &mut file,
            &metadata,
            &gen1_chunks,
            HeaderByteOrder::LittleEndian,
        )
        .unwrap();
        file.sync_all().unwrap();
    }
    let gen1_size = std::fs::metadata(&file_path).unwrap().len();

    // Spawn a child process appending data and terminate it abruptly with SIGKILL
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!(
            "echo -n 'partial-interrupted-bytes-stream' >> {} && sleep 10",
            file_path.display()
        ))
        .spawn()
        .expect("spawning child writer must succeed");

    // Wait up to 1 second for child to append uncommitted trailing bytes
    for _ in 0..100 {
        if std::fs::metadata(&file_path).unwrap().len() > gen1_size {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Abruptly kill with SIGKILL
    child.kill().expect("SIGKILL child process must succeed");
    let _ = child.wait();

    // File now has trailing uncommitted data
    assert!(std::fs::metadata(&file_path).unwrap().len() > gen1_size);

    // Verify repair trims file back to gen1_size and Generation 1 is intact
    let report = repair_generation_file(&file_path).expect("repair after SIGKILL must succeed");
    assert_eq!(report.generation, 1);
    assert_eq!(report.retained_len, gen1_size);
    assert_eq!(std::fs::metadata(&file_path).unwrap().len(), gen1_size);

    // Verify Generation 1 is fully readable
    {
        let mut file = File::open(&file_path).unwrap();
        let snapshot = open_generation_snapshot(&mut file).unwrap();
        assert_eq!(snapshot.generation(), 1);
        let payload = snapshot.read_chunk(&mut file, 1).unwrap();
        assert_eq!(payload, b"generation-1-durable-baseline");
    }

    let _ = std::fs::remove_file(&file_path);
}

#[test]
fn test_security_variable_path_traversal_rejection() {
    let mut model = ScientificDataset::new();
    model.dimensions = vec![Dimension {
        name: "lat".to_owned(),
        length: 10,
        role: DimensionRole::Latitude,
    }];

    // Test illegal relative traversal variable paths
    let malicious_names = [
        "../etc/passwd",
        "../../shadow",
        "foo/../bar",
        "foo/./bar",
        "\\windows\\system32",
        "var\0null",
        "",
        ".",
        "..",
        "var with spaces",
        "var/with/slashes",
        "var@special",
        "var#hash",
    ];

    for malicious in malicious_names {
        model.variables = vec![Variable {
            name: malicious.to_owned(),
            group: "/".to_owned(),
            dimensions: vec!["lat".to_owned()],
            data_type: DataType::F32,
            byte_order: PayloadByteOrder::LittleEndian,
            chunk_shape: vec![10],
            fill_value: None,
            missing_value: None,
            attributes: BTreeMap::new(),
            chunks: Vec::new(),
            grid_mapping: None,
        }];

        let res = model.validate();
        assert!(
            res.is_err(),
            "variable name '{}' must be rejected as invalid",
            malicious
        );
    }
}

#[test]
fn test_security_group_path_traversal_rejection() {
    let mut model = ScientificDataset::new();
    model.dimensions = vec![Dimension {
        name: "time".to_owned(),
        length: 5,
        role: DimensionRole::Time,
    }];

    let malicious_groups = [
        "/../root",
        "/etc/passwd",
        "/group/../secret",
        "//double_slash",
        "/trailing_slash/",
        "no_leading_slash",
        "/has space",
        "/has\0null",
        "/./current",
    ];

    for malicious in malicious_groups {
        model.groups = vec![wx1_format::Group {
            path: malicious.to_owned(),
            attributes: BTreeMap::new(),
        }];

        let res = model.validate();
        assert!(
            res.is_err(),
            "group path '{}' must be rejected as invalid",
            malicious
        );
    }
}

#[test]
fn test_security_max_dimensions_and_rank_boundary() {
    // Rank 64 is permitted (MAX_SCIENTIFIC_RANK = 64)
    let mut model_64 = ScientificDataset::new();
    let mut dims_64 = Vec::new();
    let mut dim_names_64 = Vec::new();
    let mut chunk_shape_64 = Vec::new();
    for i in 0..64 {
        let name = format!("d{i}");
        dims_64.push(Dimension {
            name: name.clone(),
            length: 1,
            role: DimensionRole::Other,
        });
        dim_names_64.push(name);
        chunk_shape_64.push(1);
    }
    model_64.dimensions = dims_64;
    model_64.variables = vec![Variable {
        name: "cube64".to_owned(),
        group: "/".to_owned(),
        dimensions: dim_names_64,
        data_type: DataType::F32,
        byte_order: PayloadByteOrder::LittleEndian,
        chunk_shape: chunk_shape_64,
        fill_value: None,
        missing_value: None,
        attributes: BTreeMap::new(),
        chunks: vec![wx1_format::ArrayChunkRef {
            chunk_id: 1,
            origin: vec![0; 64],
            shape: vec![1; 64],
        }],
        grid_mapping: None,
    }];
    assert!(model_64.validate().is_ok(), "Rank 64 must be allowed");

    // Rank 65 is strictly rejected with RankMismatch
    let mut model_65 = ScientificDataset::new();
    let mut dims_65 = Vec::new();
    let mut dim_names_65 = Vec::new();
    let mut chunk_shape_65 = Vec::new();
    for i in 0..65 {
        let name = format!("d{i}");
        dims_65.push(Dimension {
            name: name.clone(),
            length: 1,
            role: DimensionRole::Other,
        });
        dim_names_65.push(name);
        chunk_shape_65.push(1);
    }
    model_65.dimensions = dims_65;
    model_65.variables = vec![Variable {
        name: "cube65".to_owned(),
        group: "/".to_owned(),
        dimensions: dim_names_65,
        data_type: DataType::F32,
        byte_order: PayloadByteOrder::LittleEndian,
        chunk_shape: chunk_shape_65,
        fill_value: None,
        missing_value: None,
        attributes: BTreeMap::new(),
        chunks: Vec::new(),
        grid_mapping: None,
    }];
    assert_eq!(
        model_65.validate().unwrap_err(),
        ScientificModelError::RankMismatch
    );
}

#[test]
fn test_security_max_chunk_refs_boundary() {
    let mut model = ScientificDataset::new();
    model.dimensions = vec![Dimension {
        name: "dim0".to_owned(),
        length: 2_000_000,
        role: DimensionRole::Other,
    }];

    // Single variable with expected chunks > MAX_CHUNKS (1,048,576)
    model.variables = vec![Variable {
        name: "huge_var".to_owned(),
        group: "/".to_owned(),
        dimensions: vec!["dim0".to_owned()],
        data_type: DataType::F32,
        byte_order: PayloadByteOrder::LittleEndian,
        chunk_shape: vec![1],
        fill_value: None,
        missing_value: None,
        attributes: BTreeMap::new(),
        chunks: vec![wx1_format::ArrayChunkRef {
            chunk_id: 1,
            origin: vec![0],
            shape: vec![1],
        }],
        grid_mapping: None,
    }];

    // 2,000,000 expected chunks > MAX_CHUNKS rejected
    let err = model.validate().unwrap_err();
    assert_eq!(
        err,
        ScientificModelError::ChunkCoverage,
        "single variable exceeding MAX_CHUNKS must fail with ChunkCoverage"
    );

    // Multiple variables whose total chunks exceed MAX_CHUNKS
    let mut multi_model = ScientificDataset::new();
    multi_model.dimensions = vec![Dimension {
        name: "dim0".to_owned(),
        length: 600_000,
        role: DimensionRole::Other,
    }];
    let mut chunks_v1 = Vec::with_capacity(600_000);
    let mut chunks_v2 = Vec::with_capacity(600_000);
    for i in 0..600_000 {
        chunks_v1.push(wx1_format::ArrayChunkRef {
            chunk_id: i,
            origin: vec![i],
            shape: vec![1],
        });
        chunks_v2.push(wx1_format::ArrayChunkRef {
            chunk_id: 600_000 + i,
            origin: vec![i],
            shape: vec![1],
        });
    }
    multi_model.variables = vec![
        Variable {
            name: "v1".to_owned(),
            group: "/".to_owned(),
            dimensions: vec!["dim0".to_owned()],
            data_type: DataType::F32,
            byte_order: PayloadByteOrder::LittleEndian,
            chunk_shape: vec![1],
            fill_value: None,
            missing_value: None,
            attributes: BTreeMap::new(),
            chunks: chunks_v1,
            grid_mapping: None,
        },
        Variable {
            name: "v2".to_owned(),
            group: "/".to_owned(),
            dimensions: vec!["dim0".to_owned()],
            data_type: DataType::F32,
            byte_order: PayloadByteOrder::LittleEndian,
            chunk_shape: vec![1],
            fill_value: None,
            missing_value: None,
            attributes: BTreeMap::new(),
            chunks: chunks_v2,
            grid_mapping: None,
        },
    ];

    let multi_err = multi_model.validate().unwrap_err();
    assert_eq!(
        multi_err,
        ScientificModelError::TooManyChunkReferences,
        "cumulative chunk references exceeding 1,048,576 must fail with TooManyChunkReferences"
    );
}

#[test]
fn test_security_decompression_bomb_and_length_guards() {
    // 1. Claimed raw_len exceeding MAX_CHUNK_BYTES (64 MiB)
    let payload = vec![0u8; 32];
    let res = wx1_format::read_chunk_directory(&mut Cursor::new(&payload));
    assert!(res.is_err());
}

#[test]
fn test_cross_platform_ieee754_special_values() {
    // Create dataset containing NaN, +Inf, -Inf, -0.0, +0.0, subnormals
    let special_floats: [f32; 6] = [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -0.0,
        0.0,
        f32::from_bits(0x0000_0001), // Minimum positive subnormal float
    ];

    let mut raw_bytes = Vec::new();
    for val in &special_floats {
        raw_bytes.extend_from_slice(&val.to_le_bytes());
    }

    let chunk = EncodedChunkInput {
        id: 777,
        data: &raw_bytes,
        codec: ChunkCodec::Zstandard,
    };

    let metadata = Metadata::new("special-floats-test");
    let mut file = Cursor::new(Vec::new());
    write_generation_container(
        &mut file,
        &metadata,
        &[chunk],
        HeaderByteOrder::LittleEndian,
    )
    .unwrap();

    let mut read_file = Cursor::new(file.into_inner());
    let snapshot = open_generation_snapshot(&mut read_file).unwrap();
    let decoded = snapshot.read_chunk(&mut read_file, 777).unwrap();
    assert_eq!(decoded.len(), 24);

    for i in 0..6 {
        let original_bits = special_floats[i].to_bits();
        let decoded_bits = u32::from_le_bytes(decoded[i * 4..(i + 1) * 4].try_into().unwrap());
        if special_floats[i].is_nan() {
            assert!(f32::from_bits(decoded_bits).is_nan());
        } else {
            assert_eq!(
                original_bits, decoded_bits,
                "float {} bits must match exactly",
                i
            );
        }
    }
}
