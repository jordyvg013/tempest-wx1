# Benchmark datasets and fixtures

`bench-data` creates only the deterministic 31 x 16 synthetic F32 fixture. It writes
`.npy` arrays, a JSON manifest and SHA256 checksums to an external directory (default
`/tmp/wx1-cross-format-fixtures`). No observational data or generated binary fixture
is checked into the project ZIP or source tree.

The fixture is a smoke-test dataset, not an ECMWF/ERA5/HARMONIE performance dataset.
Its F32 values are multiples of 0.125 K. WX-1, NetCDF4 and Zarr3 are checked for
bit-exact values; the GRIB2 simple-packing adapter is checked against its declared
0.001 K maximum absolute error bound. Coordinate and attribute preservation are
checked only where the adapter declares those capabilities.

The current WX-1 Python writer cannot persist coordinate variable arrays or global
attributes. Its fixture roundtrip therefore checks data/dtype/shape, and reports
coordinate/global-attribute preservation as unsupported rather than pretending the
formats have equivalent metadata semantics.
