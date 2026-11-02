# ADR 0013: Streaming I/O, BLAKE3 commit footer and bounded processing

- Status: Experimental implementation in v0.3.7; not a stable interoperability contract
- Date: 2026-10-30

## Decisions

- Add reader-based chunk inputs. Raw and Zstandard writes stream from a caller-owned reader; LZ4 block encoding still buffers one chunk because the selected block API is not streaming.
- Verify stored-byte CRC32C in a first bounded pass, then stream raw/Zstandard decoded bytes directly to a caller-owned writer. LZ4 block decode remains bounded by the existing 64 MiB chunk cap.
- Raw range reads verify the complete chunk checksum, then copy only the requested byte interval.
- Buffer size is caller-configurable from 1 byte to 8 MiB. Input chunk splitting accepts a caller-selected size up to the existing 64 MiB chunk limit.
- Add BLAKE3 streaming hash helpers and an optional fixed 48-byte `WX1C` commit footer. The footer stores a version, committed-prefix length and BLAKE3 digest of all preceding bytes. It detects accidental changes; it is not a keyed signature.
- Readers continue accepting legacy files without a footer. When a footer is present, it must be exactly at EOF and its digest must validate.
- Add in-memory chunk-ID/physical-offset indexes and caller-keyed scientific index maps. These are preparation APIs, not persisted B+tree pages or spatial trees.
- Add temp-file, `sync_all`, atomic rename publication helpers. This protects publication of a new file, but is not yet an append-only multi-generation recovery protocol.
- Batch iteration exposes bounded descriptor batches; worker scheduling and parallel writes remain caller-controlled. No internal thread pool is introduced.

## Explicit limitations

- BLAKE3 per-chunk digests are computable but are not yet persisted in each descriptor; the optional commit footer hashes the complete committed prefix.
- Variable/time/vertical/spatial indexes are currently in-memory caller-keyed maps, not durable on-disk index pages.
- The commit footer validates one complete file image. Append-only generations, previous-commit fallback, crash recovery scanning and torn-write recovery remain future work.
- LZ4 block operations still materialize a single bounded chunk. True LZ4 frame streaming needs a separately specified codec profile.
- Parallel processing is enabled by immutable descriptors and bounded batch enumeration, but no executor, cancellation protocol or concurrent file-handle pool is shipped.

## Compatibility

The existing 40-byte chunk descriptor and directory version 1 are unchanged. Existing v0.3.5/v0.3.6 files remain readable. The commit footer is optional and appended after the directory; older readers will reject committed files with trailing bytes, so this remains an experimental v0.3.7 extension rather than a claim of backward reader compatibility.
