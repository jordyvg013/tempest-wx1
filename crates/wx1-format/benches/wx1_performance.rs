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
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::{self, Command};
use std::time::{Duration, Instant};

use wx1_format::{
    open_scientific_snapshot, open_scientific_snapshot_lazy, write_generation_container,
    ArrayChunkRef, ArraySelection, ChunkCodec, DataType, Dimension, DimensionRole,
    EncodedChunkInput, HeaderByteOrder, Metadata, PayloadByteOrder, ScientificDataset, Variable,
};

const ARRAY_SIDE: u64 = 2_048;
const CODEC_TILE: u64 = 256;
const PLANNER_TILE: u64 = 32;
const WRITE_REPETITIONS: usize = 3;
const READ_REPETITIONS: usize = 5;
const PLAN_REPETITIONS: usize = 5;
const PLANS_PER_REPETITION: usize = 200;
const WINDOW_SIDE: u64 = 256;

struct BenchmarkFixture {
    metadata: Metadata,
    payloads: Vec<Vec<u8>>,
    raw_bytes: u64,
}

impl BenchmarkFixture {
    fn new(side: u64, tile: u64, dataset_id: &str) -> Result<Self, Box<dyn Error>> {
        if side == 0 || tile == 0 || side % tile != 0 {
            return Err("benchmark side and tile must be nonzero and evenly divisible".into());
        }
        let chunks_per_axis = side / tile;
        let chunk_count = chunks_per_axis
            .checked_mul(chunks_per_axis)
            .ok_or("chunk count overflow")?;
        let chunk_capacity = usize::try_from(chunk_count).map_err(|_| "too many chunks")?;
        let tile_elements = tile
            .checked_mul(tile)
            .ok_or("tile element count overflow")?;
        let tile_bytes = tile_elements
            .checked_mul(4)
            .ok_or("tile byte count overflow")?;
        let tile_usize = usize::try_from(tile).map_err(|_| "tile does not fit usize")?;
        let tile_bytes_usize =
            usize::try_from(tile_bytes).map_err(|_| "tile byte count does not fit usize")?;
        let mut chunk_refs = Vec::with_capacity(chunk_capacity);
        let mut payloads = Vec::with_capacity(chunk_capacity);
        let mut chunk_id = 1_u64;

        for origin_y in (0..side).step_by(tile_usize) {
            for origin_x in (0..side).step_by(tile_usize) {
                let mut payload = Vec::with_capacity(tile_bytes_usize);
                for y in origin_y..origin_y + tile {
                    for x in origin_x..origin_x + tile {
                        // Deterministic, smooth, synthetic field. This is not observational data.
                        let value = (y % 256) as f32 * 0.25
                            + (x % 256) as f32 * 0.03125
                            + ((y ^ x) % 8) as f32 * 0.001;
                        payload.extend_from_slice(&value.to_le_bytes());
                    }
                }
                chunk_refs.push(ArrayChunkRef {
                    chunk_id,
                    origin: vec![origin_y, origin_x],
                    shape: vec![tile, tile],
                });
                payloads.push(payload);
                chunk_id = chunk_id.checked_add(1).ok_or("chunk id overflow")?;
            }
        }

        let model = ScientificDataset {
            dimensions: vec![
                Dimension {
                    name: "y".to_owned(),
                    length: side,
                    role: DimensionRole::Latitude,
                },
                Dimension {
                    name: "x".to_owned(),
                    length: side,
                    role: DimensionRole::Longitude,
                },
            ],
            variables: vec![Variable {
                name: "synthetic_field".to_owned(),
                group: "/".to_owned(),
                data_type: DataType::F32,
                byte_order: PayloadByteOrder::LittleEndian,
                dimensions: vec!["y".to_owned(), "x".to_owned()],
                chunk_shape: vec![tile, tile],
                fill_value: None,
                missing_value: None,
                attributes: BTreeMap::new(),
                chunks: chunk_refs,
                grid_mapping: None,
            }],
            ..ScientificDataset::default()
        };

        let elements = side
            .checked_mul(side)
            .ok_or("array element count overflow")?;
        let raw_bytes = elements.checked_mul(4).ok_or("array byte count overflow")?;
        Ok(Self {
            metadata: Metadata::new(dataset_id).with_scientific_model(model),
            payloads,
            raw_bytes,
        })
    }

    fn encoded_chunks(&self, codec: ChunkCodec) -> Vec<EncodedChunkInput<'_>> {
        self.payloads
            .iter()
            .enumerate()
            .map(|(index, data)| EncodedChunkInput {
                id: index as u64 + 1,
                data,
                codec,
            })
            .collect()
    }
}

struct RemoveFile(PathBuf);

impl Drop for RemoveFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn benchmark_path(label: &str) -> PathBuf {
    let directory = env::var_os("WX1_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    directory.join(format!("wx1-v043-bench-{}-{label}.wx1", process::id()))
}

fn median_duration(samples: &mut [Duration]) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn mib_per_second(bytes: u64, duration: Duration) -> f64 {
    let seconds = duration.as_secs_f64();
    if seconds == 0.0 {
        return f64::INFINITY;
    }
    bytes as f64 / (1024.0 * 1024.0) / seconds
}

fn process_memory_kib(field: &str) -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name != field {
            return None;
        }
        value.split_whitespace().next()?.parse().ok()
    })
}

fn memory_mib(value_kib: Option<u64>) -> String {
    value_kib
        .map(|value| format!("{:.1}", value as f64 / 1024.0))
        .unwrap_or_else(|| "unavailable".to_owned())
}

fn run_codec_case(codec: ChunkCodec, codec_name: &str) -> Result<(), Box<dyn Error>> {
    let fixture = BenchmarkFixture::new(ARRAY_SIDE, CODEC_TILE, "wx1-v043-codec")?;
    let chunks = fixture.encoded_chunks(codec);
    let path = benchmark_path(codec_name);
    let _remove_file = RemoveFile(path.clone());

    let mut write_samples = Vec::with_capacity(WRITE_REPETITIONS);
    for _ in 0..WRITE_REPETITIONS {
        let mut file = File::create(&path)?;
        let started = Instant::now();
        write_generation_container(
            &mut file,
            &fixture.metadata,
            &chunks,
            HeaderByteOrder::LittleEndian,
        )?;
        file.flush()?;
        write_samples.push(started.elapsed());
    }
    let write_median = median_duration(&mut write_samples);

    let file_bytes = fs::metadata(&path)?.len();
    let mut reader = File::open(&path)?;
    let open_started = Instant::now();
    let snapshot = open_scientific_snapshot(&mut reader)?;
    let open_elapsed = open_started.elapsed();

    let full_selection = ArraySelection::new(vec![0, 0], vec![ARRAY_SIDE, ARRAY_SIDE]);
    let mut full_read_samples = Vec::with_capacity(READ_REPETITIONS);
    for _ in 0..READ_REPETITIONS {
        let started = Instant::now();
        let array = snapshot.read_array_slice(&mut reader, "/synthetic_field", &full_selection)?;
        full_read_samples.push(started.elapsed());
        if array.data.len() as u64 != fixture.raw_bytes {
            return Err("full read returned an unexpected output length".into());
        }
        std::hint::black_box(array.data);
    }
    let full_read_median = median_duration(&mut full_read_samples);

    let mut crc_only_samples = Vec::with_capacity(READ_REPETITIONS);
    for _ in 0..READ_REPETITIONS {
        let started = Instant::now();
        let array = snapshot.read_array_slice_mode(
            &mut reader,
            "/synthetic_field",
            &full_selection,
            false,
        )?;
        crc_only_samples.push(started.elapsed());
        if array.data.len() as u64 != fixture.raw_bytes {
            return Err("crc-only full read returned an unexpected output length".into());
        }
        std::hint::black_box(array.data);
    }
    let crc_only_median = median_duration(&mut crc_only_samples);

    let window_start = ARRAY_SIDE / 4;
    let window_selection = ArraySelection::new(
        vec![window_start, window_start],
        vec![WINDOW_SIDE, WINDOW_SIDE],
    );
    let mut window_read_samples = Vec::with_capacity(READ_REPETITIONS);
    for _ in 0..READ_REPETITIONS {
        let started = Instant::now();
        let array =
            snapshot.read_array_slice(&mut reader, "/synthetic_field", &window_selection)?;
        window_read_samples.push(started.elapsed());
        let expected_bytes = WINDOW_SIDE * WINDOW_SIDE * 4;
        if array.data.len() as u64 != expected_bytes {
            return Err("window read returned an unexpected output length".into());
        }
        std::hint::black_box(array.data);
    }
    let window_read_median = median_duration(&mut window_read_samples);

    let file_ratio = file_bytes as f64 / fixture.raw_bytes as f64;
    println!(
        "case=codec codec={codec_name} dimensions={ARRAY_SIDE}x{ARRAY_SIDE} chunks={} raw_mib={:.2} file_mib={:.2} file_ratio={:.4} write_median_ms={:.3} write_mib_s={:.2} open_validate_ms={:.3} full_read_median_ms={:.3} full_read_mib_s={:.2} full_read_crc_only_ms={:.3} full_read_crc_only_mib_s={:.2} window_read_median_ms={:.3} window_read_mib_s={:.2} peak_rss_mib={}",
        fixture.payloads.len(),
        fixture.raw_bytes as f64 / (1024.0 * 1024.0),
        file_bytes as f64 / (1024.0 * 1024.0),
        file_ratio,
        write_median.as_secs_f64() * 1000.0,
        mib_per_second(fixture.raw_bytes, write_median),
        open_elapsed.as_secs_f64() * 1000.0,
        full_read_median.as_secs_f64() * 1000.0,
        mib_per_second(fixture.raw_bytes, full_read_median),
        crc_only_median.as_secs_f64() * 1000.0,
        mib_per_second(fixture.raw_bytes, crc_only_median),
        window_read_median.as_secs_f64() * 1000.0,
        mib_per_second(WINDOW_SIDE * WINDOW_SIDE * 4, window_read_median),
        memory_mib(process_memory_kib("VmHWM")),
    );
    Ok(())
}

fn run_planner_case() -> Result<(), Box<dyn Error>> {
    let fixture = BenchmarkFixture::new(ARRAY_SIDE, PLANNER_TILE, "wx1-v043-planner")?;
    let chunks = fixture.encoded_chunks(ChunkCodec::Raw);
    let path = benchmark_path("planner");
    let _remove_file = RemoveFile(path.clone());

    let mut file = File::create(&path)?;
    write_generation_container(
        &mut file,
        &fixture.metadata,
        &chunks,
        HeaderByteOrder::LittleEndian,
    )?;
    file.flush()?;
    drop(file);

    let mut reader = File::open(&path)?;
    let open_started = Instant::now();
    let snapshot = open_scientific_snapshot(&mut reader)?;
    let open_elapsed = open_started.elapsed();

    let selection_start = ARRAY_SIDE / 2 - WINDOW_SIDE / 2 + 9;
    let selection = ArraySelection::new(
        vec![selection_start, selection_start],
        vec![WINDOW_SIDE, WINDOW_SIDE],
    );
    let mut planning_samples = Vec::with_capacity(PLAN_REPETITIONS);
    let mut selected_chunks = 0usize;
    for _ in 0..PLAN_REPETITIONS {
        let started = Instant::now();
        for _ in 0..PLANS_PER_REPETITION {
            let plan = snapshot.plan_array_slice("/synthetic_field", &selection)?;
            selected_chunks = plan.chunk_ids.len();
            std::hint::black_box(plan);
        }
        planning_samples.push(started.elapsed());
    }
    if selected_chunks != 81 {
        return Err(format!("planner selected {selected_chunks} chunks; expected 81").into());
    }
    let planning_median = median_duration(&mut planning_samples);
    let plan_ns = planning_median.as_nanos() / PLANS_PER_REPETITION as u128;

    let mut read_samples = Vec::with_capacity(READ_REPETITIONS);
    let expected_bytes = WINDOW_SIDE * WINDOW_SIDE * 4;
    for _ in 0..READ_REPETITIONS {
        let started = Instant::now();
        let array = snapshot.read_array_slice(&mut reader, "/synthetic_field", &selection)?;
        read_samples.push(started.elapsed());
        if array.data.len() as u64 != expected_bytes {
            return Err("planner window read returned an unexpected output length".into());
        }
        std::hint::black_box(array.data);
    }
    let read_median = median_duration(&mut read_samples);

    println!(
        "case=planner dimensions={ARRAY_SIDE}x{ARRAY_SIDE} chunks={} chunk_shape={PLANNER_TILE}x{PLANNER_TILE} selection={WINDOW_SIDE}x{WINDOW_SIDE} selected_chunks={selected_chunks} open_validate_ms={:.3} plan_median_ns={plan_ns} plans_per_repetition={PLANS_PER_REPETITION} selection_read_median_ms={:.3} selection_read_mib_s={:.2} peak_rss_mib={}",
        fixture.payloads.len(),
        open_elapsed.as_secs_f64() * 1000.0,
        read_median.as_secs_f64() * 1000.0,
        mib_per_second(expected_bytes, read_median),
        memory_mib(process_memory_kib("VmHWM")),
    );
    Ok(())
}

fn run_planner_scale_case(chunk_target: u64) -> Result<(), Box<dyn Error>> {
    let chunks_per_axis = match chunk_target {
        4096 => 64u64,
        65536 => 256u64,
        1048576 => 1024u64,
        _ => return Err(format!("unsupported planner scale target: {chunk_target}").into()),
    };
    let tile = PLANNER_TILE;
    let side = chunks_per_axis * tile;
    let mut chunk_refs = Vec::with_capacity(chunk_target as usize);
    let mut chunk_id = 1u64;
    for origin_y in (0..side).step_by(tile as usize) {
        for origin_x in (0..side).step_by(tile as usize) {
            chunk_refs.push(ArrayChunkRef {
                chunk_id,
                origin: vec![origin_y, origin_x],
                shape: vec![tile, tile],
            });
            chunk_id += 1;
        }
    }
    let dataset = ScientificDataset {
        dimensions: vec![
            Dimension {
                name: "y".to_owned(),
                length: side,
                role: DimensionRole::Latitude,
            },
            Dimension {
                name: "x".to_owned(),
                length: side,
                role: DimensionRole::Longitude,
            },
        ],
        variables: vec![Variable {
            name: "synthetic_field".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::F32,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["y".to_owned(), "x".to_owned()],
            chunk_shape: vec![tile, tile],
            fill_value: None,
            missing_value: None,
            attributes: BTreeMap::new(),
            chunks: chunk_refs,
            grid_mapping: None,
        }],
        ..ScientificDataset::default()
    };

    // Formally validate in-memory dataset under ADR 0034 limits
    let val_start = Instant::now();
    dataset.validate()?;
    let val_elapsed = val_start.elapsed();

    // Verify CBOR serialization within MAX_METADATA_BYTES (ADR 0034)
    let ser_start = Instant::now();
    let encoded = dataset.to_cbor()?;
    let ser_elapsed = ser_start.elapsed();

    // Also measure actual on-disk container open_lazy vs open_eager when chunk_target <= 65536 or 1048576
    let path = benchmark_path(&format!("scale-{chunk_target}"));
    let _remove_file = RemoveFile(path.clone());
    let single_tile_bytes = [0u8; 4]; // 1x1 F32 payload for fast on-disk 1M-chunk directory/B+tree test
    let mut compact_refs = Vec::with_capacity(chunk_target as usize);
    let mut compact_inputs = Vec::with_capacity(chunk_target as usize);
    for idx in 0..chunk_target {
        compact_refs.push(ArrayChunkRef {
            chunk_id: idx + 1,
            origin: vec![idx],
            shape: vec![1],
        });
        compact_inputs.push(EncodedChunkInput {
            id: idx + 1,
            data: &single_tile_bytes,
            codec: ChunkCodec::Raw,
        });
    }
    let compact_model = ScientificDataset {
        dimensions: vec![Dimension {
            name: "pt".to_owned(),
            length: chunk_target,
            role: DimensionRole::Other,
        }],
        variables: vec![Variable {
            name: "pts".to_owned(),
            group: "/".to_owned(),
            data_type: DataType::F32,
            byte_order: PayloadByteOrder::LittleEndian,
            dimensions: vec!["pt".to_owned()],
            chunk_shape: vec![1],
            fill_value: None,
            missing_value: None,
            attributes: BTreeMap::new(),
            chunks: compact_refs,
            grid_mapping: None,
        }],
        ..ScientificDataset::default()
    };
    let compact_meta = Metadata::new("scale-disk-r1").with_scientific_model(compact_model);
    {
        let mut f = File::create(&path)?;
        write_generation_container(
            &mut f,
            &compact_meta,
            &compact_inputs,
            HeaderByteOrder::LittleEndian,
        )?;
        f.flush()?;
    }
    let mut r_lazy = File::open(&path)?;
    let r1_lazy_start = Instant::now();
    let _snap_lazy = open_scientific_snapshot_lazy(&mut r_lazy)?;
    let r1_lazy_ms = r1_lazy_start.elapsed().as_secs_f64() * 1000.0;

    let mut r_eager = File::open(&path)?;
    let r1_eager_start = Instant::now();
    let _snap_eager = open_scientific_snapshot(&mut r_eager)?;
    let r1_eager_ms = r1_eager_start.elapsed().as_secs_f64() * 1000.0;

    let selection_start = side / 2 - WINDOW_SIDE / 2 + 9;
    let selection = ArraySelection::new(
        vec![selection_start, selection_start],
        vec![WINDOW_SIDE, WINDOW_SIDE],
    );

    let plans_per_rep = if chunk_target == 1048576 {
        50
    } else {
        PLANS_PER_REPETITION
    };
    let mut planning_samples = Vec::with_capacity(PLAN_REPETITIONS);
    let mut selected_chunks = 0usize;
    for _ in 0..PLAN_REPETITIONS {
        let started = Instant::now();
        for _ in 0..plans_per_rep {
            let plan = dataset.plan_array_slice("/synthetic_field", &selection)?;
            selected_chunks = plan.chunk_ids.len();
            std::hint::black_box(plan);
        }
        planning_samples.push(started.elapsed());
    }
    let planning_median = median_duration(&mut planning_samples);
    let plan_ns = planning_median.as_nanos() / plans_per_rep as u128;
    let plan_us = plan_ns as f64 / 1000.0;

    println!(
        "case=planner-scale chunks={chunk_target} validate_ms={:.3} serialize_ms={:.3} r1_open_lazy_ms={r1_lazy_ms:.3} r1_open_eager_ms={r1_eager_ms:.3} encoded_bytes={} selected_chunks={selected_chunks} plan_median_ns={plan_ns} plan_median_us={plan_us:.3} peak_rss_mib={}",
        val_elapsed.as_secs_f64() * 1000.0,
        ser_elapsed.as_secs_f64() * 1000.0,
        encoded.len(),
        memory_mib(process_memory_kib("VmHWM")),
    );
    Ok(())
}

fn run_case(name: &str) -> Result<(), Box<dyn Error>> {
    match name {
        "codec-raw" => run_codec_case(ChunkCodec::Raw, "raw"),
        "codec-zstd" => run_codec_case(ChunkCodec::Zstandard, "zstandard"),
        "codec-lz4" => run_codec_case(ChunkCodec::Lz4, "lz4"),
        "planner" => run_planner_case(),
        "planner-scale-4096" => run_planner_scale_case(4096),
        "planner-scale-65536" => run_planner_scale_case(65536),
        "planner-scale-1048576" => run_planner_scale_case(1048576),
        _ => Err(format!("unknown benchmark case: {name}").into()),
    }
}

fn run_suite() -> Result<(), Box<dyn Error>> {
    let executable = env::current_exe()?;
    println!("WX-1 v0.4.3 performance benchmark");
    println!("Synthetic deterministic F32 data; optimized cargo bench profile.");
    println!("Write timings include encoding and normal file writes, but not fsync durability.");
    println!("Peak RSS is isolated per case when Linux /proc is available.");
    if let Some(directory) = env::var_os("WX1_BENCH_DIR") {
        println!(
            "Benchmark directory: {}",
            PathBuf::from(directory).display()
        );
    } else {
        println!(
            "Benchmark directory: {} (default)",
            env::temp_dir().display()
        );
    }

    for case in [
        "codec-raw",
        "codec-zstd",
        "codec-lz4",
        "planner",
        "planner-scale-4096",
        "planner-scale-65536",
        "planner-scale-1048576",
    ] {
        let output = Command::new(&executable).arg("--case").arg(case).output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("benchmark case {case} failed: {stderr}").into());
        }
        let stdout = String::from_utf8(output.stdout)?;
        print!("{stdout}");
    }
    println!("Benchmark suite completed. No performance threshold or speedup is implied.");
    Ok(())
}

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let result = if args.first().is_some_and(|argument| argument == "--case") {
        args.get(1)
            .ok_or_else(|| "missing benchmark case".into())
            .and_then(|name| run_case(name))
    } else {
        run_suite()
    };

    if let Err(error) = result {
        eprintln!("WX-1 benchmark error: {error}");
        process::exit(1);
    }
}
