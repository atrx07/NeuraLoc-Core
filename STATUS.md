# NeuraLoc-Core Status

Status date: 2026-07-13

Project root: `C:\Users\atrx07\atrx\NeuraLoc-Core`

Current version: `0.1.0`

## Summary

NeuraLoc-Core is a working Windows desktop foundation built with Tauri 2, React, TypeScript, and Rust. It starts, creates its application data directories and SQLite database, exposes typed commands for app state, hardware, settings, local models, and engine packages, and renders a polished desktop shell with functional Hardware, Settings, and Model Manager views. A pinned Windows x64 CPU llama.cpp package can now be downloaded or imported offline, verified, installed, reverified, and uninstalled through the Model Manager. The project also contains foundations for owned child processes, inference engine adapters, resource-fit policy, and versioned events.

This checkpoint is not yet a local inference product. GGUF models can be indexed and inspected and the pinned CPU runtime can be installed, but llama.cpp cannot yet be started and models cannot yet be loaded, downloaded, or used for chat. Prompt, chat, model-catalog download, image, speech, TTS, gallery, and logs workspaces remain intentional empty states.

## Implemented Functionality

### Desktop and frontend

- Tauri v2 desktop application with one resizable window, 1280 x 820 default size and 900 x 620 minimum size.
- React 18 renderer, strict TypeScript, Vite 6 build, Zustand UI state, and Lucide icons.
- Responsive application shell with collapsible navigation for Chat, Images, Speech, Text to Speech, Models, Prompts, Gallery, Downloads, Hardware, Logs, and Settings.
- Dark, light, and system theme handling. The chosen theme is persisted by the Rust settings service when running in Tauri.
- Functional Hardware view with refresh, CPU/RAM summary, detected accelerators, capability evidence, telemetry fields, and warnings.
- Functional Settings view for theme, performance profile, model retention, idle timeout, internet access, web search, and local API state.
- Functional Model Manager with native GGUF file import, recursive folder scanning, cancellation/progress, search, metadata/status rows, reverify, metadata-only removal, and llama.cpp CPU runtime install/import/verify/uninstall controls.
- Catalog and Downloads tabs remain visibly disabled until the verified catalog checkpoint.
- Browser-only demo bridge for UI development when Tauri IPC is unavailable. Demo settings persist only for the current page session, hardware values are representative, and native model imports are unavailable.
- Typed frontend domain interfaces for app snapshots, settings, hardware, local models, GGUF metadata, engine packages, scan events, navigation, and IPC errors.
- Shared adaptive binary-byte and model-metadata formatters with frontend unit tests.

### Rust application core

- Tauri startup creates workload-specific `models/{llm,image,speech,tts}` and `outputs/{images,transcripts,speech}` directories plus `prompts`, `downloads`, `cache`, and `logs`.
- SQLite database creation using bundled SQLite, WAL journal mode, foreign keys, and a five-second busy timeout.
- Ordered transactional migration runner with a migration ledger, idempotency coverage, and an explicit version-1 upgrade test.
- Additive migration 2 extends model records with verification state/error, bounded GGUF metadata JSON, modification time, and stable file identity.
- Additive migration 3 adds engine-package identity, route, install path, archive checksum, installed-file inventory, state, source, errors, and install/verification timestamps.
- Thread-safe `AppState` containing `Database`, `EnginePackageService`, `EventEmitter`, `HardwareService`, `ModelService`, `ProcessManager`, and `SettingsService` handles.
- Stable application and IPC error types with machine-readable error codes and user-facing suggestions.
- Model repository/service and typed commands for list, import, recursive scan, cancellation, reverify, and record removal.
- Engine-package repository/service and typed commands for status, online install, offline import, reverify, and uninstall.
- Bundled manifest 1 pins llama.cpp `b9986` Windows x64 CPU to its official HTTPS asset, exact 18,245,837-byte size, SHA-256, route, architecture, and expected runtime files.
- Online package installation is gated by `internetAccess`, limits redirects to approved GitHub HTTPS hosts, streams to `.partial`, enforces exact size/SHA-256, and removes the partial after success or failure.
- ZIP installation rejects traversal, links/reparse points, Windows device/alternate-stream names, duplicate paths, excessive entries/files/sizes, missing expected files, and untracked installed files; promotion uses an internal staging directory and atomic rename.
- Installation records a SHA-256 inventory for every extracted file. Reverify rejects missing, changed, linked, or added files, and startup reconciles interrupted or missing installations.
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
  - allows an adapter grace period before force-stopping only the owned child;
  - limits native probes to four seconds;
  - stops all registered processes on normal application exit.
- Engine lifecycle enum and shared `InferenceEngine`/`ChatEngine` traits with typed configuration, start request, health, token chunks, and usage.
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
        |   |-- engine_package_commands.rs
        |   |-- hardware_commands.rs
        |   |-- model_commands.rs
        |   |-- settings_commands.rs
        |   `-- mod.rs
        |-- engines/
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
2. **Rust owns native orchestration.** The central `ProcessManager` is intended to be the only child-process entry point. Engine adapters will use it rather than spawning independently.
3. **Inference is delegated.** NeuraLoc-Core is an orchestration and UX layer. Proven native engines such as llama.cpp will perform inference.
4. **Local and private by default.** Network-related settings default off. The desktop shell opens no application API port, and the CSP restricts content to packaged/Tauri resources.
5. **Large assets remain files.** Models, outputs, prompts, downloads, cache, and logs live in the application data directory. SQLite stores metadata and relationships.
6. **Schema changes are additive migrations.** Applied migrations are recorded and each migration runs in a transaction. Existing migration files should not be edited after release; add `0002_*` and later files.
7. **Capability claims require evidence.** Hardware support uses available/unknown/experimental states and does not equate device presence with backend/model compatibility.
8. **Frontend browser mode is a demo adapter.** It is useful for layout work, but native process, database, filesystem, and hardware behavior must be tested in Tauri.
9. **Shared traits precede concrete engines.** Engine and chat traits define lifecycle, health, token streaming, and cancellation contracts before llama.cpp is connected.
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

`get_app_snapshot` currently reports `databaseReady: true`, `firstRunComplete: false`, and `activeJobs: 0` as fixed values. `runningEngines` is the number of owned processes, which may include future non-engine owned processes unless that accounting is refined.

## Existing Events

`EventEnvelope<T>` is emitted through a central utility with `eventVersion: 1`, per-stream monotonic sequence, UTC `emittedAt`, and a typed payload.

`model://scan-progress` is emitted by Rust and consumed by the Model Manager for discovery/import progress. Sequence state is released when the scan ends. Other names described in `ARCHITECTURE.md`, such as `engine://state-changed`, `chat://token`, and `download://progress`, remain planned contracts.

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

- Frontend: 2 Vitest files, 5 tests for adaptive byte formatting, missing telemetry, parameter counts, and context lengths.
- Rust: 23 passing default tests plus one ignored network integration test. Coverage includes NVIDIA CSV parsing, resource fit, migration idempotency/version-1 upgrade, bounded GGUF parsing/failures, model path safety/persistence/deduplication/reconciliation/scanning, process lifecycle/logging/shutdown, package manifest validation, checksum rejection, traversal/device/alternate-stream defenses, installed-file inventory, tamper/added-file detection, and safe extraction.
- Opt-in package integration: the ignored test downloads the pinned official archive and completes install, exact file verification, and uninstall in a temporary application-data directory; it passed on 2026-07-13.

The Tauri debug build also runs the production frontend build as its configured pre-build command.

## Generated Executable

The verified unpackaged Windows debug executable is:

```text
C:\Users\atrx07\atrx\NeuraLoc-Core\src-tauri\target\debug\neuraloc-core.exe
```

Checkpoint size: 20,422,656 bytes. This is a debug executable, not a signed installer or release artifact. `src-tauri/target` is ignored by Git and can be regenerated.

## Known Warnings and Limitations

- `cargo clippy --all-targets` passes but reports expected dead-code warnings for engine, scheduler, and process interfaces that are scaffolded but not connected to runtime commands yet.
- Rust is installed under `%USERPROFILE%\.cargo\bin`; terminals that do not inherit that user PATH require the session PATH command shown above.
- npm may print a non-fatal `could not canonicalize path C:\Users\atrx07` warning in the current host environment.
- Hardware discovery is partial: no Vulkan loader enumeration, Intel iGPU details, disks, battery/power state, instruction-set report, OpenVINO runtime probe, or robust driver/runtime version inventory exists yet.
- CUDA readiness currently means `nvidia-smi` responded; a compatible llama.cpp CUDA package has not been installed or validated.
- The bundled engine catalog currently contains only llama.cpp `b9986` Windows x64 CPU. CUDA/Vulkan packages, resumable package downloads, package progress events, and package updates remain pending.
- NPU detection is name/text based through `pnputil`; model compatibility still requires a future OpenVINO compile probe.
- Process lifecycle and retained logs are not exposed through IPC yet. The manager supports an adapter grace period, but the llama.cpp adapter still needs to issue its protocol-specific shutdown request before force-stop fallback and emit engine events.
- The scheduler is a resource classification scaffold, not a queue or job runner. `activeJobs` is fixed at zero.
- Model scan events are sequenced and consumed, but engine/chat/download events, throttling, and broader stale-sequence handling remain unfinished.
- Settings, models, and engine packages have repository/service implementations. Prompt, conversation/message, general download, output, benchmark, and job repositories remain unfinished.
- The first-run flag is fixed false; setup flow and completion persistence are not implemented.
- The optional local API is only a setting. No server is started, and LAN access is not exposed in the UI.
- Frontend async initialization/settings updates have minimal error handling and no global error boundary.
- Model services use temporary-file database/fixture tests, but there are no direct Tauri command harness tests, process fixture tests, automated native-window UI tests, or installer smoke tests.
- The NSIS target is configured, but release packaging, code signing, updater policy, and runtime/model package signing are unfinished.

## Unfinished or Scaffolded Functionality

- Full SHA-256 verification/catalog matching, startup-wide missing-file reconciliation, automatic relocation discovery, and deliberate delete-file workflows for imported models.
- Advanced GGUF compatibility normalization, RAM/VRAM estimates, projector pairing, hostile-format corpus coverage, and installed-engine validation.
- Concrete llama.cpp engine adapter, version/health probes, loopback ownership, model loading, lifecycle events, logging UI, cancellation, and crash recovery. The pinned CPU package acquisition/verification/installation slice is complete.
- Model selector data and compatibility/fit recommendations.
- Markdown/text system-prompt import, YAML front matter, hashing, immutable versions, editing, searching, and selector binding.
- Streaming chat generation, token events, generation controls, cancellation, usage metrics, and context management.
- Conversation/message repositories, history list, branches, titles, pinning, export, and crash-safe partial responses.
- Signed model catalog, catalog refresh, recommendations, resumable downloads, verification, pause/retry, and installation.
- Image generation, speech recognition, text-to-speech, gallery, downloads, and logs are visual empty states only.
- OpenVINO, Vulkan, stable-diffusion.cpp, whisper.cpp, and Kokoro runtime adapters.
- Real scheduler queue, durable jobs, telemetry samples, benchmark execution, and hardware-aware routing.
- Portable data mode, diagnostics export/redaction, optional authenticated API, updater, release installer, and signing.

See `NEXT_STEPS.md` for the dependency-aware next-phase plan.
