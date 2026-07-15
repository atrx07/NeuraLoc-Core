# NeuraLoc-Core Status

Status date: 2026-07-15

Project root: `C:\Users\atrx07\atrx\NeuraLoc-Core`

Current version: `0.1.0`

## Summary

NeuraLoc-Core is an executable local-chat prototype built with Tauri 2, React, TypeScript, and Rust. It starts, creates its application data directories and SQLite database, exposes typed commands for app state, hardware, settings, local models, engine packages, llama.cpp runtime control, bounded chat generation, and immutable system prompts, and renders a polished desktop shell with functional Chat, Prompt Library, Hardware, Settings, and Model Manager views. The pinned Windows x64 CPU llama.cpp package can be installed and verified, and a ready indexed GGUF can be selected from Chat, launched, health-checked, streamed, cancelled, stopped, and inspected without exposing the internal server or session token to the renderer.

This checkpoint has durable linear local chat with an optional immutable system-prompt version, but it is not yet a complete local inference product. Native turns are stored before inference and finalized with exact streamed text, usage, and terminal state; startup identifies abandoned drafts. The searchable Chat rail lazily opens messages, restores the last conversation and exact bindings after restart, and supports rename, pin, bounded provenance-preserving Markdown export, and cascade delete. The backend can transactionally clone an independent conversation branch with remapped parent links and source provenance; Chat controls and retry generation are not connected yet. A local opt-in Qwen3 4B integration passed on 2026-07-14. Multi-layer prompt composition, enforced context management, model-catalog download, image, speech, TTS, gallery, and the dedicated Logs workspace remain unfinished.

## Implemented Functionality

### Desktop and frontend

- Tauri v2 desktop application with one resizable window, 1280 x 820 default size and 900 x 620 minimum size.
- React 18 renderer, strict TypeScript, Vite 6 build, Zustand UI state, and Lucide icons.
- Responsive application shell with collapsible navigation for Chat, Images, Speech, Text to Speech, Models, Prompts, Gallery, Downloads, Hardware, Logs, and Settings.
- Dark, light, and system theme handling. The chosen theme is persisted by the Rust settings service when running in Tauri.
- Functional Hardware view with refresh, CPU/RAM summary, detected accelerators, capability evidence, telemetry fields, and warnings.
- Functional Settings view for theme, performance profile, model retention, idle timeout, internet access, web search, and local API state.
- Functional Model Manager with native GGUF file import, recursive folder scanning, cancellation/progress, search, metadata/status rows, reverify, metadata-only removal, llama.cpp package controls, per-model load/stop controls, runtime health, lifecycle state, and retained-log inspection.
- Functional Prompt Library with native Markdown/text import, search, pinned-first summaries, exact source inspection, local creation, immutable version editing, duplicate provenance, pin/unpin, original/normalized export, and soft deletion.
- Functional Chat model selector backed by the persisted model library, grouped ready/unavailable choices, last-used preference, runtime reuse/load/switch/unload controls, visible lifecycle state, and a composer gate tied to the selected ready session.
- Chat messages with Rust-owned streaming token batches, stop generation, terminal/error states, and usage telemetry. Native generation transactionally persists the user message and assistant draft before inference, then finalizes the exact streamed response, usage, token count, and terminal reason. The history rail searches summary metadata, loads one conversation body on demand, restores model/prompt bindings and the last opened conversation, exposes inline rename/pin/export/delete actions, and becomes an overlay drawer at the 900px breakpoint. Interrupted drafts render explicitly instead of disappearing. Export reads SQLite through Rust and returns at most 16 MiB of Markdown with structured model, prompt, context, and generation-setting provenance.
- Catalog and Downloads tabs remain visibly disabled until the verified catalog checkpoint.
- Browser-only demo bridge for UI development when Tauri IPC is unavailable. Demo settings persist only for the current page session, hardware values are representative, read-only sample prompt/conversation records exercise library/history/restoration layouts, and native mutations are unavailable.
- Typed frontend domain interfaces for app snapshots, settings, hardware, local models, GGUF metadata, engine packages, runtime lifecycle/health/logs, chat requests/results/usage/events, scan/engine events, navigation, and IPC errors.
- Shared adaptive binary-byte and model-metadata formatters with frontend unit tests.

### Rust application core

- Tauri startup creates workload-specific `models/{llm,image,speech,tts}` and `outputs/{images,transcripts,speech}` directories plus `prompts`, `downloads`, `cache`, and `logs`.
- SQLite database creation using bundled SQLite, WAL journal mode, foreign keys, and a five-second busy timeout.
- Ordered transactional migration runner with a migration ledger, idempotency coverage, and an explicit version-1 upgrade test.
- Additive migration 2 extends model records with verification state/error, bounded GGUF metadata JSON, modification time, and stable file identity.
- Additive migration 3 adds engine-package identity, route, install path, archive checksum, installed-file inventory, state, source, errors, and install/verification timestamps.
- Additive migration 4 adds prompt profile timestamps, exact raw source documents, duplicate provenance, and prompt-library indexes without modifying the foundation migration.
- Additive migration 5 adds assistant draft state, job/usage/terminal metadata, deterministic message positions, recovery timestamps, and conversation/message indexes.
- Additive migration 6 adds nullable source-conversation, branch-point, and source-message provenance plus lookup indexes. Branches retain copied content when their source is deleted.
- Thread-safe `AppState` containing `Database`, `ConversationService`, `EnginePackageService`, `EngineRuntimeService`, `EventEmitter`, `HardwareService`, `ModelService`, `ProcessManager`, `PromptService`, and `SettingsService` handles.
- Stable application and IPC error types with machine-readable error codes and user-facing suggestions.
- Model repository/service and typed commands for list, import, recursive scan, cancellation, reverify, and record removal.
- Prompt repository/service and typed commands for search/list, native-dialog import, create, immutable version save, historical version read, duplicate, pin, soft delete, original/normalized export, and exact-content compile.
- Conversation repository/service and typed commands for deterministic list/page/search, lazy open, rename, pin, and cascade delete. Turn creation stores the conversation binding, linear parentage, user message, and assistant draft in one transaction; finalization stores exact text, usage, token count, and terminal reason, while startup marks abandoned drafts interrupted.
- Prompt parsing accepts bounded UTF-8 `.md`/`.txt` documents with an optional BOM and leading YAML 1.2 front matter, preserves source content and line endings, rejects aliases/anchors/custom tags/excessive nesting, validates known metadata, preserves unknown fields as inert JSON, and hashes canonical metadata plus exact content for no-op duplicate detection.
- Engine-package repository/service and typed commands for status, online install, offline import, reverify, and uninstall.
- Bundled manifest 1 pins llama.cpp `b9986` Windows x64 CPU to its official HTTPS asset, exact 18,245,837-byte size, SHA-256, route, architecture, and expected runtime files.
- Online package installation is gated by `internetAccess`, limits redirects to approved GitHub HTTPS hosts, streams to `.partial`, enforces exact size/SHA-256, and removes the partial after success or failure.
- ZIP installation rejects traversal, links/reparse points, Windows device/alternate-stream names, duplicate paths, excessive entries/files/sizes, missing expected files, and untracked installed files; promotion uses an internal staging directory and atomic rename.
- Installation records a SHA-256 inventory for every extracted file. Reverify rejects missing, changed, linked, or added files, and startup reconciles interrupted or missing installations.
- Concrete llama.cpp CPU adapter and runtime service that reverify the selected model and complete package inventory, run a terminating `--version --help` build probe, allow only bounded context/thread options, and launch only the canonical packaged executable.
- The adapter reserves loopback port `0`, binds only `127.0.0.1`, passes a random API key through the cleared child environment, polls `/health`, and requires authenticated `/props` to report the expected canonical model path and `b9986` build before declaring ready.
- Runtime commands expose status including the loaded context capacity, explicit health, start, stop, and bounded retained logs. Package uninstall is rejected while startup or a live session owns the runtime.
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
|   |   |   |-- chat-metrics.test.ts
|   |   |   |-- chat-metrics.ts
|   |   |   |-- conversation-history.test.ts
|   |   |   |-- conversation-history.ts
|   |   |   |-- model-selection.ts
|   |   |   |-- model-selection.test.ts
|   |   |   |-- prompt-selection.ts
|   |   |   `-- prompt-selection.test.ts
|   |   |-- hardware/HardwareView.tsx
|   |   |-- models/
|   |   |   |-- ModelManagerView.tsx
|   |   |   |-- model-format.ts
|   |   |   `-- model-format.test.ts
|   |   |-- prompts/PromptLibraryView.tsx
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
    |   |-- 0003_engine_packages.sql
    |   |-- 0004_prompt_library.sql
    |   |-- 0005_conversation_persistence.sql
    |   `-- 0006_conversation_branches.sql
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
        |   |-- conversation_commands.rs
        |   |-- engine_commands.rs
        |   |-- engine_package_commands.rs
        |   |-- hardware_commands.rs
        |   |-- model_commands.rs
        |   |-- prompt_commands.rs
        |   |-- settings_commands.rs
        |   `-- mod.rs
        |-- conversations/
        |   |-- repository.rs
        |   |-- service.rs
        |   |-- types.rs
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
        |-- prompts/
        |   |-- parser.rs
        |   |-- path_grants.rs
        |   |-- repository.rs
        |   |-- service.rs
        |   |-- types.rs
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
- `src-tauri/migrations/0004_prompt_library.sql` is migration version 4, name `prompt_library`; it adds profile timestamps, exact raw documents, duplicate provenance, and library indexes.
- `src-tauri/migrations/0005_conversation_persistence.sql` is migration version 5, name `conversation_persistence`; it adds draft/job/usage/terminal fields, deterministic positions, recovery timestamps, and persistence indexes.
- `src-tauri/migrations/0006_conversation_branches.sql` is migration version 6, name `conversation_branches`; it adds nullable branch/source provenance and indexes without modifying prior migrations.
- The runner creates `schema_migrations` defensively, checks each version, executes unapplied SQL in a transaction, then records the version and UTC timestamp.
- Tests run all migrations twice and upgrade a simulated version-1 database, confirming six ledger rows plus the model, package, prompt, persistence, and branch-provenance additions.

### Foundation schema

| Table | Purpose | Current code usage |
| --- | --- | --- |
| `schema_migrations` | Applied version, name, and timestamp | Active |
| `settings` | JSON settings by stable key | Active through `get_setting`/`put_setting` |
| `prompt_profiles` | Stable prompt identity, collection, pin, soft delete, timestamps | Active through `PromptRepository`/`PromptService` |
| `prompt_versions` | Immutable prompt content, hash, source, raw document, front matter, version, provenance | Active through `PromptRepository`/`PromptService` |
| `models` | Local model identity, type, path, size, verification, GGUF metadata, file identity | Active through `ModelRepository`/`ModelService` |
| `engine_packages` | Installed engine version/route/path, archive checksum, file inventory, state, errors, timestamps | Active through `EnginePackageRepository`/`EnginePackageService` |
| `conversations` | Chat identity, selected model/prompt/settings, and branch provenance | Active through `ConversationRepository`/`ConversationService` |
| `messages` | Ordered parent-linked messages, drafts, usage, terminal state, and copied-source provenance | Active through `ConversationRepository`/`ConversationService` |
| `downloads` | Resumable download state, byte counts, ETag, checksum, errors | Schema only |
| `benchmarks` | Hardware/engine/model benchmark results | Schema only |
| `outputs` | Generated file metadata and thumbnails | Schema only |
| `jobs` | Durable workload state, requests, results, and errors | Schema only |

Foreign keys connect prompt versions to profiles, conversations to models/prompt versions, and messages to conversations/parents. Deleting a conversation cascades to its messages. Unique constraints prevent duplicate model paths, duplicate prompt versions/hashes, and duplicate output paths.

Indexes exist for prompt library ordering/version history, pinned/updated conversation lists, deterministic conversation message order, unique non-null chat job IDs, model kind/verification/file identity, engine package state/route, download state/recency, output kind/recency, and benchmark lookup.

## Existing Tauri Commands

| Command | Input | Response | Current behavior |
| --- | --- | --- | --- |
| `get_app_snapshot` | none | `AppSnapshot` | Returns crate version, database ready, process count, and scaffold values for first-run/jobs |
| `list_engine_packages` | none | `EnginePackageStatus[]` | Returns bundled manifests joined with persisted installation state |
| `install_engine_package` | package ID | `EnginePackageRecord` | Downloads, verifies, safely extracts, inventories, and atomically installs the pinned package when internet access is enabled |
| `import_engine_package` | package ID and granted `.zip` path | `EnginePackageRecord` | Performs the same exact size/checksum/extraction/install flow for an offline archive |
| `verify_engine_package` | package ID | `EnginePackageRecord` | Verifies the exact installed file set, sizes, and SHA-256 inventory |
| `uninstall_engine_package` | package ID | none | Removes only the manifest-owned internal package directory and its database record |
| `get_engine_status` | none | `EngineRuntimeStatus` | Returns package/session lifecycle, process/model identity, backend version, loaded context capacity, exit metadata, and detail |
| `get_engine_health` | none | `EngineHealth` | Rechecks the owned loopback server and authenticated model/build identity |
| `start_engine` | model ID with optional context/threads | `EngineRuntimeStatus` | Reverifies model/package/binary, launches the CPU server, and waits for owned ready state |
| `stop_engine` | session ID | `EngineRuntimeStatus` | Stops only the matching retained owned session and returns its final state |
| `get_engine_logs` | session ID | `EngineLogSnapshot` | Returns the process manager's bounded, redacted retained lines |
| `start_chat_generation` | job/conversation/user/assistant/session IDs, prompt version, bounded messages, output limit | `ChatGenerationResult` | Persists the bound turn/draft first, streams token batches, then finalizes exact text, usage, and terminal state |
| `cancel_chat_generation` | job ID | boolean | Cancels only the matching active generation without stopping the loaded model |
| `list_conversations` | query, limit, offset | `ConversationSummary[]` | Returns deterministic pinned/recent summaries without loading message bodies |
| `get_conversation` | conversation ID | `ConversationDetail` | Lazily returns exact bindings/settings and ordered message content |
| `rename_conversation` | conversation ID and title | none | Validates and updates a durable title without loading messages |
| `set_conversation_pinned` | conversation ID and pin state | none | Updates durable pinned ordering without loading messages |
| `delete_conversation` | conversation ID | none | Deletes the conversation and cascades only its messages |
| `export_conversation` | conversation ID | `ConversationExport` | Returns a bounded Markdown transcript generated from durable messages with structured binding/settings provenance |
| `branch_conversation` | source/new conversation IDs and optional branch message ID | `ConversationDetail` | Transactionally clones the selected prefix with fresh IDs, remapped parents, copied bindings/settings, and source provenance |
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
| `list_prompts` | optional search query | `PromptSummary[]` | Returns up to 200 active latest-version summaries, pinned and recently updated first |
| `import_prompt` | granted absolute `.md`/`.txt` path | `PromptMutationOutcome` | Parses and creates a profile, appends a changed source as a version, or reports a hash no-op |
| `create_prompt` | stable name and document | `PromptMutationOutcome` | Validates an authored document and creates immutable version 1 |
| `save_prompt` | profile/base-version IDs and document | `PromptMutationOutcome` | Appends a version or rejects a stale base-version conflict |
| `get_prompt_version` | version ID | `PromptVersionRecord` | Returns exact immutable content, metadata, source, raw document, and provenance, including soft-deleted history |
| `duplicate_prompt` | version ID and optional name | `PromptMutationOutcome` | Creates a new profile with source profile/version provenance |
| `set_prompt_pinned` | profile ID and pin state | `PromptSummary` | Updates library ordering metadata |
| `delete_prompt` | profile ID | none | Soft-deletes the profile while retaining all historical versions |
| `export_prompt` | version ID and original/normalized mode | `PromptExport` | Returns an exact original document or explicit normalized front matter plus content |
| `compile_prompt` | version ID | `CompiledPrompt` | Returns exact system content with an explicitly approximate token estimate |

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

- Frontend: 6 Vitest files, 17 tests for adaptive byte formatting, model metadata, selector grouping/readiness, exact/approximate context metrics, immutable prompt/conversation restoration, persisted state mapping/order, inference-history filtering, and system-message ordering.
- Rust: 47 passing default tests plus two ignored opt-in integration tests. Conversation coverage includes transactional turn ordering, terminal usage/content finalization, restart interruption recovery, binding-conflict rollback, deterministic search/list metadata, pin/rename, bounded provenance export, title/filename hardening, transactional branch copying, parent remapping, empty-prefix retry branches, source-deletion survival, foreign branch-point rollback, foreign keys, and cascade deletion, alongside the prompt, chat, hardware, database, GGUF, model, process, and package suites.
- Opt-in package integration: the ignored test downloads the pinned official archive, completes install, runs the real `llama-server.exe --version --help` probe and observes build `9986`, verifies the exact files, and uninstalls in a temporary application-data directory; it passed on 2026-07-13.
- Opt-in real-model integration: an environment-selected `llama-server.exe` and tensor-bearing GGUF are loaded with the same adapter, health-checked, required to return a known visible answer with thinking disabled, usage-checked, cancellation-checked, stopped, and checked for zero owned child processes. It passed with Qwen3 4B Q4_K_M and `b9986` on 2026-07-14.

The Tauri debug build also runs the production frontend build as its configured pre-build command.

## Generated Executable

The verified unpackaged Windows debug executable is:

```text
C:\Users\atrx07\atrx\NeuraLoc-Core\src-tauri\target\debug\neuraloc-core.exe
```

Checkpoint size: 30,824,448 bytes, rebuilt on 2026-07-15 after the durable conversation-branch update. This is a debug executable, not a signed installer or release artifact. `src-tauri/target` is ignored by Git and can be regenerated.

## Known Warnings and Limitations

- `cargo clippy --all-targets` passes but reports expected dead-code warnings for scheduler and other interfaces that remain scaffolded.
- Chat currently compiles one selected user prompt layer. Prompt metadata recommendations, tool-policy/project/memory layers, and the full layer inspector remain planned; system prompts larger than the current 256 KiB chat message bound are rejected at selection time.
- Rust is installed under `%USERPROFILE%\.cargo\bin`; terminals that do not inherit that user PATH require the session PATH command shown above.
- npm may print a non-fatal `could not canonicalize path C:\Users\atrx07` warning in the current host environment.
- Hardware discovery is partial: no Vulkan loader enumeration, Intel iGPU details, disks, battery/power state, instruction-set report, OpenVINO runtime probe, or robust driver/runtime version inventory exists yet.
- CUDA readiness currently means `nvidia-smi` responded; a compatible llama.cpp CUDA package has not been installed or validated.
- The bundled engine catalog currently contains only llama.cpp `b9986` Windows x64 CPU. CUDA/Vulkan packages, resumable package downloads, package progress events, and package updates remain pending.
- NPU detection is name/text based through `pnputil`; model compatibility still requires a future OpenVINO compile probe.
- The current pinned llama.cpp server exposes no dedicated process-shutdown endpoint used by this adapter, so stop applies a 250 ms grace interval and then force-stops only the owned handle. Crash recovery/restart policy is not implemented.
- The scheduler is a resource classification scaffold, not a queue or durable job runner. `activeJobs` currently reflects only the active llama.cpp generation.
- Model scan, engine lifecycle/log, and chat token/usage/state events are sequenced and consumed. Early model-loading state is polled while `start_engine` awaits readiness; download events, general throttling, and broader stale-sequence handling remain unfinished.
- Settings, models, engine packages, prompts, and conversations/messages have repository/service implementations. General download, output, benchmark, and job repositories remain unfinished.
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
- Multi-layer prompt composition for tool policy, project instructions, memory, and the compiled-layer inspector. Markdown/text system-prompt import, versioning, editing, search, selector binding, and durable restoration are implemented.
- Retry, enforced context budgeting/strategies, model-template options, Markdown rendering, reasoning presentation, and engine crash recovery. Durable linear storage/history/restoration, startup interruption recovery, streaming, stop, and live context visibility are implemented.
- Chat branch/retry controls, retry generation, incremental draft checkpoints during very long generations, and pagination beyond the first 50 history summaries. The migration-backed branch command and repository-backed search/list/open/rename/pin/export/delete/final partial-response behavior are implemented.
- Signed model catalog, catalog refresh, recommendations, resumable downloads, verification, pause/retry, and installation.
- Image generation, speech recognition, text-to-speech, gallery, downloads, and logs are visual empty states only.
- OpenVINO, Vulkan, stable-diffusion.cpp, whisper.cpp, and Kokoro runtime adapters.
- Real scheduler queue, durable jobs, telemetry samples, benchmark execution, and hardware-aware routing.
- Portable data mode, diagnostics export/redaction, optional authenticated API, updater, release installer, and signing.

See `NEXT_STEPS.md` for the dependency-aware next-phase plan.
