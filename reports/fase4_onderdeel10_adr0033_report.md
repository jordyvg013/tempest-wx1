# WX-1 Fase 4, Onderdeel 10: ADR 0033 — GPU & Multi-Device Integratie, Non-Blocking Overdracht en Bit-Exacte Herstart

**Datum**: 2026-11-02 20:00:00 CET  
**Status**: Geaccepteerd & Geïmplementeerd (ADR 0033)  
**Doel**: Voltooiing van Fase 4, Onderdeel 10 van de WX-1 Roadmap.

---

## 1. Samenvatting & Architectonische Kernwaarden

ADR 0033 formaliseert de koppeling tussen WX-1 en moderne accelerator-gebaseerde numerieke weersvoorspellingsmodellen (NWP) en AI/ML-pipelines (PyTorch, JAX, CuPy, DLPack, multi-GPU clusters):

1. **CPU-First Garantie**:
   * De kernbibliotheek (`wx1-format`, `wx1-registry`, `wx1-ffi`) en de minimale Python-installatie vereisen **nul** GPU-toolchains, CUDA-runtimes of accelerator-drivers.
   * Containerbestanden geproduceerd op GPU/accelerator clusters zijn 100% interoperabel en bit-exact leesbaar op standaard CPU workstations.
2. **Device Array Ingestie (`_to_host_numpy`)**:
   * Directe acceptatie van JAX DeviceArrays, PyTorch Tensors, CuPy ndarrays en DLPack-compatibele structuren zonder dat de gebruiker handmatige conversies hoeft uit te voeren.
   * Normalisatie naar aaneengesloten C-order hostbuffers via gestandaardiseerde protocollen (`__dlpack__`, `__array__`, `detach().cpu().numpy()`).
3. **Non-Blocking Asynchrone Overdracht**:
   * Integratie met `wx1.AsyncWriter` voor continue weersimulaties: GPU-rekentijd wordt niet geblokkeerd tijdens I/O en compressie dankzij dubbel-gebufferde worker queues en afzonderlijke I/O-threads.
4. **Bit-Exact Herstartprofiel (`profile="restart"`)**:
   * Geen enkel kwaliteitsverlies toegestaan: `lossy_error_bound` is strikt verboden en resulteert in een directe `ValueError`.
   * Volledige bewaring van IEEE-754 bitpatronen (`NaN`, signed zeros, subnormals) via `zstd_shuffle` of `lz4_shuffle`.
5. **Multi-Device Domeinpartities & Reconstructie**:
   * Ondersteuning voor multi-GPU / MPI gedistribueerde subdomeinen via `write_restart_checkpoint` en `read_restart_checkpoint` met gestandaardiseerde metadata (`restart:iteration`, `restart:sim_time_seconds`, `restart:prng_state`, `shard_info`).
   * Naadloze reconstructie van het globale voorspellingsdomein uit gedistribueerde shard-bestanden.

---

## 2. Geïmplementeerde Functionaliteit

### 2.1 Python API Uitbreidingen (`python/wx1/__init__.py`)

* **`_to_host_numpy(data)`**:
  Detecteert en extraheert host `numpy.ndarray` veilig uit:
  * Ruwe `numpy.ndarray`;
  * JAX / CuPy / PyTorch via `__array__` / `np.asarray`;
  * PyTorch tensors via `detach().cpu().numpy()` / `__dlpack__`;
  * DLPack capsules via `numpy.from_dlpack(data)` met fallback.
  * Garandeert continue C-stijl geheugenopmaak via `np.ascontiguousarray`.

* **`_place_on_device(arr, to_device)`**:
  Plaatst gedecodeerde host arrays direct terug op het gewenste doelsysteem:
  * `"cpu"` / `None`: Standaard `numpy.ndarray`;
  * `"jax"`: `jax.device_put(arr)`;
  * `"torch"`: `torch.as_tensor(arr)`;
  * `"cupy"`: `cupy.asarray(arr)`.

* **`write_restart_checkpoint(...)` & `read_restart_checkpoint(...)`**:
  * Schrijft bit-exacte checkpoint-bestanden (enkele array of dictionary van prognostische variabelen).
  * Bewaart en valideert model-metadata (`iteration`, `sim_time_seconds`, `prng_state`, `shard_info`).
  * Forceert `profile="restart"` (weigert lossy bounds).

* **Directe `to_device` Routering bij Lezen**:
  * `read_array(path, to_device=...)` en `read_array_slice(path, start, count, to_device=...)` ondersteunen rechtstreekse transfer naar doelplatformen.

---

## 3. Testresultaten & Verificatie

Er is een uitgebreide testsuite toegevoegd in `python/tests/test_gpu_restart_adr0033.py`:

| Test | Doel | Resultaat |
| :--- | :--- | :--- |
| `test_jax_array_write_and_bit_exact_readback` | Schrijven van JAX DeviceArray, direct teruglezen naar JAX device | **GESLAAGD** |
| `test_async_writer_with_jax_and_dlpack_timesteps` | Multi-step simulatie met non-blocking JAX-invoer naar `AsyncWriter` | **GESLAAGD** |
| `test_bit_exact_restart_checkpoint_single_variable` | Checkpoint van enkele prognostische variabele met PRNG & shard metadata | **GESLAAGD** |
| `test_bit_exact_restart_checkpoint_multi_variable` | Compleet prognostisch veld (`u`, `v`, `t`, `p`) opgeslagen en bit-exact hersteld | **GESLAAGD** |
| `test_restart_checkpoint_rejects_lossy_quantization` | Verifieert dat lossy quantizatie strikt geweigerd wordt bij herstartbestanden | **GESLAAGD** |
| `test_simulated_multi_device_sharded_reconstruction` | 4-GPU simulatie met 2D subdomeinen: bit-exacte reconstructie van het globale veld | **GESLAAGD** |
| `test_to_device_cpu_and_unsupported` | Controleert routering naar CPU en foutafhandeling bij ongeldige device-namen | **GESLAAGD** |

### Workspace Teststatus
* **Pytest**: 52 geslaagd, 3 overgeslagen (optionele dependencies zarr/grib2 fixtures), 0 mislukt.
* **Cargo Test**: 163 geslaagd, 0 mislukt, 0 gewaarschuwd.
* **Cargo Clippy**: Geslaagd (`-D warnings`).
* **Cargo Fmt**: Geslaagd (`--check`).
