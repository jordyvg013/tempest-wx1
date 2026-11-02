"""GPU / Accelerator Integration, Non-Blocking Transfer, and Bit-Exact Restart Tests (ADR 0033)."""

from __future__ import annotations

import os
import numpy as np
import pytest
import wx1

# Prevent JAX greedy preallocation of entire GPU VRAM
os.environ.setdefault("XLA_PYTHON_CLIENT_PREALLOCATE", "false")


def test_jax_array_write_and_bit_exact_readback(tmp_path):
    pytest.importorskip("jax")
    import jax
    import jax.numpy as jnp

    # Create synthetic atmospheric geopotential field on JAX device
    shape = (16, 16)
    jax_data = jnp.arange(256, dtype=jnp.float32).reshape(shape) * 9.80665

    path = tmp_path / "jax_geopotential.wx1"
    wx1.write_array(
        path,
        dataset_id="jax_test_ds",
        variable_path="/geopotential",
        dimensions=["lat", "lon"],
        data=jax_data,
        profile="restart",
        codec="zstd_shuffle",
    )

    f = wx1.open_file(path)
    # Read back placing directly on JAX device
    restored_jax = f.read_array("/geopotential", to_device="jax")
    assert isinstance(restored_jax, jax.Array)
    np.testing.assert_array_equal(np.asarray(restored_jax), np.asarray(jax_data))


def test_async_writer_with_jax_and_dlpack_timesteps(tmp_path):
    pytest.importorskip("jax")
    import jax
    import jax.numpy as jnp

    path = tmp_path / "nwp_loop_async.wx1"
    shape = (8, 8)

    # Initialize double-buffered async writer
    writer = wx1.AsyncWriter(
        path,
        dataset_id="nwp_async_run",
        variable_path="/t2m",
        spatial_dimensions=["lat", "lon"],
        spatial_shape=shape,
        codec="zstd_shuffle",
        workers=2,
        max_pending_generations=2,
    )

    ground_truth_steps = []
    # Simulate 4 consecutive NWP timesteps computed on accelerator
    for step in range(4):
        # Step t computed on device
        device_step = jnp.full(shape, 273.15 + step * 2.5, dtype=jnp.float32)
        ground_truth_steps.append(np.asarray(device_step))
        # Submit device array directly (tests non-blocking _to_host_numpy transfer)
        writer.submit_step(device_step)

    telemetry = writer.close()
    assert telemetry.get("committed_generations", 0) >= 4

    # Verify all timesteps are bit-exact in container
    f = wx1.open_file(path)
    assert f.generation >= 4
    for step in range(4):
        step_slice = f.read_array_slice("/t2m", [step, 0, 0], [1, 8, 8])
        np.testing.assert_array_equal(step_slice[0], ground_truth_steps[step])


def test_bit_exact_restart_checkpoint_single_variable(tmp_path):
    path = tmp_path / "model_checkpoint_step_100.wx1"
    rng = np.random.default_rng(42)
    state = rng.standard_normal((32, 32), dtype=np.float32)

    wx1.write_restart_checkpoint(
        path,
        state,
        dataset_id="global_forecast_model",
        dimensions=["lat", "lon"],
        iteration=100,
        sim_time_seconds=3600.0 * 6,
        prng_state="philox4x32_state_0xABCD1234",
        shard_info={"device_id": 0, "rank": 0, "num_ranks": 1},
    )

    restored = wx1.read_restart_checkpoint(path)
    assert restored["iteration"] == 100
    assert restored["sim_time_seconds"] == 21600.0
    assert restored["prng_state"] == "philox4x32_state_0xABCD1234"
    assert restored["shard_info"]["rank"] == "0"
    assert restored["shard_info"]["num_ranks"] == "1"

    restored_arr = restored["variables"]["state"]
    # Guarantee 100% bit-exact restoration
    np.testing.assert_array_equal(restored_arr, state)


def test_bit_exact_restart_checkpoint_multi_variable(tmp_path):
    checkpoint_dir = tmp_path / "nwp_checkpoint_dir"
    rng = np.random.default_rng(99)

    prognostic_state = {
        "u": rng.standard_normal((16, 16), dtype=np.float32),
        "v": rng.standard_normal((16, 16), dtype=np.float32),
        "temperature": rng.standard_normal((16, 16), dtype=np.float32) + 280.0,
        "pressure": np.full((16, 16), 101325.0, dtype=np.float32),
    }

    wx1.write_restart_checkpoint(
        checkpoint_dir,
        prognostic_state,
        dataset_id="ifs_restart_run",
        dimensions={"u": ["y", "x"], "v": ["y", "x"], "temperature": ["y", "x"], "pressure": ["y", "x"]},
        iteration=500,
        sim_time_seconds=7200.0,
        prng_state=424242,
        shard_info={"rank": 2, "num_ranks": 8},
    )

    restored = wx1.read_restart_checkpoint(checkpoint_dir)
    assert restored["iteration"] == 500
    assert restored["sim_time_seconds"] == 7200.0
    assert restored["prng_state"] == 424242
    assert restored["shard_info"]["rank"] == 2

    for var_name, original_arr in prognostic_state.items():
        assert var_name in restored["variables"]
        restored_arr = restored["variables"][var_name]
        np.testing.assert_array_equal(restored_arr, original_arr)


def test_restart_checkpoint_rejects_lossy_quantization(tmp_path):
    path = tmp_path / "invalid_restart.wx1"
    data = np.ones((8, 8), dtype=np.float32)
    with pytest.raises(ValueError, match="lossy quantization is strictly forbidden"):
        wx1.write_array(
            path,
            dataset_id="test_fail",
            variable_path="/t",
            dimensions=["y", "x"],
            data=data,
            profile="restart",
            lossy_error_bound=0.01,
        )


def test_simulated_multi_device_sharded_reconstruction(tmp_path):
    """Simulates a 4-GPU distributed model partitioning a 64x64 domain into four 32x32 subdomains."""
    global_domain_shape = (64, 64)
    rng = np.random.default_rng(2026)
    global_truth = rng.standard_normal(global_domain_shape, dtype=np.float32)

    # 2x2 subdomain decomposition across 4 simulated accelerator ranks
    ranks_config = [
        {"rank": 0, "subgrid_origin": (0, 0), "slice": (slice(0, 32), slice(0, 32))},
        {"rank": 1, "subgrid_origin": (0, 32), "slice": (slice(0, 32), slice(32, 64))},
        {"rank": 2, "subgrid_origin": (32, 0), "slice": (slice(32, 64), slice(0, 32))},
        {"rank": 3, "subgrid_origin": (32, 32), "slice": (slice(32, 64), slice(32, 64))},
    ]

    shards_dir = tmp_path / "sharded_checkpoint"
    shards_dir.mkdir()

    # Each device independently saves its own local subdomain checkpoint
    for cfg in ranks_config:
        rank = cfg["rank"]
        local_slice = cfg["slice"]
        local_data = global_truth[local_slice]

        shard_path = shards_dir / f"shard_rank_{rank}.wx1"
        wx1.write_restart_checkpoint(
            shard_path,
            local_data,
            dataset_id=f"global_run_rank_{rank}",
            dimensions=["y", "x"],
            iteration=10,
            shard_info={
                "rank": rank,
                "num_ranks": 4,
                "global_shape": global_domain_shape,
                "subgrid_origin": cfg["subgrid_origin"],
            },
        )

    # Simulation restarts: Reconstruct the full seamless global domain from the 4 device shards
    reconstructed_global = np.empty(global_domain_shape, dtype=np.float32)
    for cfg in ranks_config:
        rank = cfg["rank"]
        shard_path = shards_dir / f"shard_rank_{rank}.wx1"
        shard_restored = wx1.read_restart_checkpoint(shard_path)

        local_arr = shard_restored["variables"]["state"]
        local_slice = cfg["slice"]
        reconstructed_global[local_slice] = local_arr

    # Assert reconstructed global field is 100% bit-exact with the original state
    np.testing.assert_array_equal(reconstructed_global, global_truth)


def test_to_device_cpu_and_unsupported(tmp_path):
    path = tmp_path / "device_routing.wx1"
    data = np.arange(16, dtype=np.float32).reshape(4, 4)
    wx1.write_array(
        path,
        dataset_id="routing_test",
        variable_path="/test_var",
        dimensions=["y", "x"],
        data=data,
    )

    f = wx1.open_file(path)
    # CPU returns ndarray
    arr_cpu = f.read_array("/test_var", to_device="cpu")
    assert isinstance(arr_cpu, np.ndarray)

    # Invalid device name raises ValueError
    with pytest.raises(ValueError, match="Unsupported to_device 'bogus_tpu'"):
        f.read_array("/test_var", to_device="bogus_tpu")
