# WX-1 format primitives — v0.3

This milestone adds bounds-checked fixed-width decoding only. It is not a file format and does not establish a public header, magic value, alignment, footer, index-page encoding or CBOR schema.

`wx1-format::Decoder` accepts an explicit `ByteOrder` and reads u16/u32/u64 and IEEE-754 f32/f64 values. Failed reads return an error and do not advance the cursor. No unsafe code or allocation is used. Little-endian remains the recommended default for future file creation; big-endian decoding is supported.

The legacy `WX1X` prefix probe remains explicitly non-normative and is retained only for backward diagnostic purposes.
