# WX-1 Verificatierapport: Fysieke NVIDIA GPU & 6 GB VRAM Hardwaretest

**Datum**: 2026-11-02 20:05:00 CET  
**Hardware**: NVIDIA GeForce RTX 3060 Laptop GPU (6144 MiB VRAM)  
**Host / Omgeving**: Linux WSL2 (Kernel 6.18.54.1), CUDA UMD 13.4, Driver 615.78.05  
**Python Framework**: Python 3.12.16 | JAX 0.12.0.1 met CUDA Backend (`[CudaDevice(id=0)]`) | NumPy 2.2.6  
**Status**: **100% GESLAAGD & BIT-EXACT BEVESTIGD OP HARDWARE**

---

## 1. Hardware- & VRAM-Specificaties (`nvidia-smi`)

Directe query via `nvidia-smi` op het actieve systeem:
```
+-----------------------------------------------------------------------------------------+
| NVIDIA-SMI 615.78.05              KMD Version: 617.42        CUDA UMD Version: 13.4     |
+-----------------------------------------+------------------------+----------------------+
| GPU  Name                 Persistence-M | Bus-Id          Disp.A | Volatile Uncorr. ECC |
| Fan  Temp   Perf          Pwr:Usage/Cap |           Memory-Usage | GPU-Util  Compute M. |
|=========================================+========================+======================|
|   0  NVIDIA GeForce RTX 3060 ...    On  |   00000000:01:00.0 Off |                  N/A |
| N/A   60C    P0             23W /   95W |       0MiB /   6144MiB |      0%      Default |
+-----------------------------------------+------------------------+----------------------+
```
* **GPU Model**: NVIDIA GeForce RTX 3060 Laptop GPU
* **Totale VRAM**: 6144 MiB (6.0 GB)
* **Beschikbaar voor model-arrays**: > 5.8 GB vrij geheugen

---

## 2. Geteste Componenten & Resultaten

### 2.1 Hardware Detectie & Geheugenallocatie (Dynamic BFC Allocator)
* Standaard probeert JAX/XLA vaak 90% van de VRAM in één keer te claimen (wat bij 6 GB laptop-GPU's kan conflicteren met Windows Desktop Window Manager buffers).
* Met `XLA_PYTHON_CLIENT_PREALLOCATE=false` alloceert WX-1 dynamisch uitsluitend het daadwerkelijk benodigde GPU-geheugen. Hierdoor functioneert WX-1 vlekkeloos en zonder geheugendruk op 6 GB VRAM.

### 2.2 Bit-Exact Herstarten vanaf GPU (`write_restart_checkpoint` & `read_restart_checkpoint`)
* **Testopzet**:
  * 3D prognostische atmosferische velden (`u`, `v`, `temperature`, `specific_humidity`) direct gegenereerd op `CudaDevice(id=0)`.
  * Opslag via `wx1.write_restart_checkpoint` direct vanuit het GPU-geheugen via zero-copy DLPack / host memory staging.
  * Teruglezen via `wx1.read_restart_checkpoint(..., to_device="jax")`.
* **Resultaat**:
  * De data wordt direct teruggeplaatst op `CudaDevice(id=0)`.
  * **Absolute afwijking op de GPU**: `diff = 0.0` (100% bit-exact, exact identieke IEEE-754 binaire representatie).

### 2.3 Non-Blocking Simulatiestroom (`AsyncWriter` met GPU-tijdstappen)
* **Testopzet**:
  * Een weersimulatie berekent 10 opeenvolgende tijdstappen direct op de RTX 3060 CUDA cores.
  * Elke tijdstap wordt direct via `writer.submit_step(device_tensor)` aan de asynchrone I/O worker overgedragen.
* **Resultaat**:
  * De GPU hoeft niet te wachten op schijf-I/O of Zstandard-compressie.
  * Alle generaties worden naadloos gecommitteerd en bit-exact teruggelezen naar de GPU.

### 2.4 Zware NWP Belastingstest (536.8 MB prognostisch volume op GPU)
* **Testopzet**:
  * 8 atmosferische 3D-variabelen (`u`, `v`, `w`, `temperature`, `specific_humidity`, `pressure`, `cloud_water`, `ozone`) met resolutie `(64, 512, 512)`.
  * Totaal GPU-geheugengebruik: **536.8 MB actieve data** in VRAM.
* **Gemeten prestaties op de RTX 3060 (6 GB)**:
  * VRAM-gebruik tijdens test: **627 MiB** (ruim binnen de 6144 MiB limiet; > 5.3 GB vrije reserve).
  * Checkpoint schrijfsnelheid: **130.9 MB/s** (inclusief Zstd compressie & checksumming).
  * Directe read-back naar GPU VRAM: **239.4 MB/s**.
  * Bit-exact verificatie: **Geen enkele bit afwijking (`diff = 0.0`) op alle 8 variabelen**.

---

## 3. Testsuite Uitvoering

In `python/tests/test_nvidia_cuda_hardware.py` zijn 4 formele hardware-tests vastgelegd:

```
python/tests/test_nvidia_cuda_hardware.py::test_nvidia_gpu_hardware_detection PASSED
python/tests/test_nvidia_cuda_hardware.py::test_nvidia_gpu_restart_checkpoint_bit_exact PASSED
python/tests/test_nvidia_cuda_hardware.py::test_nvidia_gpu_async_writer_streaming PASSED
python/tests/test_nvidia_cuda_hardware.py::test_nvidia_gpu_vram_load_within_6gb PASSED
```

**Totale teststatus codebase**:
* **Pytest**: 56 geslaagd, 3 overgeslagen (externe format fixtures), 0 mislukt.
* **Cargo Test**: 163 geslaagd, 0 mislukt.
* **Cargo Clippy & Fmt**: 0 waarschuwingen, 0 fouten.
