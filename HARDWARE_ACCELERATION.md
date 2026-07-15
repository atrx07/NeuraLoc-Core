# Hardware Acceleration

## Detection model

Startup builds a capability matrix from independent probes. Every field has a status of `available`, `unavailable`, `unknown`, or `experimental`, plus evidence and probe time. Missing tools never become a false negative for the physical device.

Initial Windows probes include:

- CPU identity, cores, instruction sets, RAM, and disks through native/system APIs.
- NVIDIA GPU, driver, CUDA visibility, utilization, temperature, and VRAM through NVML when bundled, with `nvidia-smi` as a structured fallback.
- Intel graphics and NPU identity through Windows device APIs.
- Vulkan loader/device enumeration.
- OpenVINO device enumeration from the installed NeuraLoc-Core runtime.
- AC/battery and thermal information where Windows exposes reliable APIs.

Hardware identity and runtime readiness are separate. An RTX may be present while the CUDA engine package is missing; an Intel NPU may be present while a model is incompatible.

## Target-machine capability matrix

The Intel Core Ultra 9, RTX 5070, Intel integrated graphics, and Intel AI Boost NPU are expected target components, not hard-coded facts. Detection drives the result.

| Workload | Confirmed initial route | Conditional route | Experimental/unsupported assumption |
| --- | --- | --- | --- |
| GGUF chat | llama.cpp CUDA, Vulkan, CPU | SYCL build if validated | Arbitrary GGUF directly on Intel NPU |
| Image generation | stable-diffusion.cpp CUDA, Vulkan, CPU | OpenVINO converted pipeline | Arbitrary checkpoint directly on NPU |
| Whisper | whisper.cpp CPU and supported GPU builds | OpenVINO model package | Automatic split across RTX and NPU |
| Kokoro TTS | ONNX CPU | OpenVINO CPU/iGPU/NPU if the graph compiles | NPU availability inferred from device name alone |
| Embeddings | CPU or CUDA adapter | OpenVINO on compatible format | Sharing one model across unrelated runtimes |

## Recommendation inputs

Each model catalog entry supplies format, parameter count, quantization, size, context, required projector, compatible engines/devices, and conservative memory coefficients. The hardware snapshot supplies available RAM/VRAM, runtime versions, current allocations, power state, temperature, and measured benchmarks.

### Memory estimate

For GGUF chat:

```text
estimated_ram = file_size + cpu_resident_weights + runtime_overhead + safety_reserve
estimated_vram = gpu_weight_fraction + kv_cache + compute_buffers + display_reserve
kv_cache = layers * context_tokens * bytes_per_token_per_layer
```

The engine adapter supplies exact coefficients when its metadata endpoint supports them. Otherwise the catalog provides conservative family-specific ranges. Vision projector memory is added separately.

### Current CPU implementation

The first implemented fit route is the pinned llama.cpp CPU backend. For each ready GGUF, Rust uses the exact file size as resident weights, caps the estimate to the runtime's current 4,096-token maximum, and calculates:

```text
known-shape KV cache = layers * context * embedding * 4 bytes
unknown-shape KV fallback = max(file_size / 4, 256 MiB)
runtime overhead = max(file_size / 10, 512 MiB)
estimated RAM = file_size + KV cache + runtime overhead
system reserve = max(2 GiB, 10% of installed RAM)
usable current RAM = current available RAM - system reserve
```

The 4-byte KV coefficient conservatively represents fp16 K and V values across the full embedding. Known layer/embedding metadata receives `medium` confidence; the bounded fallback receives `low` confidence. The selector exposes the components and assumptions, groups `Excellent`/`Good` as recommended, keeps `Tight` or unconfirmed choices separate, and disables a successful `Not recommended` estimate. Fit probing is advisory: if a live hardware refresh fails, Chat still opens and marks the model unconfirmed. No CUDA/Vulkan VRAM or projector claim is made until those routes have verified engine packages and allocation data.

For image pipelines, the estimate includes weights, text encoder, VAE, activation peak at requested dimensions/batch, backend workspace, and display reserve. Resolution therefore affects fit recommendations.

### Fit score

Hard incompatibility yields `Not recommended`. Otherwise:

```text
headroom = min(available_ram / estimated_ram, available_vram / estimated_vram)
compatibility = 0..1
benchmark = normalized measured performance or a low-confidence prior
power_fit = profile/device suitability
thermal_fit = throttling risk adjustment

score = 0.45 * headroom
      + 0.25 * compatibility
      + 0.20 * benchmark
      + 0.10 * min(power_fit, thermal_fit)
```

Labels:

- `Excellent`: compatible and at least 30% estimated memory headroom.
- `Good`: compatible and at least 15% headroom.
- `Tight`: compatible but below 15% headroom.
- `Not recommended`: incompatible or estimate exceeds configured safety limits.

The UI displays the estimate, confidence, assumptions, suggested backend, context size, GPU layers, and quantization. A benchmark supersedes only the performance prior, never compatibility rules.

## Target-specific routing policy

On a detected RTX 5070, CUDA is preferred for supported LLM and image workloads because those engines are mature and VRAM bandwidth is valuable. Exact VRAM is always queried; laptop and desktop variants differ.

The Intel NPU is recommended only when OpenVINO enumerates `NPU`, the model package declares a supported OpenVINO format, and a preparation/compile probe succeeds. Good secondary candidates are transcription, embeddings, or TTS, but only after per-model validation.

The Intel iGPU may handle Vulkan or OpenVINO-compatible secondary workloads when shared-memory pressure is acceptable. CPU remains the universal fallback for compatible native engines and handles indexing and checksums outside inference.

## Scheduler profiles

| Profile | Policy |
| --- | --- |
| Maximum performance | prefer fastest measured device; allow higher power and fan use |
| Balanced | preserve memory reserve and avoid concurrent heavy GPU jobs |
| Low power | prefer NPU/iGPU/CPU efficiency where measured and compatible |
| Quiet | cap concurrency and CPU threads; avoid sustained benchmark/load churn |
| Manual | validate the selected device but do not silently substitute it |

Default heavy-workload concurrency is one per constrained device. Secondary jobs run concurrently only if compatibility, estimated memory, thermal state, and configured reserve all pass.

## Confidence and uncertainty

Confirmed capability means the engine/runtime combination is installed and a probe passed. Supported means upstream documentation and catalog metadata agree but no local benchmark exists. Experimental means the route is opt-in and may fail. Unknown is never presented as unavailable.

Open technical uncertainties:

- RTX 5070 laptop VRAM capacity and driver/runtime versions vary by machine.
- NPU telemetry is not consistently exposed across Windows/OEM driver versions.
- OpenVINO GenAI model coverage changes independently of physical NPU support.
- Vulkan llama.cpp performance and stability vary by driver and model.
- Reliable cross-device power comparison may require optional vendor APIs.

These are handled with probe evidence, versioned compatibility data, and explicit UI labels rather than optimistic fallbacks.
