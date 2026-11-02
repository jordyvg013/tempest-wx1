# WX-1 persistent B+tree indexes — experimental draft v0.3.9

**Status:** experimental, non-normative; not a stable interoperability contract.

This draft extends the v0.3.8 append-only generation profile with immutable, persistent B+tree pages. The primary chunk directory remains authoritative. Indexes are secondary accelerators and MUST NOT be used as an unchecked source of payload offsets.

## 1. Profile and compatibility

- New files use container header major `1`, minor `5`, the v0.3.8 directory schema v2 (16-byte directory header and 72-byte descriptors), and generation footer version `2`.
- Readers MUST continue to read header minor `4` files with the v0.3.8 64-byte generation footer version `1` and no persistent index catalog.
- Readers MUST continue to read the legacy directory-v1 / optional `WX1C` path.
- A reader MUST NOT assume that an unknown later header minor or footer version is compatible.
- Header `directory_offset` remains zero for generation files. The current directory root and persistent-index catalog pointer are in the generation footer.

## 2. Generation footer version 2

The footer is exactly 80 bytes. All multi-byte integers use the container byte order.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | Magic `WX1G` |
| 4 | 2 | Footer version `2` |
| 6 | 2 | Footer length `80` |
| 8 | 8 | Generation number, starting at `1` |
| 16 | 8 | Previous footer absolute offset; zero for generation 1 |
| 24 | 8 | Directory absolute offset for this generation |
| 32 | 8 | Persistent-index catalog page absolute offset |
| 40 | 32 | BLAKE3 over the complete generation region, excluding this footer |
| 72 | 8 | Reserved; MUST be zero |

The generation digest covers, in physical order, the new stored chunk payloads, this generation's complete directory, all newly written B+tree pages, and the index catalog page. Reused pages from earlier generations are not copied into the new generation and are covered by their original generation digest.

The catalog pointer MUST identify the final 4096-byte page immediately preceding the footer. A v0.3.9 generation MUST contain a catalog page, even when some optional logical indexes are absent. The chunk-ID index is mandatory; an empty dataset represents it with a zero root and zero item count.

## 3. Index page layout

Every index page is exactly 4096 bytes. Pages are immutable after publication. All multi-byte fields use the container byte order. The page begins with a 64-byte header:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | Magic `WX1B` |
| 4 | 2 | Page schema version `1` |
| 6 | 1 | Page kind: `1` leaf, `2` internal, `3` catalog |
| 7 | 1 | Flags; currently zero |
| 8 | 4 | Page size `4096` |
| 12 | 2 | B+tree level (`0` for leaf; positive for internal; `0` for catalog) |
| 14 | 2 | Record/separator count |
| 16 | 8 | Generation number in which this page was written |
| 24 | 32 | BLAKE3 page digest |
| 56 | 8 | Reserved; MUST be zero |

The page digest is `BLAKE3(page[0..4096])` after bytes `24..56` have been set to zero. The stored digest is then copied into bytes `24..56`. Readers MUST verify the digest before trusting page fields beyond the fixed header and MUST reject nonzero padding.

### 3.1 Leaf page (kind 1, level 0)

Each record is exactly 40 bytes:

| Offset in record | Size | Field |
|---:|---:|---|
| 0 | 32 | Opaque, fixed-width index key |
| 32 | 8 | Referenced chunk ID |

Records are strictly ordered by `(key bytes lexicographically, chunk_id numerically)`. A repeated logical key is allowed for different chunk IDs. A repeated `(key, chunk_id)` pair is invalid. Maximum records per page: `floor((4096 - 64) / 40) = 100`.

### 3.2 Internal page (kind 2, level greater than zero)

The payload starts with one 8-byte child-page offset. It is followed by `record_count` separators, each exactly 48 bytes:

- 32-byte opaque key;
- 8-byte chunk ID;
- 8-byte right-child page offset.

A page with `N` separators has exactly `N + 1` children. Each separator is the minimum composite key in the child immediately to its right. Child ranges MUST be strictly ordered and non-overlapping. Maximum separators per page: `floor((4096 - 64 - 8) / 48) = 83` (maximum 84 children).

All child offsets MUST point to complete, previously written pages in a committed index-page region. A child offset MUST be less than its parent page offset. Cycles, repeated child pages within a tree, invalid levels, incorrect separator bounds, and out-of-range offsets are errors.

### 3.3 Catalog page (kind 3, level 0)

Each catalog record is exactly 24 bytes:

| Offset in record | Size | Field |
|---:|---:|---|
| 0 | 2 | Logical index kind ID |
| 2 | 2 | Flags; bit 0 means required; all other bits zero |
| 4 | 4 | Reserved; MUST be zero |
| 8 | 8 | B+tree root page offset; zero only when item count is zero |
| 16 | 8 | Number of leaf records in this logical index |

Catalog kinds MUST be unique and sorted numerically. Maximum catalog kinds per page: 64 for this profile. Unknown required kinds MUST be rejected. Unknown optional kinds may be skipped while retaining the opaque catalog entry. The catalog page MUST be the last page of the generation and its offset is recorded in footer v2.

## 4. Logical index kinds and key contract

This version defines these kind IDs:

| ID | Kind | Key meaning in v0.3.9 |
|---:|---|---|
| 1 | Chunk ID | Unsigned chunk ID encoded as 8 big-endian bytes followed by 24 zero bytes |
| 2 | Variable | Caller-provided variable ID encoded as 4 big-endian bytes followed by 28 zero bytes |
| 3 | Time | Caller-provided unsigned time-index ordinal encoded as 8 big-endian bytes followed by 24 zero bytes |
| 4 | Vertical | Caller-provided unsigned level-index ordinal encoded as 4 big-endian bytes followed by 28 zero bytes |
| 5 | Spatial | Caller-provided unsigned spatial-tile ID encoded as 8 big-endian bytes followed by 24 zero bytes |
| 6 | Model/provider | Opaque caller-defined 32-byte key |

The 32-byte key is an opaque, canonical sort key. Index page ordering is bytewise lexicographic. Chunk ID is the secondary ordering component, not part of the 32-byte key.

The existing `ScientificChunkKey` is an explicit ordinal key containing variable ID, time index, level index, and spatial tile ID. v0.3.9 may persist those values as four separate index entries for a chunk. Its `time_index` is NOT a timestamp; `level_index` is NOT a physical level value or unit; `spatial_tile` is NOT a CRS-aware geometry. Calendar semantics, actual time intervals, level type/value/unit, CRS, axis order, longitude wrapping, regular-grid bounding boxes and spatial range semantics are deferred to the v0.4.0 scientific data model. Implementations MUST NOT infer them in this release.

## 5. Copy-on-write and generation publication

- Published index pages are never overwritten.
- Insertion writes new page(s) along the modified B+tree search path; unchanged subtrees may retain their old page offsets. A split writes new left/right pages and propagates a separator upward. Root splits create a new root page.
- The generation writes new chunks, its directory, new COW B+tree pages, and a new catalog page, then syncs data/pages before publishing footer v2. It syncs the footer before reporting durable file-backed success.
- The new catalog may reuse roots from prior generations for indexes not modified in this commit.
- Deletion, replacement of an existing `(key, chunk_id)` mapping, page compaction and vacuum are out of scope. A duplicate mapping is an error.
- The chunk-ID tree is maintained automatically for every new chunk. Secondary-index insertions are explicit caller input; absent indexes are not guessed.

## 6. Directory authority and validation

Every visible index entry's chunk ID MUST resolve to a descriptor in the fully validated primary directory chain. The chunk-ID index MUST contain exactly one mapping for every visible directory chunk and no extra mappings; key and value MUST encode the same ID. Secondary indexes may contain multiple chunks per key but MUST NOT contain a duplicate composite key/value pair.

Index page offsets MUST lie wholly inside a page region appended after a generation's directory and before that generation's footer. A page region is a whole-number sequence of 4096-byte pages. Its generation number MUST match the footer generation for that region. A page referenced from a later generation may be in any earlier committed page region, never in uncommitted tail bytes.

Readers MUST validate page BLAKE3, header/version/kind/flags/reserved/padding, level and count limits, child offsets, separator ordering, tree depth, duplicate/cyclic references, root item counts and all chunk-ID references. Index validation MUST NOT replace directory/chunk validation. A corrupt index MUST produce a specific index error; the implementation MUST NOT silently use a corrupt offset. A missing optional index produces a specific `IndexNotPresent` result for that query. The mandatory chunk-ID index missing or inconsistent is a corrupt-index error.

## 7. Query operations

The API supports exact-key lookup and inclusive key-range lookup for one logical index kind. Results are chunk IDs ordered by `(key, chunk_id)`. Query result limits and tree traversal depth are bounded. Every result is resolved against the pinned snapshot's descriptor set before being returned.

The snapshot remains pinned to the footer/catalog selected at open. A later append does not change its visible index roots or results.

## 8. Resource limits and failure behavior

- Page size is fixed at 4096; pages are read into a fixed-size bounded buffer.
- Maximum catalog entries: 64.
- Maximum tree depth: 64.
- Maximum visited pages per logical tree: 1,000,000.
- Maximum records per logical tree: 1,000,000.
- All offset arithmetic and allocation-size conversions are checked before use.
- Unknown mandatory page versions, kinds or flags are rejected with an index-specific error.
- No parser or query path may use file-controlled sizes for an unbounded allocation.
- No file-input path may panic or use `unwrap`/`expect`.

## 9. Explicitly out of scope

- A standardized meteorological timestamp/calendar schema.
- Physical vertical-coordinate semantics and unit conversion.
- CRS-aware bounding-box, longitude-wrapping or grid-neighborhood queries.
- Spatial R-tree/Hilbert/grid specialized indexes.
- Delete/update of existing index mappings, vacuum/compaction and index-repair CLI.
- Python, GRIB1/GRIB2, NetCDF, Zarr, xarray and GPU.

These are not implied by the presence of the generic time, vertical and spatial-tile keyspaces. They require the v0.4 scientific schema and separate specifications.
