# Roadmap

## Phase 1: Foundation

Status: in progress.

- Tauri v2 shell and React/TypeScript application shell.
- Modular Rust services, typed errors, IPC capability file, and event envelope.
- SQLite WAL database with initial migration.
- Settings and theme persistence.
- Hardware snapshot and honest capability matrix.
- Owned-process manager and lifecycle types.
- Basic navigation, hardware view, settings, logs, and first-run state.
- Browser-safe demo bridge for frontend development.

Exit gate: frontend build/tests pass; Rust format, tests, and clippy pass on a Rust-equipped Windows host; no child process survives normal app exit.

## Phase 2: LLM Chat

Status: in progress. Local GGUF indexing, basic metadata inspection, owned-process hardening, verified Windows x64 CPU runtime installation, and the concrete llama.cpp load/health/stop/log lifecycle are implemented. Real-model integration validation, the Chat selector, and streaming chat remain pending.

- llama.cpp adapter, engine package installer, GGUF import/indexing.
- Streaming chat, cancellation, conversation branches, exports.
- Chat-template registry and context budget strategies.
- Model/session controls, memory estimates, and bounded OOM fallback.

## Phase 3: Prompt Library

- Markdown/text import, drag/drop, YAML parsing, immutable versions.
- Search, tags, collections, favorites, editor, export.
- Adjacent model/prompt selectors and compiled prompt inspector.
- Conversation version binding and prompt-change flows.

## Phase 4: Model Catalog and Downloads

- Signed catalog, hardware fit scoring, install-from-chat flow.
- Resumable verified downloads with pause, retry, and cleanup.
- Local import validation and recommendation explanations.

## Phase 5: Images

- stable-diffusion.cpp adapter and CUDA/Vulkan/CPU routing.
- Generation controls, cancellation, metadata, gallery, reuse.
- Explicitly compatible OpenVINO pipeline support.

## Phase 6: Speech and TTS

- whisper.cpp recording/import/conversion and transcript history.
- Kokoro ONNX voices, playback, PCM WAV export, and history.
- Send transcript to Chat and read assistant response aloud.

## Phase 7: Hardware Optimization

- Per-model backend benchmarks and recommendation feedback.
- OpenVINO device packages and validated NPU/iGPU routes.
- Live telemetry, scheduler profiles, thermal/power policies.

## Phase 8: Optional Surfaces

- Vision GGUF and projector validation.
- Web search with untrusted-content isolation.
- Permission-based project tools.
- Optional authenticated OpenAI-compatible loopback API.

## Known uncertainties

- Intel NPU model/runtime coverage is experimental until local OpenVINO compile probes pass.
- Vendor telemetry availability varies by driver and laptop firmware.
- Some upstream engines require loopback HTTP internally; adapters must contain and authenticate that transport where possible.
- Signed catalog key rotation and offline revocation design must be completed before public distribution.
- Tauri updater and Windows code-signing strategy must be chosen before beta packaging.
