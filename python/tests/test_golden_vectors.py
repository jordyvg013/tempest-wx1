"""Independent Conformance Golden Vector Verification Suite (ADR 0031)."""

from pathlib import Path
import numpy as np
import pytest
import wx1

GOLDEN_DIR = Path(__file__).resolve().parent.parent.parent / "tests" / "fixtures" / "golden_vectors"


def test_gv01_valid_scalar_2d_zstd():
    path = GOLDEN_DIR / "gv01_valid_scalar_2d_zstd.wx1"
    assert path.exists(), f"Golden vector {path} must exist"
    f = wx1.open_file(str(path))
    assert f.dataset_id == "gv01"
    assert any(v["name"] == "t2m" for v in f.variables)
    data = f.read_array("/t2m")
    assert data.shape == (16, 16)
    assert data.dtype == np.float32
    assert np.isclose(data[0, 0], 273.15, atol=1e-4)
    assert np.isclose(data[15, 15], 273.15 + 25.5, atol=1e-3)


def test_gv02_valid_volume_3d_lz4():
    path = GOLDEN_DIR / "gv02_valid_volume_3d_lz4.wx1"
    assert path.exists()
    f = wx1.open_file(str(path))
    assert any(v["name"] == "gh" for v in f.variables)
    data = f.read_array("/gh")
    assert data.shape == (4, 8, 8)
    assert data.dtype == np.float32
    assert data[0, 0, 0] == 1000.0


def test_gv03_valid_multigen_append():
    path = GOLDEN_DIR / "gv03_valid_multigen_append.wx1"
    assert path.exists()
    f = wx1.open_file(str(path))
    assert f.generation == 3
    data = f.read_array("/temperature")
    assert data.shape == (3, 2)
    assert data[0, 0] == 10.0
    assert data[2, 1] == 60.0


def test_gv04_valid_special_floats():
    path = GOLDEN_DIR / "gv04_valid_special_floats.wx1"
    assert path.exists()
    f = wx1.open_file(str(path))
    data = f.read_array("/special_floats")
    assert data.shape == (6,)
    assert np.isnan(data[0])
    assert np.isinf(data[1]) and data[1] > 0
    assert np.isinf(data[2]) and data[2] < 0
    assert np.signbit(data[3]) and data[3] == 0.0  # -0.0
    assert not np.signbit(data[4]) and data[4] == 0.0  # +0.0
    assert data[5] == 1.0


def test_gv05_corrupt_magic_fails_cleanly():
    path = GOLDEN_DIR / "gv05_corrupt_magic.wx1"
    assert path.exists()
    with pytest.raises(Exception):
        wx1.open_file(str(path))


def test_gv06_corrupt_footer_crc_fails_cleanly():
    path = GOLDEN_DIR / "gv06_corrupt_footer_crc.wx1"
    assert path.exists()
    with pytest.raises(Exception):
        wx1.open_file(str(path))


def test_gv07_corrupt_chunk_blake3_fails_cleanly():
    path = GOLDEN_DIR / "gv07_corrupt_chunk_blake3.wx1"
    assert path.exists()
    with pytest.raises(Exception):
        f = wx1.open_file(str(path))
        f.read_array("/t2m")


def test_gv08_corrupt_truncated_payload_fails_cleanly():
    path = GOLDEN_DIR / "gv08_corrupt_truncated_payload.wx1"
    assert path.exists()
    with pytest.raises(Exception):
        wx1.open_file(str(path))


def test_gv09_corrupt_unsupported_codec_fails_cleanly():
    path = GOLDEN_DIR / "gv09_corrupt_unsupported_codec.wx1"
    assert path.exists()
    with pytest.raises(Exception):
        wx1.open_file(str(path))


def test_gv10_malicious_path_traversal_fails_cleanly():
    path = GOLDEN_DIR / "gv10_malicious_path_traversal.wx1"
    assert path.exists()
    with pytest.raises(Exception):
        wx1.open_file(str(path))


def test_gv11_malicious_decompression_bomb_fails_cleanly():
    path = GOLDEN_DIR / "gv11_malicious_decompression_bomb.wx1"
    assert path.exists()
    with pytest.raises(Exception):
        wx1.open_file(str(path))
