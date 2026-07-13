# NeuraLoc-Core Status

Status date: 2026-07-12

Project root: `C:\Users\atrx07\atrx\NeuraLoc-Core`

Current version: `0.1.0`

## Summary

NeuraLoc-Core is a working Windows desktop foundation built with Tauri 2, React, TypeScript, and Rust. It starts, creates its application data directories and SQLite database, exposes typed commands for app state, hardware, and settings, and renders a polished desktop shell with functional Hardware and Settings views. The project also contains foundations for owned child processes, inference engine adapters, resource-fit policy, versioned events, and the full planned metadata schema.

This checkpoint is not yet a local inference product. No model can currently be imported, loaded, downloaded, or used for chat. Most non-system workspaces are intentional empty states.

## Implemented Functionality

### Desktop and frontend

- Tauri v2 desktop application with one resizable window, 1280 x 820 default size and 900 x 620 minimum size.
- React 18 renderer, strict TypeScript, Vite 6 build, Zustand UI state, and Lucide icons.
- Responsive application shell with collapsible navigation for Chat, Images, Speech, Text to Speech, Models, Prompts, Gallery, Downloads, Hardware, Logs, and Settings.
- Dark, light, and system theme handling. The chosen theme is persisted by the Rust settings service when running in Tauri.
- Functional Hardware view with refresh, CPU/RAM summary, detected accelerators, capability evidence, telemetry fields, and warnings.
- Functional Settings view for theme, performance profile, model retention, idle timeout, internet access, web search, and local API state.
- Browser-only demo bridge for UI development when Tauri IPC is unavailable. Demo settings persist only for the current page session and hardware values are representative data.
- Typed frontend domain interfaces for app snapshots, settings, hardware, devices, capabilities, navigation, and IPC errors.
- Shared binary-byte formatter with frontend unit tests.

### Rust application core

- Tauri startup creates the platform application-data directory and these children: `models`, `outputs`, `prompts`, `downloads`, `cache`, and `logs`.
- SQLite database creation using bundled SQLite, WAL journal mode, foreign keys, and a five-second busy timeout.
- Ordered transactional migration runner with a migration ledger and an idempotency test.
- Thread-safe `AppState` containing `Database`, `HardwareService`, `ProcessManager`, and `SettingsService` handles.
- Stable application and IPC error types with machine-readable error codes and user-facing suggestions.
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
  - starts executables without a shell and passes arguments as an array;
  - nulls stdin and captures stdout/stderr;
  - keeps a bounded 2,000-line in-memory log per process;
  - assigns UUID ownership IDs and records PID/start time;
  - provides summaries and logs internally;
  - limits native probes to four seconds;
  - stops all registered processes on normal application exit.
- Engine lifecycle enum and shared `InferenceEngine`/`ChatEngine` traits with typed configuration, start request, health, token chunks, and usage.
- Scheduler domain types for job kinds/states and a resource policy that labels memory fit as excellent, good, tight, or not recommended.
- Full generated desktop icon bundle, including Windows ICO/PNG, macOS ICNS, iOS, and Android assets.
- Tauri capability file grants only `core:default` to the main window; no filesystem, shell, or network plugin is exposed to the renderer.

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
    |-- migrations/0001_foundation.sql
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
        |   |-- hardware_commands.rs
        |   |-- settings_commands.rs
        |   `-- mod.rs
        |-- engines/
        |   |-- traits.rs
        |   `-- mod.rs
        |-- hardware/
        |   |-- detector.rs
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
- The runner creates `schema_migrations` defensively, checks each version, executes unapplied SQL in a transaction, then records the version and UTC timestamp.
- The existing test runs the migration twice against an in-memory database and confirms exactly one ledger row.

### Foundation schema

| Table | Purpose | Current code usage |
| --- | --- | --- |
| `schema_migrations` | Applied version, name, and timestamp | Active |
| `settings` | JSON settings by stable key | Active through `get_setting`/`put_setting` |
| `prompt_profiles` | Stable prompt identity, collection, pin, soft delete | Schema only |
| `prompt_versions` | Immutable prompt content, hash, source, front matter, version | Schema only |
| `models` | Local model identity, type, format, path, size, checksum, compatibility | Schema only |
| `conversations` | Chat identity and selected model/prompt/settings | Schema only |
| `messages` | Branchable conversation messages and token counts | Schema only |
| `downloads` | Resumable download state, byte counts, ETag, checksum, errors | Schema only |
| `benchmarks` | Hardware/engine/model benchmark results | Schema only |
| `outputs` | Generated file metadata and thumbnails | Schema only |
| `jobs` | Durable workload state, requests, results, and errors | Schema only |

Foreign keys connect prompt versions to profiles, conversations to models/prompt versions, and messages to conversations/parents. Deleting a conversation cascades to its messages. Unique constraints prevent duplicate model paths, duplicate prompt versions/hashes, and duplicate output paths.

Indexes exist for conversation recency, conversation messages, model kind, download state/recency, output kind/recency, and benchmark lookup.

## Existing Tauri Commands

| Command | Input | Response | Current behavior |
| --- | --- | --- | --- |
| `get_app_snapshot` | none | `AppSnapshot` | Returns crate version, database ready, process count, and scaffold values for first-run/jobs |
| `get_hardware_snapshot` | none | `HardwareSnapshot` | Returns cached hardware data or performs the first native probe |
| `refresh_hardware` | none | `HardwareSnapshot` | Forces `sysinfo`, NVIDIA, and Windows NPU probes and replaces the cache |
| `get_settings` | none | `AppSettings` | Returns the in-memory settings loaded from SQLite/defaults |
| `update_settings` | typed partial `SettingsPatch` | `AppSettings` | Validates, normalizes dependent flags, persists JSON, and returns the full state |

`get_app_snapshot` currently reports `databaseReady: true`, `firstRunComplete: false`, and `activeJobs: 0` as fixed values. `runningEngines` is the number of owned processes, which may include future non-engine owned processes unless that accounting is refined.

## Existing Events

`EventEnvelope<T>` is implemented with `eventVersion: 1`, a caller-supplied monotonic sequence, UTC `emittedAt`, and a typed payload.

No application event is emitted or consumed yet. Names described in `ARCHITECTURE.md` such as `hardware://updated`, `engine://state-changed`, `chat://token`, and `download://progress` are planned contracts, not current runtime behavior.

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

- Frontend: 1 Vitest file, 2 tests for byte formatting and missing telemetry.
- Rust: NVIDIA CSV parsing, migration idempotency, and memory-reserve fit policy.

The Tauri debug build also runs the production frontend build as its configured pre-build command.

## Generated Executable

The verified unpackaged Windows debug executable is:

```text
C:\Users\atrx07\atrx\NeuraLoc-Core\src-tauri\target\debug\neuraloc-core.exe
```

Checkpoint size: 17,983,488 bytes. This is a debug executable, not a signed installer or release artifact. `src-tauri/target` is ignored by Git and can be regenerated.

## Known Warnings and Limitations

- `cargo clippy --all-targets` passes but reports expected dead-code warnings for engine, scheduler, event, and process interfaces that are scaffolded but not connected to commands yet.
- Rust is installed under `%USERPROFILE%\.cargo\bin`; terminals that do not inherit that user PATH require the session PATH command shown above.
- npm may print a non-fatal `could not canonicalize path C:\Users\atrx07` warning in the current host environment.
- Hardware discovery is partial: no Vulkan loader enumeration, Intel iGPU details, disks, battery/power state, instruction-set report, OpenVINO runtime probe, or robust driver/runtime version inventory exists yet.
- CUDA readiness currently means `nvidia-smi` responded; a compatible llama.cpp CUDA package has not been installed or validated.
- NPU detection is name/text based through `pnputil`; model compatibility still requires a future OpenVINO compile probe.
- Process states are not advanced after spawn, natural child exit is not reaped into a crashed/stopped state, logs are not exposed through IPC, and stop currently waits before force-killing rather than sending an adapter-specific graceful shutdown request.
- The scheduler is a resource classification scaffold, not a queue or job runner. `activeJobs` is fixed at zero.
- The event envelope exists, but no events, throttling, stale-sequence handling, or frontend listeners exist.
- Only settings have a repository/service implementation. The remaining schema tables have no CRUD layer.
- The first-run flag is fixed false; setup flow and completion persistence are not implemented.
- The optional local API is only a setting. No server is started, and LAN access is not exposed in the UI.
- Frontend async initialization/settings updates have minimal error handling and no global error boundary.
- Test coverage is foundation-level; there are no command integration tests, temporary-file database tests, process fixture tests, Tauri UI tests, or installer smoke tests.
- The NSIS target is configured, but release packaging, code signing, updater policy, and runtime/model package signing are unfinished.

## Unfinished or Scaffolded Functionality

- Local model folder discovery, file picker, GGUF import, revalidation, removal, metadata parsing, checksums, and model repositories.
- llama.cpp package acquisition, verification, installation, engine adapter, health checks, loopback ownership, lifecycle transitions, logging UI, cancellation, and crash recovery.
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
