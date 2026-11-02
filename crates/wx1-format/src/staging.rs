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

//! Parallel tile/rank chunk staging, deterministic merge/commit, and bounded asynchronous
//! scientific writer with backpressure telemetry (ADR 0025).

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use crate::{
    append_staged_scientific_generation_with_indexes_hooks, crc32c, encode_chunk,
    write_staged_generation_container_with_indexes_hooks, ChunkCodec, ChunkDescriptor,
    ChunkIoError, EncodedChunkInput, HeaderByteOrder, Metadata, PersistentIndexEntry, MAX_CHUNKS,
    MAX_CHUNK_BYTES, MAX_STORED_CHUNK_BYTES,
};

/// Maximum number of worker threads permitted in a single parallel staging pool.
pub const MAX_STAGING_WORKERS: usize = 256;

/// Durability mode for synchronous and asynchronous generation commits (`W2`, `W4`, `W6`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteDurability {
    /// Flushes userspace buffers in commit order without issuing `fsync` / `sync_data`.
    Buffered,
    /// Executes `sync_data` before writing the generation footer and `sync_all` after the footer
    /// (plus parent directory sync on atomic creation).
    Durable,
}

/// A self-contained, pre-encoded and pre-hashed chunk produced by a parallel worker.
///
/// Producing `StagedChunk` instances requires zero shared mutable state: each worker
/// compresses its own payload bytes and computes `CRC32C` and `BLAKE3` independently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedChunk {
    /// Unique chunk identifier within the container.
    pub id: u64,
    /// Uncompressed payload length in bytes ($\le 64\text{ MiB}$).
    pub raw_len: u64,
    /// Compressed payload bytes ($\le \text{MAX\_STORED\_CHUNK\_BYTES}$).
    pub stored_bytes: Vec<u8>,
    /// Precomputed Castagnoli CRC32C checksum over `stored_bytes`.
    pub crc32c: u32,
    /// Precomputed BLAKE3 digest over `stored_bytes`.
    pub blake3: [u8; 32],
    /// Lossless chunk codec used to encode `stored_bytes`.
    pub codec: ChunkCodec,
}

impl StagedChunk {
    /// Converts this staged chunk into a `ChunkDescriptor` at the given physical file offset.
    pub(crate) fn descriptor_at(&self, offset: u64) -> ChunkDescriptor {
        ChunkDescriptor {
            id: self.id,
            offset,
            stored_len: self.stored_bytes.len() as u64,
            raw_len: self.raw_len,
            crc32c: self.crc32c,
            codec: self.codec,
            blake3: Some(self.blake3),
        }
    }
}

/// Encodes, checksums (`CRC32C`), and hashes (`BLAKE3`) a single chunk in isolation.
pub fn stage_chunk(input: EncodedChunkInput<'_>) -> Result<StagedChunk, ChunkIoError> {
    let raw_len = input.data.len() as u64;
    if raw_len > MAX_CHUNK_BYTES {
        return Err(ChunkIoError::ChunkTooLarge);
    }
    let stored_bytes = encode_chunk(input.codec, input.data)?;
    if stored_bytes.len() as u64 > MAX_STORED_CHUNK_BYTES {
        return Err(ChunkIoError::ChunkTooLarge);
    }
    let checksum = crc32c(&stored_bytes);
    let digest = *blake3::hash(&stored_bytes).as_bytes();
    Ok(StagedChunk {
        id: input.id,
        raw_len,
        stored_bytes,
        crc32c: checksum,
        blake3: digest,
        codec: input.codec,
    })
}

/// Stages a slice of chunks across `workers` parallel CPU threads and returns them
/// deterministically sorted by ascending `chunk.id`.
///
/// Regardless of `workers` ($1, 2, 4, 8, 12, \dots$) or the input slice ordering,
/// the returned `Vec<StagedChunk>` is bit-for-bit identical.
pub fn stage_chunks_parallel(
    inputs: &[EncodedChunkInput<'_>],
    workers: usize,
) -> Result<Vec<StagedChunk>, ChunkIoError> {
    if inputs.len() as u64 > MAX_CHUNKS {
        return Err(ChunkIoError::TooManyChunks);
    }
    let mut seen_ids = std::collections::BTreeSet::new();
    for chunk in inputs {
        if !seen_ids.insert(chunk.id) {
            return Err(ChunkIoError::DuplicateChunkId);
        }
        if chunk.data.len() as u64 > MAX_CHUNK_BYTES {
            return Err(ChunkIoError::ChunkTooLarge);
        }
    }
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let effective_workers = workers.clamp(1, MAX_STAGING_WORKERS).min(inputs.len());
    let mut staged = Vec::new();
    staged
        .try_reserve_exact(inputs.len())
        .map_err(|_| ChunkIoError::TooManyChunks)?;

    if effective_workers == 1 {
        for &input in inputs {
            staged.push(stage_chunk(input)?);
        }
    } else {
        let batch_size = inputs.len().div_ceil(effective_workers);
        let worker_results: Vec<Result<Vec<StagedChunk>, ChunkIoError>> = thread::scope(|scope| {
            let mut handles = Vec::with_capacity(effective_workers);
            for slice in inputs.chunks(batch_size) {
                handles.push(scope.spawn(move || {
                    let mut local = Vec::with_capacity(slice.len());
                    for &item in slice {
                        local.push(stage_chunk(item)?);
                    }
                    Ok(local)
                }));
            }
            handles
                .into_iter()
                .map(|handle| {
                    handle.join().unwrap_or_else(|_| {
                        Err(ChunkIoError::Compression(
                            "parallel staging worker thread panicked".to_owned(),
                        ))
                    })
                })
                .collect()
        });
        for batch in worker_results {
            staged.extend(batch?);
        }
    }

    // Deterministic canonical merge order: sort by ascending chunk_id.
    staged.sort_by_key(|chunk| chunk.id);
    Ok(staged)
}

/// Writes a new generation container from pre-staged chunks into a seekable writer.
pub fn write_staged_generation_container<W: Write + Seek>(
    writer: &mut W,
    metadata: &Metadata,
    staged_chunks: &[StagedChunk],
    index_entries: &[PersistentIndexEntry],
    order: HeaderByteOrder,
) -> Result<(), ChunkIoError> {
    write_staged_generation_container_with_indexes_hooks(
        writer,
        metadata,
        staged_chunks,
        index_entries,
        order,
        |_| Ok(()),
        |_| Ok(()),
    )
}

/// Creates and publishes a new generation container from pre-staged chunks via a sibling
/// temporary file, honoring the requested `WriteDurability` mode.
pub fn write_staged_generation_container_atomic(
    path: impl AsRef<Path>,
    metadata: &Metadata,
    staged_chunks: &[StagedChunk],
    index_entries: &[PersistentIndexEntry],
    order: HeaderByteOrder,
    durability: WriteDurability,
) -> Result<(), ChunkIoError> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or(ChunkIoError::DirectoryOffsetInvalid)?;
    let mut temp_name = name.to_os_string();
    temp_name.push(format!(".wx1-staged-tmp-{}", std::process::id()));
    let temp_path = parent.join(temp_name);
    let mut temp_created = false;
    let result = (|| {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        temp_created = true;
        match durability {
            WriteDurability::Buffered => {
                write_staged_generation_container_with_indexes_hooks(
                    &mut file,
                    metadata,
                    staged_chunks,
                    index_entries,
                    order,
                    |_| Ok(()),
                    |_| Ok(()),
                )?;
                file.flush()?;
            }
            WriteDurability::Durable => {
                write_staged_generation_container_with_indexes_hooks(
                    &mut file,
                    metadata,
                    staged_chunks,
                    index_entries,
                    order,
                    |f| {
                        f.flush()?;
                        f.sync_data()
                    },
                    |f| {
                        f.flush()?;
                        f.sync_all()
                    },
                )?;
                file.flush()?;
                file.sync_all()?;
            }
        }
        drop(file);
        std::fs::rename(&temp_path, path)?;
        if durability == WriteDurability::Durable {
            #[cfg(unix)]
            File::open(parent)?.sync_all()?;
        }
        Ok::<(), ChunkIoError>(())
    })();
    if result.is_err() && temp_created {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

/// Appends a pre-staged generation to an open scientific container stream.
pub fn append_staged_scientific_generation<W: Read + Write + Seek>(
    writer: &mut W,
    staged_chunks: &[StagedChunk],
    index_entries: &[PersistentIndexEntry],
    updated_metadata: &Metadata,
) -> Result<u64, ChunkIoError> {
    append_staged_scientific_generation_with_indexes_hooks(
        writer,
        staged_chunks,
        index_entries,
        updated_metadata,
        |_| Ok(()),
        |_| Ok(()),
    )
}

/// Appends a pre-staged generation to a scientific container file on disk, honoring `WriteDurability`.
pub fn append_staged_scientific_generation_file(
    path: impl AsRef<Path>,
    staged_chunks: &[StagedChunk],
    index_entries: &[PersistentIndexEntry],
    updated_metadata: &Metadata,
    durability: WriteDurability,
) -> Result<u64, ChunkIoError> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    match durability {
        WriteDurability::Buffered => append_staged_scientific_generation_with_indexes_hooks(
            &mut file,
            staged_chunks,
            index_entries,
            updated_metadata,
            |f| f.flush(),
            |f| f.flush(),
        ),
        WriteDurability::Durable => append_staged_scientific_generation_with_indexes_hooks(
            &mut file,
            staged_chunks,
            index_entries,
            updated_metadata,
            |f| {
                f.flush()?;
                f.sync_data()
            },
            |f| {
                f.flush()?;
                f.sync_all()
            },
        ),
    }
}

/// Configuration for `AsyncScientificWriter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsyncWriterOptions {
    /// Maximum number of staged generations allowed in flight in the background queue (`1..=64`).
    pub max_pending_generations: usize,
    /// Maximum total compressed staged bytes allowed in flight before applying backpressure.
    pub max_queued_bytes: u64,
    /// Number of parallel worker threads used to stage chunks upon submission.
    pub staging_workers: usize,
    /// Commit durability policy (`Buffered` vs `Durable`).
    pub durability: WriteDurability,
    /// Container header byte order for newly created files.
    pub byte_order: HeaderByteOrder,
}

impl Default for AsyncWriterOptions {
    fn default() -> Self {
        Self {
            max_pending_generations: 2,
            max_queued_bytes: 512 * 1024 * 1024,
            staging_workers: 4,
            durability: WriteDurability::Durable,
            byte_order: HeaderByteOrder::LittleEndian,
        }
    }
}

/// Telemetry recorded by `AsyncScientificWriter` for NWP checkpoint and I/O stall analysis (`W5`, `W6`, `D4`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AsyncWriterTelemetry {
    /// Number of generations submitted by the caller.
    pub submitted_generations: u64,
    /// Number of generations durably or buffered-committed by the background writer thread.
    pub committed_generations: u64,
    /// Total uncompressed bytes across all submitted generations.
    pub total_raw_bytes: u64,
    /// Total compressed bytes across all submitted generations.
    pub total_staged_bytes: u64,
    /// Cumulative time (in nanoseconds) the caller was blocked by queue backpressure.
    pub total_enqueue_stall_ns: u64,
    /// Maximum single-step time (in nanoseconds) the caller was blocked by queue backpressure.
    pub max_enqueue_stall_ns: u64,
    /// Number of submissions that encountered a full queue or byte budget and experienced backpressure.
    pub backpressure_events: u64,
    /// Cumulative wall time (in nanoseconds) spent in parallel chunk staging.
    pub total_stage_duration_ns: u64,
    /// Cumulative wall time (in nanoseconds) spent by the background thread writing and syncing generations.
    pub total_commit_duration_ns: u64,
}

struct QueueState {
    in_flight_generations: usize,
    in_flight_bytes: u64,
    telemetry: AsyncWriterTelemetry,
    background_error: Option<String>,
}

enum AsyncTask {
    StageAndCommit {
        metadata: Box<Metadata>,
        raw_chunks: Vec<(u64, ChunkCodec, Vec<u8>)>,
        index_entries: Vec<PersistentIndexEntry>,
        queued_bytes: u64,
    },
    CommitGeneration {
        metadata: Box<Metadata>,
        staged_chunks: Vec<StagedChunk>,
        index_entries: Vec<PersistentIndexEntry>,
        staged_bytes: u64,
    },
    Flush,
    Shutdown,
}

/// Bounded asynchronous scientific dataset writer with backpressure and stall telemetry.
///
/// Designed for NWP timestep loops (`W5`, `W6`, `D4`): chunk compression, byte-shuffling,
/// CRC32C, and BLAKE3 execute in parallel worker threads, while sequential file I/O and
/// `fsync` run on a dedicated background thread behind a bounded queue.
pub struct AsyncScientificWriter {
    path: PathBuf,
    options: AsyncWriterOptions,
    sender: Option<SyncSender<AsyncTask>>,
    state: Arc<(Mutex<QueueState>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

impl AsyncScientificWriter {
    /// Opens a bounded asynchronous writer targeting `path`.
    ///
    /// If `path` does not exist or is empty when the first generation is committed,
    /// generation 1 is created atomically. Subsequent submissions append new generations (`2..N`).
    pub fn open(path: impl AsRef<Path>, options: AsyncWriterOptions) -> Result<Self, ChunkIoError> {
        if options.max_pending_generations == 0 || options.max_pending_generations > 64 {
            return Err(ChunkIoError::InvalidStreamBufferSize);
        }
        if options.max_queued_bytes == 0 {
            return Err(ChunkIoError::InvalidStreamBufferSize);
        }
        let path = path.as_ref().to_path_buf();
        let (sender, receiver) = mpsc::sync_channel::<AsyncTask>(options.max_pending_generations);
        let state = Arc::new((
            Mutex::new(QueueState {
                in_flight_generations: 0,
                in_flight_bytes: 0,
                telemetry: AsyncWriterTelemetry::default(),
                background_error: None,
            }),
            Condvar::new(),
        ));
        let bg_state = Arc::clone(&state);
        let bg_path = path.clone();
        let worker = thread::spawn(move || {
            let mut initialized_on_disk = bg_path.exists()
                && std::fs::metadata(&bg_path)
                    .map(|m| m.len() > 0)
                    .unwrap_or(false);

            while let Ok(task) = receiver.recv() {
                match task {
                    AsyncTask::StageAndCommit {
                        metadata,
                        raw_chunks,
                        index_entries,
                        queued_bytes,
                    } => {
                        let t_stage = Instant::now();
                        let encoded_inputs: Vec<EncodedChunkInput<'_>> = raw_chunks
                            .iter()
                            .map(|(cid, codec, buf)| EncodedChunkInput {
                                id: *cid,
                                codec: *codec,
                                data: buf.as_slice(),
                            })
                            .collect();
                        let stage_res =
                            stage_chunks_parallel(&encoded_inputs, options.staging_workers);
                        let stage_ns = t_stage.elapsed().as_nanos() as u64;

                        let (res, commit_ns, staged_bytes_sum) = match stage_res {
                            Ok(staged_chunks) => {
                                let sb: u64 = staged_chunks
                                    .iter()
                                    .map(|c| c.stored_bytes.len() as u64)
                                    .sum();
                                let t0 = Instant::now();
                                let r = if !initialized_on_disk {
                                    let w = write_staged_generation_container_atomic(
                                        &bg_path,
                                        &metadata,
                                        &staged_chunks,
                                        &index_entries,
                                        options.byte_order,
                                        options.durability,
                                    );
                                    if w.is_ok() {
                                        initialized_on_disk = true;
                                    }
                                    w
                                } else {
                                    append_staged_scientific_generation_file(
                                        &bg_path,
                                        &staged_chunks,
                                        &index_entries,
                                        &metadata,
                                        options.durability,
                                    )
                                    .map(|_| ())
                                };
                                (r, t0.elapsed().as_nanos() as u64, sb)
                            }
                            Err(e) => (Err(e), 0, 0),
                        };

                        let (lock, cvar) = &*bg_state;
                        if let Ok(mut guard) = lock.lock() {
                            guard.in_flight_generations =
                                guard.in_flight_generations.saturating_sub(1);
                            guard.in_flight_bytes =
                                guard.in_flight_bytes.saturating_sub(queued_bytes);
                            guard.telemetry.total_staged_bytes = guard
                                .telemetry
                                .total_staged_bytes
                                .saturating_add(staged_bytes_sum);
                            guard.telemetry.total_stage_duration_ns = guard
                                .telemetry
                                .total_stage_duration_ns
                                .saturating_add(stage_ns);
                            guard.telemetry.total_commit_duration_ns = guard
                                .telemetry
                                .total_commit_duration_ns
                                .saturating_add(commit_ns);
                            match res {
                                Ok(()) => {
                                    guard.telemetry.committed_generations =
                                        guard.telemetry.committed_generations.saturating_add(1);
                                }
                                Err(err) => {
                                    if guard.background_error.is_none() {
                                        guard.background_error = Some(err.to_string());
                                    }
                                }
                            }
                            cvar.notify_all();
                        }
                    }
                    AsyncTask::CommitGeneration {
                        metadata,
                        staged_chunks,
                        index_entries,
                        staged_bytes,
                    } => {
                        let t0 = Instant::now();
                        let res = if !initialized_on_disk {
                            let r = write_staged_generation_container_atomic(
                                &bg_path,
                                &metadata,
                                &staged_chunks,
                                &index_entries,
                                options.byte_order,
                                options.durability,
                            );
                            if r.is_ok() {
                                initialized_on_disk = true;
                            }
                            r
                        } else {
                            append_staged_scientific_generation_file(
                                &bg_path,
                                &staged_chunks,
                                &index_entries,
                                &metadata,
                                options.durability,
                            )
                            .map(|_| ())
                        };
                        let commit_ns = t0.elapsed().as_nanos() as u64;

                        let (lock, cvar) = &*bg_state;
                        if let Ok(mut guard) = lock.lock() {
                            guard.in_flight_generations =
                                guard.in_flight_generations.saturating_sub(1);
                            guard.in_flight_bytes =
                                guard.in_flight_bytes.saturating_sub(staged_bytes);
                            guard.telemetry.total_commit_duration_ns = guard
                                .telemetry
                                .total_commit_duration_ns
                                .saturating_add(commit_ns);
                            match res {
                                Ok(()) => {
                                    guard.telemetry.committed_generations =
                                        guard.telemetry.committed_generations.saturating_add(1);
                                }
                                Err(err) => {
                                    if guard.background_error.is_none() {
                                        guard.background_error = Some(err.to_string());
                                    }
                                }
                            }
                            cvar.notify_all();
                        }
                    }
                    AsyncTask::Flush => {
                        let (lock, cvar) = &*bg_state;
                        if let Ok(_guard) = lock.lock() {
                            cvar.notify_all();
                        }
                    }
                    AsyncTask::Shutdown => break,
                }
            }
        });

        Ok(Self {
            path,
            options,
            sender: Some(sender),
            state,
            worker: Some(worker),
        })
    }

    /// Target file path of this writer.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Stages `chunks` in parallel across `options.staging_workers` threads and enqueues
    /// the generation for asynchronous background commit.
    ///
    /// If the bounded queue (`max_pending_generations` or `max_queued_bytes`) is full,
    /// this method applies backpressure and records the exact wait duration in `telemetry`.
    pub fn submit_generation(
        &self,
        metadata: Metadata,
        chunks: &[EncodedChunkInput<'_>],
        index_entries: &[PersistentIndexEntry],
    ) -> Result<(), ChunkIoError> {
        let t_stage = Instant::now();
        let staged_chunks = stage_chunks_parallel(chunks, self.options.staging_workers)?;
        let stage_ns = t_stage.elapsed().as_nanos() as u64;

        self.submit_staged_generation_internal(
            metadata,
            staged_chunks,
            index_entries.to_vec(),
            stage_ns,
        )
    }

    /// Enqueues owned raw chunks `(chunk_id, codec, raw_bytes)` into the bounded queue
    /// so that both parallel compression/hashing (`stage_chunks_parallel`) AND disk I/O
    /// run asynchronously on the background pipeline while the NWP compute thread immediately
    /// proceeds to the next timestep.
    pub fn submit_raw_generation(
        &self,
        metadata: Metadata,
        raw_chunks: Vec<(u64, ChunkCodec, Vec<u8>)>,
        index_entries: Vec<PersistentIndexEntry>,
    ) -> Result<(), ChunkIoError> {
        if let Some(model) = metadata.scientific_model() {
            model.validate().map_err(ChunkIoError::ScientificModel)?;
        }
        let raw_bytes: u64 = raw_chunks.iter().map(|(_, _, b)| b.len() as u64).sum();

        let (lock, cvar) = &*self.state;
        let t_wait = Instant::now();
        let mut guard = lock
            .lock()
            .map_err(|_| ChunkIoError::Compression("async writer mutex poisoned".to_owned()))?;

        if let Some(ref err_msg) = guard.background_error {
            return Err(ChunkIoError::Compression(err_msg.clone()));
        }

        let mut experienced_backpressure = false;
        while guard.in_flight_generations >= self.options.max_pending_generations
            || (guard.in_flight_generations > 0
                && guard.in_flight_bytes.saturating_add(raw_bytes) > self.options.max_queued_bytes)
        {
            experienced_backpressure = true;
            guard = cvar.wait(guard).map_err(|_| {
                ChunkIoError::Compression("async writer condvar poisoned".to_owned())
            })?;
            if let Some(ref err_msg) = guard.background_error {
                return Err(ChunkIoError::Compression(err_msg.clone()));
            }
        }

        let stall_ns = if experienced_backpressure {
            t_wait.elapsed().as_nanos() as u64
        } else {
            0
        };

        guard.in_flight_generations += 1;
        guard.in_flight_bytes = guard.in_flight_bytes.saturating_add(raw_bytes);
        guard.telemetry.submitted_generations =
            guard.telemetry.submitted_generations.saturating_add(1);
        guard.telemetry.total_raw_bytes = guard.telemetry.total_raw_bytes.saturating_add(raw_bytes);
        if experienced_backpressure {
            guard.telemetry.backpressure_events =
                guard.telemetry.backpressure_events.saturating_add(1);
            guard.telemetry.total_enqueue_stall_ns = guard
                .telemetry
                .total_enqueue_stall_ns
                .saturating_add(stall_ns);
            if stall_ns > guard.telemetry.max_enqueue_stall_ns {
                guard.telemetry.max_enqueue_stall_ns = stall_ns;
            }
        }
        drop(guard);

        let sender = self
            .sender
            .as_ref()
            .ok_or_else(|| ChunkIoError::Compression("async writer already closed".to_owned()))?;
        sender
            .send(AsyncTask::StageAndCommit {
                metadata: Box::new(metadata),
                raw_chunks,
                index_entries,
                queued_bytes: raw_bytes,
            })
            .map_err(|_| ChunkIoError::Compression("async writer thread terminated".to_owned()))?;
        Ok(())
    }
    pub fn submit_staged_generation(
        &self,
        metadata: Metadata,
        mut staged_chunks: Vec<StagedChunk>,
        index_entries: Vec<PersistentIndexEntry>,
    ) -> Result<(), ChunkIoError> {
        staged_chunks.sort_by_key(|c| c.id);
        self.submit_staged_generation_internal(metadata, staged_chunks, index_entries, 0)
    }

    fn submit_staged_generation_internal(
        &self,
        metadata: Metadata,
        staged_chunks: Vec<StagedChunk>,
        index_entries: Vec<PersistentIndexEntry>,
        stage_ns: u64,
    ) -> Result<(), ChunkIoError> {
        let raw_bytes: u64 = staged_chunks.iter().map(|c| c.raw_len).sum();
        let staged_bytes: u64 = staged_chunks
            .iter()
            .map(|c| c.stored_bytes.len() as u64)
            .sum();

        let (lock, cvar) = &*self.state;
        let t_wait = Instant::now();
        let mut guard = lock
            .lock()
            .map_err(|_| ChunkIoError::Compression("async writer mutex poisoned".to_owned()))?;

        if let Some(ref err_msg) = guard.background_error {
            return Err(ChunkIoError::Compression(err_msg.clone()));
        }

        let mut experienced_backpressure = false;
        while guard.in_flight_generations >= self.options.max_pending_generations
            || (guard.in_flight_generations > 0
                && guard.in_flight_bytes.saturating_add(staged_bytes)
                    > self.options.max_queued_bytes)
        {
            experienced_backpressure = true;
            guard = cvar.wait(guard).map_err(|_| {
                ChunkIoError::Compression("async writer condvar poisoned".to_owned())
            })?;
            if let Some(ref err_msg) = guard.background_error {
                return Err(ChunkIoError::Compression(err_msg.clone()));
            }
        }

        let stall_ns = if experienced_backpressure {
            t_wait.elapsed().as_nanos() as u64
        } else {
            0
        };

        guard.in_flight_generations += 1;
        guard.in_flight_bytes = guard.in_flight_bytes.saturating_add(staged_bytes);
        guard.telemetry.submitted_generations =
            guard.telemetry.submitted_generations.saturating_add(1);
        guard.telemetry.total_raw_bytes = guard.telemetry.total_raw_bytes.saturating_add(raw_bytes);
        guard.telemetry.total_staged_bytes = guard
            .telemetry
            .total_staged_bytes
            .saturating_add(staged_bytes);
        guard.telemetry.total_stage_duration_ns = guard
            .telemetry
            .total_stage_duration_ns
            .saturating_add(stage_ns);
        if experienced_backpressure {
            guard.telemetry.backpressure_events =
                guard.telemetry.backpressure_events.saturating_add(1);
            guard.telemetry.total_enqueue_stall_ns = guard
                .telemetry
                .total_enqueue_stall_ns
                .saturating_add(stall_ns);
            if stall_ns > guard.telemetry.max_enqueue_stall_ns {
                guard.telemetry.max_enqueue_stall_ns = stall_ns;
            }
        }
        drop(guard);

        let sender = self
            .sender
            .as_ref()
            .ok_or_else(|| ChunkIoError::Compression("async writer already closed".to_owned()))?;
        sender
            .send(AsyncTask::CommitGeneration {
                metadata: Box::new(metadata),
                staged_chunks,
                index_entries,
                staged_bytes,
            })
            .map_err(|_| ChunkIoError::Compression("async writer thread terminated".to_owned()))?;
        Ok(())
    }

    /// Waits until all currently queued generations have been committed by the background thread.
    pub fn flush(&self) -> Result<AsyncWriterTelemetry, ChunkIoError> {
        if let Some(ref sender) = self.sender {
            let _ = sender.send(AsyncTask::Flush);
        }
        let (lock, cvar) = &*self.state;
        let mut guard = lock
            .lock()
            .map_err(|_| ChunkIoError::Compression("async writer mutex poisoned".to_owned()))?;
        while guard.in_flight_generations > 0 && guard.background_error.is_none() {
            guard = cvar.wait(guard).map_err(|_| {
                ChunkIoError::Compression("async writer condvar poisoned".to_owned())
            })?;
        }
        if let Some(ref err_msg) = guard.background_error {
            return Err(ChunkIoError::Compression(err_msg.clone()));
        }
        Ok(guard.telemetry)
    }

    /// Returns a snapshot of the writer telemetry without blocking.
    pub fn telemetry(&self) -> Result<AsyncWriterTelemetry, ChunkIoError> {
        let (lock, _) = &*self.state;
        let guard = lock
            .lock()
            .map_err(|_| ChunkIoError::Compression("async writer mutex poisoned".to_owned()))?;
        if let Some(ref err_msg) = guard.background_error {
            return Err(ChunkIoError::Compression(err_msg.clone()));
        }
        Ok(guard.telemetry)
    }

    /// Flushes all pending generations, shuts down the background thread, and returns final telemetry.
    pub fn close(mut self) -> Result<AsyncWriterTelemetry, ChunkIoError> {
        let telemetry = self.flush()?;
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(AsyncTask::Shutdown);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        Ok(telemetry)
    }
}

impl Drop for AsyncScientificWriter {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(AsyncTask::Shutdown);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
