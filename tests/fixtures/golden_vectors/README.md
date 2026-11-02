# WX-1 Independent Conformance Golden Vectors (v1.0-draft)

This directory contains deterministic binary test vectors for validating WX-1 format implementations across different programming languages (Rust, Python, C, Fortran) and target platforms (x86_64, aarch64, Windows, macOS, Linux).

See `provenance.json` for full machine-readable metadata and SHA-256 hashes.

## Vector Catalogue

| ID | File | Category | Size | Expected Behavior | Description |
| :--- | :--- | :--- | :---: | :--- | :--- |
| **`GV-01`** | `gv01_valid_scalar_2d_zstd.wx1` | Valid | 9.0 KB | Must open cleanly | 2D surface field (`t2m`, 16×16) with Zstd compression and CF metadata. |
| **`GV-02`** | `gv02_valid_volume_3d_lz4.wx1` | Valid | 9.3 KB | Must open cleanly | 3D isobaric volume (`gh`, 4×8×8) with LZ4 compression. |
| **`GV-03`** | `gv03_valid_multigen_append.wx1` | Valid | 33 KB | Must open cleanly | 3 append generations with evolving time dimension. |
| **`GV-04`** | `gv04_valid_special_floats.wx1` | Valid | 8.9 KB | Must open cleanly | IEEE-754 bit-exact values: NaN, +Inf, -Inf, -0.0, +0.0, 1.0. |
| **`GV-05`** | `gv05_corrupt_magic.wx1` | Corrupt | 9.0 KB | Must reject with error | Container magic byte corruption (`BAD\0`). |
| **`GV-06`** | `gv06_corrupt_footer_crc.wx1` | Corrupt | 9.0 KB | Must reject with error | Damaged commit footer checksum. |
| **`GV-07`** | `gv07_corrupt_chunk_blake3.wx1` | Corrupt | 9.0 KB | Must reject with error | Single bit-flip inside compressed chunk payload. |
| **`GV-08`** | `gv08_corrupt_truncated_payload.wx1` | Corrupt | 8.9 KB | Must reject with error | Container truncated by 50 bytes mid-file. |
| **`GV-09`** | `gv09_corrupt_unsupported_codec.wx1` | Corrupt | 9.0 KB | Must reject with error | Unsupported codec ID `0x99` in chunk descriptor. |
| **`GV-10`** | `gv10_malicious_path_traversal.wx1` | Security | 9.0 KB | Must reject with error | Path traversal injection (`../`) in variable identifier. |
| **`GV-11`** | `gv11_malicious_decompression_bomb.wx1` | Security | 9.0 KB | Must reject with error | Decompression bomb declaring raw length > 64 MiB limit. |
