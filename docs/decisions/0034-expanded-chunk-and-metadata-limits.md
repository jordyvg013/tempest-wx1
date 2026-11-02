# ADR 0034: Expanded scientific chunk and metadata limits

- Status: Proposed; implementation and WSL verification pending
- Date: 2026-11-01
- Scope: Scientific model parser/validator, generation directory limits, index-entry limits, metadata encoding/decoding and planner benchmark fixtures

## Context

The Phase-2 planner target includes 1,048,576 chunk references. The historical `MAX_CHUNKS=1000000` and 16 MiB metadata cap prevent a fully validated/serialized scientific metadata model at that exact scale. Raising only the planner's in-memory vector would not test the actual scientific model's validation and serialization limits.

## Decision proposed

Raise the scientific/generation candidate maximum to `MAX_CHUNKS=1048576` and the corresponding metadata cap to `33554432` bytes (32 MiB), subject to source-level compatibility work and tests. Keep unrelated caps (for example, index-page count, per-chunk size and array-output size) unchanged unless a separate measured requirement and decision justifies them.

The 1,048,576 planner case must construct, validate and serialize the full in-memory `ScientificDataset` before timing the planner. It does not imply writing a file containing one million payload chunks.

## Compatibility analysis required

- Preserve the 72-byte generation descriptor, the distinct 40-byte legacy chunk descriptor, header/footer structures, directory entry layouts, index-page layouts and checksum encodings.
- Existing files written within earlier limits must remain readable.
- New files exceeding older readers' historical limits may be rejected by those readers. The implementation must make this forward-reader limitation explicit and must not reinterpret existing fields.
- If a profile/header-minor distinction is needed to make the new capacity unambiguous, that must be decided and specified before implementation; no silent profile reinterpretation.
- Count/length checks must occur before allocation; attacker-controlled sizes must use checked arithmetic and fallible reservation where applicable.

## Required tests before acceptance

1. Accept the new exact maximum in the intended profile.
2. Reject `MAX_CHUNKS+1` before large descriptor allocation.
3. Preserve old-profile caps and old-file readability.
4. Exercise directory, index-tree and scientific metadata decode limits.
5. Encode/decode metadata above 16 MiB but at or below 32 MiB in the new profile.
6. Reject metadata above 32 MiB before allocation.
7. Run format, workspace, Clippy, compatibility and benchmark tests.

## Consequences

The change affects resource policy and which files a reader accepts, even if no byte-layout field changes. It is not a claim of wire-layout compatibility with older readers for newly enlarged files. The decision remains proposed until code and tests are verified in WSL.
