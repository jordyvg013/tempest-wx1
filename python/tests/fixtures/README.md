# Local Python adapter fixtures

All adapter tests are offline. Tests create deterministic small NetCDF fixtures in a pytest temporary directory. NetCDF3/scipy is exercised whenever SciPy is installed; a separate NetCDF4 fixture test runs only when the `netCDF4` optional engine is installed. Zarr v2 fixtures are created locally when Zarr is installed. The GRIB2 reader test requires a local fixture path supplied via `WX1_GRIB2_FIXTURE`; the test does not download data or assume a sample-file location.

For a formal cross-format benchmark, fixtures must additionally be accompanied by a SHA256 manifest, dimensions, coordinate/units metadata, license/provenance and exact library versions. The small tests here are functional smoke/roundtrip tests, not benchmark datasets.
