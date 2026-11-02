# WX-1 durable directory and append-only generations — experimental draft v0.3.8

**Status:** experimental, non-normative; not a stable interoperability contract.

This draft adds directory schema v2 and append-only generation commits while retaining the v0.3.5–v0.3.7 v1 directory/commit reader path. The existing 32-byte container header and metadata schema are unchanged.

## 1. Container preamble and version dispatch

The 32-byte header and deterministic-CBOR metadata payload keep the layouts defined in ADR 0009 and the v0.3.4 metadata profile. Generation containers use header major version `1`, minor version `4`, and **MUST keep `header.directory_offset = 0`**. The active directory/root offset lives in the generation footer; it is never written into the mutable header. Metadata begins at byte 32 and the first generation starts immediately after metadata.

Legacy files with directory schema v1 (40-byte descriptors), including optional `WX1C` v1 commit footers, remain on the legacy reader path. A generation-profile header (minor version exactly `4` in this release) is read using the `WX1G` footer chain and directory schema v2. Unknown later minor versions are not assumed compatible. A file is not upgraded in place by a reader.

## 2. Directory v2

Directory magic remains the four bytes `WX1D`. The 16-byte directory header is unchanged except that schema version is `2` and descriptor size is `72`:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | Magic `WX1D` |
| 4 | 2 | Directory schema version: `2` |
| 6 | 2 | Descriptor size: `72` |
| 8 | 4 | Entry count |
| 12 | 4 | Reserved, MUST be zero |

All multi-byte directory-header and descriptor fields use the container's byte-order marker. The descriptor is:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Unique chunk ID; unique across the complete committed generation chain |
| 8 | 8 | Absolute stored-payload file offset |
| 16 | 8 | Stored byte length |
| 24 | 8 | Decoded/raw byte length |
| 32 | 4 | CRC32C of stored payload bytes |
| 36 | 1 | Codec: 0 raw, 1 Zstandard, 2 LZ4 block |
| 37 | 1 | Flags; currently zero |
| 38 | 2 | Reserved; MUST be zero |
| 40 | 32 | BLAKE3 digest of stored payload bytes |

The BLAKE3 chunk digest is exactly `BLAKE3(file[offset .. offset + stored_len])`: the bytes as stored on disk, **after compression** when a codec is used. It excludes the descriptor, directory, footer, and decoded/raw representation. CRC32C keeps its existing meaning and also covers precisely those stored payload bytes. CRC32C and BLAKE3 are independent checks: CRC32C is the quick accidental-corruption check; BLAKE3 is the stronger per-chunk digest. Neither is a signature.

Each v2 directory describes only the chunks introduced by its generation. The combined view of a snapshot is the ordered union of its valid chain's directories. Duplicate chunk IDs in one directory or anywhere in the chain invalidate that candidate generation. Entry counts, stored/raw lengths, directory allocation sizes, offsets and ranges are checked before allocation or I/O. Chunks MUST lie inside their own generation's payload region and MUST NOT overlap each other or the directory.

## 3. Generation footer

Every committed generation ends with a fixed 64-byte footer. Footer fields use the container's declared byte order (the older `WX1C` footer remains little-endian and unchanged).

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | Magic `WX1G` |
| 4 | 2 | Footer schema version: `1` |
| 6 | 2 | Footer length: `64` |
| 8 | 8 | Generation number; starts at `1` and increments by exactly one |
| 16 | 8 | `prev_footer_offset`; zero for generation 1, otherwise the absolute offset of the preceding footer's first byte |
| 24 | 8 | Root/directory offset for this generation |
| 32 | 32 | BLAKE3 digest of this generation's data and directory bytes |

A footer MUST be the final 64 bytes of its generation. The root offset MUST point to that generation's v2 directory, and the directory's checked end offset MUST equal the footer offset. No index pages exist in this release.

For generation 1, `generation_start = 32 + metadata_len`. For generation `n > 1`, `generation_start = prev_footer_offset + 64`. The generation digest is exactly `BLAKE3(file[generation_start .. footer_offset])`; it covers every new stored payload byte and the complete v2 directory bytes for this generation, but excludes the footer itself and all earlier generations, footers, the static header and metadata preamble. A footer is not valid unless its structural fields, previous-footer link, generation number, directory, per-chunk CRC32C and per-chunk BLAKE3 digests, and generation digest validate.

## 4. Append and durability ordering

Writers MUST append at physical EOF and MUST NOT rewrite bytes belonging to an earlier committed generation or modify the header. A writer MUST reject an existing file with trailing bytes beyond its last fully valid footer; the caller must explicitly run repair first.

The required file durability sequence is:

1. Write all new payload bytes.
2. Write the complete v2 directory.
3. Flush buffered output and call `sync_data` (or the platform-equivalent durability operation) on file-backed writers.
4. Write the 64-byte `WX1G` footer last.
5. Flush and call `sync_all` before reporting a durable commit.

A generic `Read + Write + Seek` API can enforce write ordering and `flush`, but cannot promise filesystem durability by itself. The file-backed append API performs the sync operations. A newly created file is preferably built in a sibling temporary file, fully synced, atomically renamed, and followed by parent-directory sync where the platform supports it. Hardware caches, network filesystems and platform-specific sync/rename behavior may weaken durability guarantees; these are not a universal power-loss proof.

A generation is logically visible only after its complete footer is present and every validation above succeeds. A failed/incomplete append MUST leave prior committed bytes intact. A later append MUST NOT silently continue past an invalid trailing tail.

This release supports **one writer per file at a time**. Callers MUST externally serialize append, repair, and any competing file mutation; the file-backed API does not acquire a cross-process lock. Concurrent writers are unsupported and may race while selecting EOF and the previous footer. Multiple readers may hold independent pinned snapshots while a single writer appends.

## 5. Opening, scan and fallback

Opening a generation-profile file first checks the header and bounded metadata. The reader checks the terminal footer candidate and, if that candidate or its chain is invalid, scans backward for `WX1G` candidates using bounded buffers. Candidates are considered from greatest to least physical offset. A candidate is eligible only after its complete footer chain validates:

- generation numbers descend by one when following `prev_footer_offset` and terminate at generation 1 with a zero predecessor;
- footer offsets and generation ranges are monotonic, non-overlapping and within actual file bounds;
- each directory uses schema v2, has bounded entry count and exactly terminates at its footer;
- chunk IDs are unique across the chain; all chunk ranges, codec fields, flags and reserved fields validate;
- every chunk's stored-byte CRC32C and BLAKE3 digest validate; and
- the BLAKE3 digest for each generation's `[generation_start, footer_offset)` bytes validates.

The reader selects the fully valid candidate with the greatest footer offset. Bytes after that footer are ignored by read-only open; they are **never truncated, rewritten, or silently repaired**. If no valid generation exists, the reader returns a specific failure (preserving the most specific observed structural/integrity error where available, otherwise `NoValidGeneration`). The backwards scan uses bounded working memory and a hard candidate limit to avoid unbounded work on hostile files.

Opening a generation snapshot validates the full chain and all stored chunks. This is intentionally conservative and can read the entire committed payload, so open latency is proportional to committed file size. Future releases may add a separately specified validation policy; this release does not weaken the full validation rule.

## 6. Snapshots

A `GenerationSnapshot` is pinned to the footer offset/generation selected when it is opened. Its metadata and descriptor set do not advance when later generations are appended. Payload reads through the snapshot accept only descriptors contained in that snapshot and re-check the selected chunk's integrity before returning decoded bytes. Since this release is append-only, older payload offsets remain stable. The reader never infers that a newer generation is part of an existing snapshot.

## 7. Repair and CLI contract

Inspection and normal read APIs are read-only. The explicit `repair <file>` command may truncate bytes strictly after the end of the last fully valid generation footer and sync the file. It MUST first validate a complete generation and MUST make no changes if no valid generation exists. It MUST NOT reconstruct missing payloads, invent directory entries, or modify previously committed bytes. Appending after an incomplete tail requires explicit repair first.

CLI success output is emitted only after the requested operation and all validations finish. Failed operations return a non-zero exit status and do not print a success summary. `--help` documents generation creation, append, inspection and repair. Inspection does not call repair.

## 8. Error model

The implementation distinguishes directory-format/structure/bounds errors, legacy v1 commit-footer structure errors, generation-footer structure errors, generation-chain errors, generation digest mismatches, per-chunk BLAKE3 mismatches, CRC32C mismatches, malformed metadata, unsupported codecs, I/O errors and the absence of any valid generation. An outer `InvalidDirectory` error MUST NOT replace a more specific footer, chain, BLAKE3 or CRC error.

## 9. Compatibility and non-goals

- Existing v1 directory files and the legacy `WX1C` footer remain readable through the v1 path. The supplied v0.3.7-fix3 archive contains no checked-in binary fixture files; the regression suite exercises v1 output from the retained legacy writer.
- v1 readers are not expected to read v2 descriptors or `WX1G` generation files.
- The v2 layout is experimental and non-normative; no v1.0 interoperability promise is made.
- Durable B+tree/index pages, GPU, Python bindings, xarray, mutation/replacement of existing chunk IDs, and large refactors are out of scope. A new generation only introduces new unique chunk IDs.
