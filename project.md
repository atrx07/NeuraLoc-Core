# NeuraLoc-Core

NeuraLoc-Core is a privacy-first Windows desktop environment for running local AI workloads through proven native inference engines. The application is an orchestration and user-experience layer; it does not implement model inference itself.

## Canonical naming

- Product, display, and repository name: `NeuraLoc-Core`
- Canonical repository: `https://github.com/atrx07/NeuraLoc-Core`
- Package, executable, and storage stem: `neuraloc-core`
- Windows bundle identifier: `com.neuraloc.core`

The normalized machine identifiers are intentional. Human-facing documentation, application titles, repository links, and release names use `NeuraLoc-Core`.

## Product goals

- Run chat, image, speech-to-text, and text-to-speech workloads locally by default.
- Select backends from measured hardware capabilities and model compatibility.
- Keep idle resource use low and make every loaded model and running engine visible.
- Treat prompts, models, conversations, downloads, and outputs as durable, inspectable assets.
- Offer a polished experience without hiding security-sensitive network or file access.
- Remain modular enough to replace an engine without rewriting the UI or scheduler.

## Supported workload families

| Workload | Initial engine | Formats | Preferred path on target hardware |
| --- | --- | --- | --- |
| LLM chat | llama.cpp | GGUF, optional matching mmproj | CUDA, then Vulkan/CPU |
| Image generation | stable-diffusion.cpp | Supported single-file checkpoints | CUDA, then Vulkan/CPU |
| Speech recognition | whisper.cpp | Whisper GGML models | Compatible GPU path, then CPU |
| Text-to-speech | Kokoro ONNX | Curated ONNX packages | OpenVINO/CPU when verified |
| Intel-optimized inference | OpenVINO GenAI | Explicitly converted packages | NPU/iGPU/CPU by compatibility |

Support is capability-driven. The presence of a device does not imply that every model can use it.

## Product principles

1. **Local unless disclosed.** Chat and inference are offline. Catalog refreshes, model downloads, and optional web search are the only normal network paths.
2. **No hidden orchestration.** Running engines, jobs, devices, memory estimates, ports, and tool permissions are visible.
3. **Structured boundaries.** React calls typed Tauri commands. Only the Rust process manager launches child processes.
4. **Honest recommendations.** Fit and speed are estimates, marked with confidence and supporting measurements.
5. **Recoverable operations.** Downloads, migrations, and jobs are resumable or transactional where practical.
6. **User-owned data.** Models and large outputs remain ordinary files; metadata uses SQLite and can be exported.

## Scope and non-goals

The first release targets Windows 11 and dynamically detects CPU, RAM, NVIDIA GPU/VRAM, Intel graphics/NPU, acceleration runtimes, disk space, and power state. Other operating systems may be added after Windows packaging and lifecycle behavior are stable.

Not in scope for the initial release:

- Training or fine-tuning models.
- Automatic execution of code from model repositories.
- Transparent tensor splitting across unrelated devices.
- Cloud accounts, analytics, or hidden cloud inference.
- Unrestricted shell, filesystem, or LAN access.

## Data locations

Installed builds use the platform application-data directory. Portable mode uses a user-selected `data/` directory beside the executable only after write access is validated.

```text
data/
|-- neuraloc-core.db
|-- models/
|   |-- llm/
|   |-- image/
|   |-- speech/
|   `-- tts/
|-- outputs/
|   |-- images/
|   |-- transcripts/
|   `-- speech/
|-- prompts/
|-- downloads/
|-- cache/
`-- logs/
```

## Success criteria

- The app starts with no inference engines running.
- Hardware and backend readiness are understandable from one screen.
- Every child process is registered, monitored, and stopped on application exit.
- A user can install/import a model without touching a terminal.
- Prompt text and composition are exact, versioned, and previewable.
- The app remains responsive during downloads, model loading, and inference.
- Normal desktop operation opens no listening network socket.

## Delivery definition

Each phase is accepted only when its migrations, commands, UI states, tests, diagnostics simulation, and documentation are complete. Experimental hardware paths remain behind capability checks and explicit labels.
