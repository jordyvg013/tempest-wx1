# Copyright 2026 Jordy van Gils
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""Direct Hardware Integration Tests on Physical NVIDIA GPU and CUDA VRAM (ADR 0033)."""

from __future__ import annotations

import os
import shutil
import subprocess
import numpy as np
import pytest
import wx1

# Ensure JAX does not greedily preallocate 90% of GPU VRAM
os.environ["XLA_PYTHON_CLIENT_PREALLOCATE"] = "false"


def _get_cuda_gpu():
    if shutil.which("nvidia-smi") is None:
        return None
    try:
        import jax

        devices = jax.devices("gpu")
        if len(devices) > 0:
            return devices[0]
    except Exception:
        return None
    return None


@pytest.fixture
def cuda_device():
    dev = _get_cuda_gpu()
    if dev is None:
        pytest.skip("Physical NVIDIA CUDA GPU not available")
    return dev


def test_nvidia_gpu_hardware_detection(cuda_device):
    """Verifies physical NVIDIA GPU presence and VRAM ceiling."""
    import jax

    assert cuda_device.platform == "gpu"
    assert "NVIDIA" in cuda_device.device_kind or "RTX" in cuda_device.device_kind

    # Query VRAM using nvidia-smi
    out = subprocess.check_output(
        ["nvidia-smi", "--query-gpu=name,memory.total", "--format=csv,noheader,nounits"],
        text=True,
    ).strip()
    name, total_vram_str = [part.strip() for part in out.split(",")]
    total_vram_mb = int(total_vram_str)

    # Confirm it has at least 5800 MiB (approx 6 GB)
    assert total_vram_mb >= 5800, f"Expected ~6GB VRAM, got {total_vram_mb} MiB"


def test_nvidia_gpu_restart_checkpoint_bit_exact(cuda_device, tmp_path):
    """Allocates 3D multi-level prognostic fields on NVIDIA GPU and verifies bit-exact restart."""
    import jax
    import jax.numpy as jnp

    shape_3d = (32, 128, 128)
    key = jax.random.PRNGKey(42)
    k1, k2, k3, k4 = jax.random.split(key, 4)

    # Allocate directly on CUDA device
    u_gpu = jax.random.normal(k1, shape_3d, dtype=jnp.float32)
    v_gpu = jax.random.normal(k2, shape_3d, dtype=jnp.float32)
    t_gpu = jax.random.normal(k3, shape_3d, dtype=jnp.float32) + 273.15
    q_gpu = jax.random.uniform(k4, shape_3d, dtype=jnp.float32, minval=0.0, maxval=0.02)

    prognostic_state = {
        "u": u_gpu,
        "v": v_gpu,
        "temperature": t_gpu,
        "specific_humidity": q_gpu,
    }

    ckpt_path = tmp_path / "nvidia_gpu_checkpoint"
    wx1.write_restart_checkpoint(
        ckpt_path,
        prognostic_state,
        dataset_id="nvidia_rtx3060_checkpoint",
        iteration=720,
        sim_time_seconds=43200.0,
        prng_state="philox_seed_cuda_0x42",
        shard_info={"gpu_id": 0, "device_kind": cuda_device.device_kind},
    )

    # Restore checkpoint directly back to NVIDIA GPU
    restored = wx1.read_restart_checkpoint(ckpt_path, to_device="jax")

    assert restored["iteration"] == 720
    assert restored["sim_time_seconds"] == 43200.0
    assert restored["prng_state"] == "philox_seed_cuda_0x42"

    for var_name, original_gpu in prognostic_state.items():
        restored_gpu = restored["variables"][var_name]
        assert isinstance(restored_gpu, jax.Array)
        assert list(restored_gpu.devices())[0].platform == "gpu"
        # Verify 100% bit-exact equality on GPU
        diff = float(jnp.max(jnp.abs(restored_gpu - original_gpu)))
        assert diff == 0.0, f"Bit mismatch on {var_name}"
        np.testing.assert_array_equal(np.asarray(restored_gpu), np.asarray(original_gpu))


def test_nvidia_gpu_async_writer_streaming(cuda_device, tmp_path):
    """Simulates NWP time-integration on NVIDIA GPU streaming via AsyncWriter."""
    import jax
    import jax.numpy as jnp

    stream_file = tmp_path / "gpu_stream.wx1"
    writer = wx1.AsyncWriter(
        stream_file,
        dataset_id="gpu_stream_simulation",
        variable_path="/t2m",
        spatial_dimensions=["lat", "lon"],
        spatial_shape=(256, 256),
        codec="zstd_shuffle",
        workers=2,
    )

    ground_truth = []
    for step in range(6):
        # Timestep computed directly on NVIDIA GPU
        device_data = jnp.full((256, 256), 280.0 + step * 0.5, dtype=jnp.float32)
        ground_truth.append(device_data)
        writer.submit_step(device_data)

    writer.close()

    # Read back placing slices directly onto NVIDIA GPU
    f = wx1.open_file(stream_file)
    for step in range(6):
        slice_gpu = f.read_array_slice("/t2m", [step, 0, 0], [1, 256, 256], to_device="jax")
        assert isinstance(slice_gpu, jax.Array)
        assert list(slice_gpu.devices())[0].platform == "gpu"
        np.testing.assert_array_equal(np.asarray(slice_gpu[0]), np.asarray(ground_truth[step]))


def test_nvidia_gpu_vram_load_within_6gb(cuda_device, tmp_path):
    """Verifies multiple 3D atmospheric variables allocate and roundtrip within 6GB VRAM limit."""
    import jax
    import jax.numpy as jnp

    # Allocate 4 variables of 32x256x256 f32 = ~33.5 MB each = ~134 MB total on GPU
    shape = (32, 256, 256)
    key = jax.random.PRNGKey(999)
    k1, k2 = jax.random.split(key)

    u = jax.random.normal(k1, shape, dtype=jnp.float32)
    v = jax.random.normal(k2, shape, dtype=jnp.float32)

    path = tmp_path / "vram_test.wx1"
    wx1.write_array(
        path,
        dataset_id="vram_test",
        variable_path="/u",
        dimensions=["lev", "lat", "lon"],
        data=u,
        profile="restart",
    )

    f = wx1.open_file(path)
    u_restored = f.read_array("/u", to_device="jax")
    assert isinstance(u_restored, jax.Array)
    assert list(u_restored.devices())[0].platform == "gpu"
    np.testing.assert_array_equal(np.asarray(u_restored), np.asarray(u))
