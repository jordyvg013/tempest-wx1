# ADR 0014: Durable directory v2 and append-only commit generations

- Status: Accepted for experimental v0.3.8 implementation; not a stable interoperability contract
- Date: 2026-10-30

## Context

v0.3.7-fix3 has a 40-byte v1 chunk descriptor, stored-byte CRC32C, and an optional `WX1C` footer that hashes the complete file prefix. Its writer back-patches `header.directory_offset`; it cannot append multiple independently recoverable generations, and a chunk's BLAKE3 digest is not persisted in the directory. A partially appended update must not invalidate an earlier commit.

## Decision

1. Retain the current v1 directory and `WX1C` reader/writer paths for compatibility. Add directory schema v2, a 72-byte descriptor, and a fixed 64-byte `WX1G` footer.
2. Store a 32-byte BLAKE3 digest for every chunk. Hash the exact stored bytes in `file[offset..offset+stored_len]`, after compression when applicable. Preserve CRC32C over the same stored bytes.
3. Make each generation append-only. A generation directory lists only its new chunks; the snapshot's directory view is the union of all directories in the validated chain. Chunk IDs remain globally unique across the chain.
4. Keep `header.directory_offset` zero for generation-profile files. The generation footer is the authoritative root pointer. It records generation number, previous-footer offset, directory offset, and BLAKE3 of only that generation's newly appended payload and directory bytes (excluding the footer). Exact bytes and byte order are frozen for this experimental profile in the v0.3.8 draft specification.
5. A generation is accepted only if the complete footer chain, each directory, all chunk CRC32C/BLAKE3 values, and every generation digest validate. A read-only open scans backward using bounded buffers and chooses the valid generation with the greatest physical footer offset. Invalid trailing bytes are ignored for reading but never rewritten by normal open/inspect.
6. Repair is explicit and only truncates the tail after a fully valid generation. An append refuses to proceed when an uncommitted/invalid tail exists. File-backed writer APIs sync data and directory before writing the footer, then sync the footer before reporting durable success.
7. Require one writer per file at a time. Callers serialize append/repair externally; the file-backed API does not take a cross-process lock. Multiple independent readers may retain pinned snapshots while one writer appends.
8. Keep index pages, Python, GPU and scientific-array schema work out of v0.3.8. CPU-only Rust remains the complete storage implementation.

## Alternatives considered

- **Continue back-patching the header:** rejected because overwriting the root pointer violates append-only semantics and can leave the header pointing at an incomplete generation after a torn update.
- **Keep only a whole-file hash:** rejected because it cannot validate a generation independently and prevents straightforward fallback to an earlier commit.
- **Hash decoded payload bytes:** rejected because it requires decompression before integrity verification, is codec-dependent, and does not identify the exact bytes stored in the file.
- **Treat CRC32C as sufficient:** rejected because CRC32C is retained for fast detection but is not the stronger persisted per-chunk digest required by the roadmap.
- **Repair automatically on open:** rejected because it silently mutates data and makes inspection side-effecting.
- **Allow duplicate chunk IDs to shadow earlier entries:** rejected for this release. Updates/replacements need explicit copy-on-write semantics and are deferred.

## Consequences

- The v1 reader and existing v1 fixtures remain supported, but v1 readers cannot read v2 generation files.
- Opening a v2 file performs conservative full-chain validation, including reading stored chunks. Open cost is proportional to the committed data size; this favors integrity over open latency in the experimental milestone.
- Concurrent writers are unsupported. Callers must externally serialize appends and repairs; readers can retain snapshots while one writer appends.
- Generic writer APIs can guarantee ordering and `flush`, but only file-backed APIs can call `sync_data`/`sync_all`. The specification documents this distinction and platform/filesystem limitations.
- A failed append does not produce a valid footer and leaves earlier generations readable. Before another append, users must explicitly repair an invalid trailing tail.
- The wire format remains experimental. External interoperability vectors, fuzzing and power-loss testing are still required before stability claims.
