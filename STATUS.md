# NeuraLoc-Core Status

Status date: 2026-07-14

Project root: `C:\Users\atrx07\atrx\NeuraLoc-Core`

Current version: `0.1.0`

## Summary

NeuraLoc-Core is an executable local-chat prototype built with Tauri 2, React, TypeScript, and Rust. It starts, creates its application data directories and SQLite database, exposes typed commands for app state, hardware, settings, local models, engine packages, llama.cpp runtime control, and bounded chat generation, and renders a polished desktop shell with functional Chat, Hardware, Settings, and Model Manager views. The pinned Windows x64 CPU llama.cpp package can be installed and verified, and a ready indexed GGUF can be selected from Chat, launched, health-checked, streamed, cancelled, stopped, and inspected without exposing the internal server or session token to the renderer.

This checkpoint is usable for ephemeral local chat, but it is not yet a complete local inference product. A local opt-in integration test loaded the user's Qwen3 4B Q4_K_M GGUF, authenticated the pinned `b9986` server, streamed a response with usage, cancelled a second active request, stopped the server, and confirmed zero owned child processes on 2026-07-14. The model file remains external and is not stored in the repository. Prompt import/selection, conversation persistence, context management, model-catalog download, image, speech, TTS, gallery, and the dedicated Logs workspace remain unfinished.

## Implemented Functionality

### Desktop and frontend

- Tauri v2 desktop application with one resizable window, 1280 x 820 default size and 900 x 620 minimum size.
- React 18 renderer, strict TypeScript, Vite 6 build, Zustand UI state, and Lucide icons.
- Responsive application shell with collapsible navigation for Chat, Images, Speech, Text to Speech, Models, Prompts, Gallery, Downloads, Hardware, Logs, and Settings.
- Dark, light, and system theme handling. The chosen theme is persisted by the Rust settings service when running in Tauri.
- Functional Hardware view with refresh, CPU/RAM summary, detected accelerators, capability evidence, telemetry fields, and warnings.
- Functional Settings view for theme, performance profile, model retention, idle timeout, internet access, web search, and local API state.
- Functional Model Manager with native GGUF file import, recursive folder scanning, cancellation/progress, search, metadata/status rows, reverify, metadata-only removal, llama.cpp package controls, per-model load/stop controls, runtime health, lifecycle state, and retained-log inspection.
- Functional Chat model selector backed by the persisted model library, grouped ready/unavailable choices, last-used preference, runtime reuse/load/switch/unload controls, visible lifecycle state, and a composer gate tied to the selected ready session.
- Ephemeral Chat messages with Rust-owned streaming token batches, stop generation, terminal/error states, plain-text rendering, and prompt/output token plus tokens-per-second usage when llama.cpp reports it. The mounted Chat workspace retains its in-memory messages and generation listeners while navigating elsewhere in the app. The composer remains pinned while the message viewport scrolls independently, and streaming follows the latest token only while the user stays near the bottom. Messages currently live only in renderer memory and do not survive an application restart.
- Catalog and Downloads tabs remain visibly disabled until the verified catalog checkpoint.
- Browser-only demo bridge for UI development when Tauri IPC is unavailable. Demo settings persist only for the current page session, hardware values are representative, and native model imports are unavailable.
- Typed frontend domain interfaces for app snapshots, settings, hardware, local models, GGUF metadata, engine packages, runtime lifecycle/health/logs, chat requests/results/usage/events, scan/engine events, navigation, and IPC errors.
- Shared adaptive binary-byte and model-metadata formatters with frontend unit tests.

### Rust application core

- Tauri startup creates workload-specific `models/{llm,image,speech,tts}` and `outputs/{images,transcripts,speech}` directories plus `prompts`, `downloads`, `cache`, and `logs`.
- SQLite database creation using bundled SQLite, WAL journal mode, foreign keys, and a five-second busy timeout.
- Ordered transactional migration runner with a migration ledger, idempotency coverage, and an explicit version-1 upgrade test.
- Additive migration 2 extends model records with verification state/error, bounded GGUF metadata JSON, modification time, and stable file identity.
- Additive migration 3 adds engine-package identity, route, install path, archive checksum, installed-file inventory, state, source, errors, and install/verification timestamps.
- Thread-safe `AppState` containing `Database`, `EnginePackageService`, `EngineRuntimeService`, `EventEmitter`, `HardwareService`, `ModelService`, `ProcessManager`, and `SettingsService` handles.
- Stable application and IPC error types with machine-readable error codes and user-facing suggestions.
- Model repository/service and typed commands for list, import, recursive scan, cancellation, reverify, and record removal.
- Engine-package repository/service and typed commands for status, online install, offline import, reverify, and uninstall.
- Bundled manifest 1 pins llama.cpp `b9986` Windows x64 CPU to its official HTTPS asset, exact 18,245,837-byte size, SHA-256, route, architecture, and expected runtime files.
- Online package installation is gated by `internetAccess`, limits redirects to approved GitHub HTTPS hosts, streams to `.partial`, enforces exact size/SHA-256, and removes the partial after success or failure.
- ZIP installation rejects traversal, links/reparse points, Windows device/alternate-stream names, duplicate paths, excessive entries/files/sizes, missing expected files, and untracked installed files; promotion uses an internal staging directory and atomic rename.
- Installation records a SHA-256 inventory for every extracted file. Reverify rejects missing, changed, linked, or added files, and startup reconciles interrupted or missing installations.
- Concrete llama.cpp CPU adapter and runtime service that reverify the selected model and complete package inventory, run a terminating `--version --help` build probe, allow only bounded context/thread options, and launch only the canonical packaged executable.
- The adapter reserves loopback port `0`, binds only `127.0.0.1`, passes a random API key through the cleared child environment, polls `/health`, and requires authenticated `/props` to report the expected canonical model path and `b9986` build before declaring ready.
- Runtime commands expose status, explicit health, start, stop, and bounded retained logs. Package uninstall is rejected while startup or a live session owns the runtime.
- Model-load cancellation can stop the owned process while readiness polling is in flight. Stop uses a short adapter grace interval before the process manager force-stops only its tracked child.
- `ChatEngine` posts bounded OpenAI-compatible message payloads to the Rust-internal authenticated `/v1/chat/completions` endpoint, disables model thinking through bounded chat-template kwargs so Qwen3 returns visible answer text, parses bounded UTF-8 SSE data lines, batches token events every 16 ms or 256 bytes, records usage, rejects truncated or visible-text-empty streams, and supports one active cancellable generation.
- Typed `start_chat_generation` and `cancel_chat_generation` commands validate job/session/message identity, role/content/count/output limits, require the matching ready model session, and emit sequenced token, usage, and terminal-state events.
- Central model path validation requires absolute canonical regular files/folders, rejects device/traversal/symlink/reparse paths, limits imports to `.gguf`, and never deletes a model file.
- Cheap verification checks GGUF magic/version, bounded counts and metadata sizes, modification time, path identity, and hard-link duplicates without loading tensors or complete model files into memory.
- Bounded GGUF inspection extracts architecture, model name, file type/quantization, parameter count, context length, embedding length, layer count, and chat-template presence while retaining a small diagnostic metadata preview.
- Recursive scans skip links, enforce depth/file limits, accept cancellation, and emit per-scan monotonically sequenced progress envelopes consumed by the frontend.
- Settings service with defaults, persisted JSON storage, patch semantics, and validation:
  - idle unload timeout must be 1 through 240 minutes;
  - API port must be 1024 or higher;
  - disabling internet also disables web search;
  - disabling the API also disables LAN access.
- Native hardware snapshot through `sysinfo` for CPU identity/core counts/utilization and total/available RAM.
- NVIDIA probe through `nvidia-smi.exe` for GPU name, VRAM, utilization, and temperature.
- Windows compute-accelerator probe through `pnputil.exe` for an Intel NPU or AI Boost device.
- Evidence-based capability states for CPU fallback, CUDA LLM/image routes, Vulkan LLM route, and OpenVINO NPU route.
- Hardware snapshot caching plus an explicit refresh operation.
- Central owned-process manager that:
  - requires canonical absolute executable paths, starts without a shell, and bounds argument/environment input;
  - clears the inherited environment, restores a minimal platform baseline, and applies only adapter-provided entries;
  - nulls stdin and captures stdout/stderr with 16 KiB line and 2,000-line per-process bounds;
  - redacts common token, API-key, and authorization markers before retaining logs;
  - assigns UUID ownership IDs and records PID/start time;
  - supervises natural exits, records exit code/end time, and distinguishes stopped, crashed, and error states;
  - provides lifecycle updates, summaries, active counts, and retained logs internally;
  - runs terminating native build/capability probes through the same owned-process boundary;
  - allows an adapter grace period before force-stopping only the owned child;
  - limits native probes to four seconds;
  - stops all registered processes on normal application exit.
- Engine lifecycle enum and shared `InferenceEngine`/`ChatEngine` traits with concrete llama.cpp lifecycle, streaming generation, usage, and cancellation implementations.
- Scheduler domain types for job kinds/states and a resource policy that labels memory fit as excellent, good, tight, or not recommended.
- Full generated desktop icon bundle, including Windows ICO/PNG, macOS ICNS, iOS, and Android assets.
- Tauri capability file grants `core:default` plus only native dialog open/confirm permissions; no general filesystem, shell, or network plugin is exposed to the renderer.

## Current Directory Structure

Generated dependency/build directories (`node_modules`, `dist`, and `src-tauri/target`) are omitted.

```text
NeuraLoc-Core/
|-- .gitignore
|-- README.md
|-- STATUS.md
|-- NEXT_STEPS.md
|-- project.md
|-- ARCHITECTURE.md
|-- DEVELOPMENT.md
|-- HARDWARE_ACCELERATION.md
|-- MODEL_CATALOG.md
|-- PROMPT_SYSTEM.md
|-- ROADMAP.md
|-- SECURITY.md
|-- package.json
|-- package-lock.json
|-- vite.config.ts
|-- tsconfig.json
|-- tsconfig.app.json
|-- tsconfig.node.json
|-- index.html
|-- src/
|   |-- main.tsx
|   |-- app/
|   |   |-- App.tsx
|   |   `-- styles.css
|   |-- components/
|   |   `-- Sidebar.tsx
|   |-- features/
|   |   |-- chat/
|   |   |   |-- ChatWorkspace.tsx
|   |   |   |-- model-selection.ts
|   |   |   `-- model-selection.test.ts
|   |   |-- hardware/HardwareView.tsx
|   |   |-- models/
|   |   |   |-- ModelManagerView.tsx
|   |   |   |-- model-format.ts
|   |   |   `-- model-format.test.ts
|   |   |-- settings/SettingsView.tsx
|   |   `-- workspaces/WorkspaceView.tsx
|   |-- services/bridge.ts
|   |-- stores/app-store.ts
|   |-- types/domain.ts
|   `-- utils/
|       |-- format.ts
|       `-- format.test.ts
`-- src-tauri/
    |-- Cargo.toml
    |-- Cargo.lock
    |-- build.rs
    |-- tauri.conf.json
    |-- capabilities/default.json
    |-- manifests/llama-cpp-b9986-windows-x86_64-cpu.json
    |-- migrations/
    |   |-- 0001_foundation.sql
    |   |-- 0002_model_library.sql
    |   `-- 0003_engine_packages.sql
    |-- icons/                 # generated desktop/mobile icon bundle
    |-- gen/schemas/           # generated Tauri capability schemas
    `-- src/
        |-- main.rs
        |-- lib.rs
        |-- app_state.rs
        |-- errors.rs
        |-- events.rs
        |-- commands/
        |   |-- app_commands.rs
        |   |-- chat_commands.rs
        |   |-- engine_commands.rs
        |   |-- engine_package_commands.rs
        |   |-- hardware_commands.rs
        |   |-- model_commands.rs
        |   |-- settings_commands.rs
        |   `-- mod.rs
        |-- engines/
        |   |-- llama_cpp.rs
        |   |-- service.rs
        |   |-- traits.rs
        |   `-- mod.rs
        |-- engine_packages/
        |   |-- repository.rs
        |   |-- service.rs
        |   |-- types.rs
        |   `-- mod.rs
        |-- hardware/
        |   |-- detector.rs
        |   |-- types.rs
        |   `-- mod.rs
        |-- models/
        |   |-- gguf.rs
        |   |-- path_grants.rs
        |   |-- repository.rs
        |   |-- service.rs
        |   |-- types.rs
        |   `-- mod.rs
        |-- processes/
        |   |-- manager.rs
        |   |-- lifecycle.rs
        |   `-- mod.rs
        |-- scheduler/
        |   |-- job.rs
        |   |-- resource_policy.rs
        |   `-- mod.rs
        |-- settings/mod.rs
        `-- storage/
            |-- database.rs
            |-- migrations.rs
            `-- mod.rs
```

## Architectural Decisions

1. **Tauri IPC is the renderer boundary.** React calls a typed bridge; it does not receive raw process, database, shell, or unrestricted filesystem access.
2. **Rust owns native orchestration.** The central `ProcessManager` is the only child-process entry point used by the concrete llama.cpp adapter.
3. **Inference is delegated.** NeuraLoc-Core is an orchestration and UX layer. Proven native engines such as llama.cpp will perform inference.
4. **Local and private by default.** Network-related settings default off. The desktop shell opens no application API port, and the CSP restricts content to packaged/Tauri resources.
5. **Large assets remain files.** Models, outputs, prompts, downloads, cache, and logs live in the application data directory. SQLite stores metadata and relationships.
6. **Schema changes are additive migrations.** Applied migrations are recorded and each migration runs in a transaction. Existing migration files should not be edited after release; add `0002_*` and later files.
7. **Capability claims require evidence.** Hardware support uses available/unknown/experimental states and does not equate device presence with backend/model compatibility.
8. **Frontend browser mode is a demo adapter.** It is useful for layout work, but native process, database, filesystem, and hardware behavior must be tested in Tauri.
9. **Shared traits contain concrete engines.** The llama.cpp adapter implements lifecycle and health through `InferenceEngine` plus bounded streaming generation and cancellation through `ChatEngine`.
10. **Process ownership is explicit.** Only tracked child handles are stopped. NeuraLoc-Core does not kill by executable name or occupied port.

## Database and Migrations

The database file is `neuraloc-core.db` inside the Tauri-resolved platform application-data directory. On open, SQLite enables WAL mode, enables foreign-key enforcement, applies a five-second busy timeout, and runs pending migrations.

### Migration mechanism

- `src-tauri/src/storage/migrations.rs` owns the ordered migration registry.
- `src-tauri/migrations/0001_foundation.sql` is migration version 1, name `foundation`.
- `src-tauri/migrations/0002_model_library.sql` is migration version 2, name `model_library`; version 1 remains unchanged.
- `src-tauri/migrations/0003_engine_packages.sql` is migration version 3, name `engine_packages`; prior migration files remain unchanged.
- The runner creates `schema_migrations` defensively, checks each version, executes unapplied SQL in a transaction, then records the version and UTC timestamp.
- Tests run all migrations twice and upgrade a simulated version-1 database, confirming three ledger rows, the new model columns, and the engine-package table.

### Foundation schema

| Table | Purpose | Current code usage |
| --- | --- | --- |
| `schema_migrations` | Applied version, name, and timestamp | Active |
| `settings` | JSON settings by stable key | Active through `get_setting`/`put_setting` |
| `prompt_profiles` | Stable prompt identity, collection, pin, soft delete | Schema only |
| `prompt_versions` | Immutable prompt content, hash, source, front matter, version | Schema only |
| `models` | Local model identity, type, path, size, verification, GGUF metadata, file identity | Active through `ModelRepository`/`ModelService` |
| `engine_packages` | Installed engine version/route/path, archive checksum, file inventory, state, errors, timestamps | Active through `EnginePackageRepository`/`EnginePackageService` |
| `conversations` | Chat identity and selected model/prompt/settings | Schema only |
| `messages` | Branchable conversation messages and token counts | Schema only |
| `downloads` | Resumable download state, byte counts, ETag, checksum, errors | Schema only |
| `benchmarks` | Hardware/engine/model benchmark results | Schema only |
| `outputs` | Generated file metadata and thumbnails | Schema only |
| `jobs` | Durable workload state, requests, results, and errors | Schema only |

Foreign keys connect prompt versions to profiles, conversations to models/prompt versions, and messages to conversations/parents. Deleting a conversation cascades to its messages. Unique constraints prevent duplicate model paths, duplicate prompt versions/hashes, and duplicate output paths.

Indexes exist for conversation recency, conversation messages, model kind, model verification state, unique non-null file identity, engine package state/route, download state/recency, output kind/recency, and benchmark lookup.

## Existing Tauri Commands

| Command | Input | Response | Current behavior |
| --- | --- | --- | --- |
| `get_app_snapshot` | none | `AppSnapshot` | Returns crate version, database ready, process count, and scaffold values for first-run/jobs |
| `list_engine_packages` | none | `EnginePackageStatus[]` | Returns bundled manifests joined with persisted installation state |
| `install_engine_package` | package ID | `EnginePackageRecord` | Downloads, verifies, safely extracts, inventories, and atomically installs the pinned package when internet access is enabled |
| `import_engine_package` | package ID and granted `.zip` path | `EnginePackageRecord` | Performs the same exact size/checksum/extraction/install flow for an offline archive |
| `verify_engine_package` | package ID | `EnginePackageRecord` | Verifies the exact installed file set, sizes, and SHA-256 inventory |
| `uninstall_engine_package` | package ID | none | Removes only the manifest-owned internal package directory and its database record |
| `get_engine_status` | none | `EngineRuntimeStatus` | Returns package/session lifecycle, process/model identity, version, exit metadata, and detail |
| `get_engine_health` | none | `EngineHealth` | Rechecks the owned loopback server and authenticated model/build identity |
| `start_engine` | model ID with optional context/threads | `EngineRuntimeStatus` | Reverifies model/package/binary, launches the CPU server, and waits for owned ready state |
| `stop_engine` | session ID | `EngineRuntimeStatus` | Stops only the matching retained owned session and returns its final state |
| `get_engine_logs` | session ID | `EngineLogSnapshot` | Returns the process manager's bounded, redacted retained lines |
| `start_chat_generation` | job/conversation/message/session IDs, bounded role/content messages, output limit | `ChatGenerationResult` | Requires the matching ready session, streams token batches, and returns completed/cancelled state plus usage |
| `cancel_chat_generation` | job ID | boolean | Cancels only the matching active generation without stopping the loaded model |
| `get_hardware_snapshot` | none | `HardwareSnapshot` | Returns cached hardware data or performs the first native probe |
| `refresh_hardware` | none | `HardwareSnapshot` | Forces `sysinfo`, NVIDIA, and Windows NPU probes and replaces the cache |
| `get_settings` | none | `AppSettings` | Returns the in-memory settings loaded from SQLite/defaults |
| `update_settings` | typed partial `SettingsPatch` | `AppSettings` | Validates, normalizes dependent flags, persists JSON, and returns the full state |
| `list_models` | none | `ModelRecord[]` | Returns sorted persisted model summaries and bounded GGUF metadata |
| `import_model` | granted absolute `.gguf` path | `ImportModelOutcome` | Canonicalizes, deduplicates, inspects, and persists ready/invalid state |
| `scan_model_folder` | scan ID and granted folder | `ModelScanSummary` | Recursively discovers/imports GGUF files with limits and progress events |
| `cancel_model_scan` | scan ID | boolean | Signals a live discovery/import scan to stop |
| `reverify_model` | model ID | `ModelRecord` | Refreshes file state/metadata or marks a missing record without deleting it |
| `remove_model_record` | model ID | none | Removes SQLite metadata only; the GGUF file remains on disk |

`get_app_snapshot` currently reports `databaseReady: true` and `firstRunComplete: false` as fixed values. `activeJobs` reflects the active llama.cpp generation registry and the top bar follows chat terminal events. `runningEngines` is the number of owned processes, which may include future non-engine owned processes unless that accounting is refined.

## Existing Events

`EventEnvelope<T>` is emitted through a central utility with `eventVersion: 1`, per-stream monotonic sequence, UTC `emittedAt`, and a typed payload.

`model://scan-progress` is consumed for discovery/import progress. `engine://state-changed` and bounded-batch `engine://log-line` are consumed by Model Manager and Chat for live and terminal session updates. `chat://token`, `chat://usage`, and `chat://state-changed` are sequenced per generation and consumed by Chat. Job, download, telemetry, and settings events remain planned contracts.

## Passing Tests and Build Commands

Verified on Windows 11 with Node `v24.18.0`, npm `11.16.0`, Rust `1.97.0`, Cargo `1.97.0`, and the stable `x86_64-pc-windows-msvc` toolchain.

```powershell
npm.cmd run build
npm.cmd run test
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
$env:Path = "$HOME\.cargo\bin;$env:Path"
npm.cmd run tauri -- build --debug --no-bundle
```

Current automated tests:

- Frontend: 3 Vitest files, 7 tests for adaptive byte formatting, missing telemetry, model metadata, selector grouping/labels, and selected-session readiness.
- Rust: 30 passing default tests plus two ignored opt-in integration tests. Coverage includes bounded chat request validation, non-thinking chat payloads, split SSE decoding, usage extraction, owned build probes, pinned-version parsing, fixed/bounded llama.cpp arguments, and `/props` model/build identity validation alongside the hardware, database, GGUF, model, process, and package suites.
- Opt-in package integration: the ignored test downloads the pinned official archive, completes install, runs the real `llama-server.exe --version --help` probe and observes build `9986`, verifies the exact files, and uninstalls in a temporary application-data directory; it passed on 2026-07-13.
- Opt-in real-model integration: an environment-selected `llama-server.exe` and tensor-bearing GGUF are loaded with the same adapter, health-checked, required to return a known visible answer with thinking disabled, usage-checked, cancellation-checked, stopped, and checked for zero owned child processes. It passed with Qwen3 4B Q4_K_M and `b9986` on 2026-07-14.

The Tauri debug build also runs the production frontend build as its configured pre-build command.

## Generated Executable

The verified unpackaged Windows debug executable is:

```text
C:\Users\atrx07\atrx\NeuraLoc-Core\src-tauri\target\debug\neuraloc-core.exe
```

Checkpoint size: 29,455,872 bytes, rebuilt on 2026-07-14 after the visible-response and navigation-lifecycle fixes. This is a debug executable, not a signed installer or release artifact. `src-tauri/target` is ignored by Git and can be regenerated.

## Known Warnings and Limitations

- `cargo clippy --all-targets` passes but reports expected dead-code warnings for scheduler and other interfaces that remain scaffolded.
- Rust is installed under `%USERPROFILE%\.cargo\bin`; terminals that do not inherit that user PATH require the session PATH command shown above.
- npm may print a non-fatal `could not canonicalize path C:\Users\atrx07` warning in the current host environment.
- Hardware discovery is partial: no Vulkan loader enumeration, Intel iGPU details, disks, battery/power state, instruction-set report, OpenVINO runtime probe, or robust driver/runtime version inventory exists yet.
- CUDA readiness currently means `nvidia-smi` responded; a compatible llama.cpp CUDA package has not been installed or validated.
- The bundled engine catalog currently contains only llama.cpp `b9986` Windows x64 CPU. CUDA/Vulkan packages, resumable package downloads, package progress events, and package updates remain pending.
- NPU detection is name/text based through `pnputil`; model compatibility still requires a future OpenVINO compile probe.
- The current pinned llama.cpp server exposes no dedicated process-shutdown endpoint used by this adapter, so stop applies a 250 ms grace interval and then force-stops only the owned handle. Crash recovery/restart policy is not implemented.
- The scheduler is a resource classification scaffold, not a queue or durable job runner. `activeJobs` currently reflects only the active llama.cpp generation.
- Model scan, engine lifecycle/log, and chat token/usage/state events are sequenced and consumed. Early model-loading state is polled while `start_engine` awaits readiness; download events, general throttling, and broader stale-sequence handling remain unfinished.
- Settings, models, and engine packages have repository/service implementations. Prompt, conversation/message, general download, output, benchmark, and job repositories remain unfinished.
- The first-run flag is fixed false; setup flow and completion persistence are not implemented.
- The optional local API is only a setting. No server is started, and LAN access is not exposed in the UI.
- Frontend async initialization/settings updates have minimal error handling and no global error boundary.
- Model services use temporary-file database/fixture tests and process lifecycle uses copied deterministic test executables, but there are no direct Tauri command harness tests, automated native-window UI tests, or installer smoke tests.
- The NSIS target is configured, but release packaging, code signing, updater policy, and runtime/model package signing are unfinished.

## Unfinished or Scaffolded Functionality

- Full SHA-256 verification/catalog matching, startup-wide missing-file reconciliation, automatic relocation discovery, and deliberate delete-file workflows for imported models.
- Advanced GGUF compatibility normalization, RAM/VRAM estimates, projector pairing, hostile-format corpus coverage, and installed-engine validation.
- A redistributable tensor-bearing GGUF fixture for normal CI, graceful protocol-level request draining, conservative OOM fallback, and crash recovery. The environment-selected real-model load/stream/stop test is opt-in and passed locally.
- Advanced model selector compatibility/fit estimates, disabled-state explanations, load estimates, and Rust-persisted preference. Basic library-backed selection and runtime control are implemented.
- Markdown/text system-prompt import, YAML front matter, hashing, immutable versions, editing, searching, and selector binding.
- Durable streaming chat persistence, retry, context budgeting, model-template options, Markdown rendering, reasoning presentation, and crash recovery. Basic streaming, token/state/usage events, generation stop, and ephemeral messages are implemented.
- Conversation/message repositories, history list, branches, titles, pinning, export, and crash-safe partial responses.
- Signed model catalog, catalog refresh, recommendations, resumable downloads, verification, pause/retry, and installation.
- Image generation, speech recognition, text-to-speech, gallery, downloads, and logs are visual empty states only.
- OpenVINO, Vulkan, stable-diffusion.cpp, whisper.cpp, and Kokoro runtime adapters.
- Real scheduler queue, durable jobs, telemetry samples, benchmark execution, and hardware-aware routing.
- Portable data mode, diagnostics export/redaction, optional authenticated API, updater, release installer, and signing.

See `NEXT_STEPS.md` for the dependency-aware next-phase plan.
