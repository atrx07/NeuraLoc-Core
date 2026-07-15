# Architecture

## System shape

NeuraLoc-Core is a Tauri v2 application with a React/TypeScript renderer and a Rust core. UI communication uses Tauri commands and events. Native engines may expose loopback HTTP internally because upstream binaries require it, but that transport is owned by the Rust engine adapter and is never the normal UI boundary.

```text
React features and stores
        |
        | typed Tauri commands / events
        v
Command layer -> domain services -> scheduler -> engine adapters
        |                |                |
        |                |                `-> centralized process manager
        |                `-> hardware capability matrix
        `-> repositories -> SQLite metadata + ordinary model/output files
```

## Repository structure

```text
NeuraLoc-Core/
|-- src/
|   |-- app/                    # shell, routes, layout, providers
|   |-- components/             # reusable UI primitives
|   |-- features/
|   |   |-- chat/
|   |   |-- images/
|   |   |-- models/
|   |   |-- prompts/
|   |   |-- speech/
|   |   |-- tts/
|   |   |-- gallery/
|   |   |-- downloads/
|   |   |-- hardware/
|   |   |-- logs/
|   |   `-- settings/
|   |-- hooks/
|   |-- services/               # typed IPC facade only
|   |-- stores/                 # small domain stores
|   |-- types/
|   `-- utils/
|-- src-tauri/
|   |-- capabilities/
|   |-- migrations/
|   |-- src/
|   |   |-- commands/
|   |   |-- engines/
|   |   |-- hardware/
|   |   |-- processes/
|   |   |-- scheduler/
|   |   |-- models/
|   |   |-- prompts/
|   |   |-- chat/
|   |   |-- storage/
|   |   |-- telemetry/
|   |   |-- api/
|   |   |-- security/
|   |   |-- app_state.rs
|   |   |-- errors.rs
|   |   |-- lib.rs
|   |   `-- main.rs
|   |-- Cargo.toml
|   `-- tauri.conf.json
|-- catalog/                    # versioned, signed catalog data
|-- fixtures/                   # small test models and fake engines
|-- docs/
|-- PROJECT.md
|-- ARCHITECTURE.md
|-- SECURITY.md
|-- HARDWARE_ACCELERATION.md
|-- PROMPT_SYSTEM.md
|-- MODEL_CATALOG.md
|-- DEVELOPMENT.md
`-- ROADMAP.md
```

## Rust core boundaries

### Commands

Commands deserialize and validate IPC input, call one domain service, and map domain errors to stable IPC errors. Commands do not spawn processes, query arbitrary files, or contain model-family rules.

### Engine adapters

All inference engines implement a shared lifecycle and expose workload-specific traits where needed.

```rust
#[async_trait]
pub trait InferenceEngine: Send + Sync {
    fn engine_id(&self) -> &'static str;
    fn capabilities(&self) -> EngineCapabilities;
    async fn prepare(&self, config: EngineConfig) -> EngineResult<()>;
    async fn start(&self, request: EngineStartRequest) -> EngineResult<EngineHandle>;
    async fn stop(&self) -> EngineResult<()>;
    async fn health(&self) -> EngineResult<EngineHealth>;
}

#[async_trait]
pub trait ChatEngine: InferenceEngine {
    async fn load_model(&self, request: ChatModelRequest) -> EngineResult<ModelSession>;
    async fn generate(&self, request: ChatRequest, sink: TokenSink) -> EngineResult<Usage>;
    async fn cancel(&self, job_id: JobId) -> EngineResult<()>;
}
```

`EngineHandle` contains an opaque owned-process ID, selected endpoint if required, backend version, and lifecycle state. It never exposes a raw mutable child handle to callers.

### Process manager

The process manager is the sole child-process entry point. It accepts executable paths and argument arrays, assigns an ownership token, captures bounded logs, tracks start time and PID, and performs graceful then forced shutdown only for its own live handles. It never kills by port number or executable name.

### Scheduler

Jobs enter a bounded queue with workload, compatible devices, estimated memory, priority, cancellation token, and persistence policy. The resource policy selects a device and decides whether another model must unload. The scheduler cannot override engine compatibility.

### Storage

Repositories own SQL and return domain records. Migrations are ordered, transactional, and recorded in `schema_migrations`. Files are committed with write-to-temporary, flush, checksum where relevant, and atomic rename.

## State ownership

`AppState` contains thread-safe handles to services, not mutable business records:

- `Database`
- `ConversationService`
- `HardwareService`
- `ProcessManager`
- `Scheduler`
- `SettingsService`
- `PromptService`
- engine registry
- cancellation registry

React state is split into UI session state, persisted user settings, and server-derived snapshots. Zustand stores own only their feature state. TanStack Query may be introduced for cached IPC reads once request volume warrants it.

The mounted Chat workspace holds the active renderer view, while Rust owns the durable conversation record. `start_chat_generation` transactionally creates the conversation plus user message and assistant draft before inference, accumulates the exact streamed assistant text, and finalizes content, usage, and terminal state before emitting completion. Startup marks any remaining drafts as interrupted. The selected model and immutable prompt version are conversation bindings; the history UI loads their records lazily instead of placing all message content in global state. Conversation export is also Rust-owned: it reads the durable record, emits a bounded Markdown transcript, and embeds structured model, prompt, context, and generation-setting provenance. Branching transactionally copies a selected prefix into a new conversation, assigns new message IDs, remaps parent links, clears historical job IDs, and records source provenance. Source deletion nulls provenance links but cannot cascade into the independent branch.

## SQLite schema

SQLite runs in WAL mode with foreign keys enabled and a busy timeout. Large files stay outside the database.

```sql
CREATE TABLE schema_migrations (
  version INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  applied_at TEXT NOT NULL
);

CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE prompt_profiles (
  id TEXT PRIMARY KEY,
  stable_name TEXT NOT NULL,
  collection TEXT,
  pinned INTEGER NOT NULL DEFAULT 0,
  deleted_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE prompt_versions (
  id TEXT PRIMARY KEY,
  profile_id TEXT NOT NULL REFERENCES prompt_profiles(id),
  version INTEGER NOT NULL,
  source_path TEXT,
  source_hash TEXT NOT NULL,
  front_matter_json TEXT NOT NULL,
  content TEXT NOT NULL,
  raw_document TEXT NOT NULL,
  source_profile_id TEXT REFERENCES prompt_profiles(id),
  source_version_id TEXT REFERENCES prompt_versions(id),
  created_at TEXT NOT NULL,
  UNIQUE(profile_id, version),
  UNIQUE(profile_id, source_hash)
);

CREATE TABLE models (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  display_name TEXT NOT NULL,
  family TEXT,
  format TEXT NOT NULL,
  path TEXT NOT NULL UNIQUE,
  size_bytes INTEGER NOT NULL,
  sha256 TEXT,
  compatibility_json TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  last_verified_at TEXT,
  verification_state TEXT NOT NULL DEFAULT 'metadata_pending',
  verification_error TEXT,
  gguf_metadata_json TEXT NOT NULL DEFAULT 'null',
  modified_at_unix_ms INTEGER NOT NULL DEFAULT 0,
  file_identity TEXT
);

CREATE TABLE conversations (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  model_id TEXT REFERENCES models(id),
  prompt_version_id TEXT REFERENCES prompt_versions(id),
  generation_settings_json TEXT NOT NULL,
  context_strategy TEXT NOT NULL,
  pinned INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  source_conversation_id TEXT REFERENCES conversations(id) ON DELETE SET NULL,
  branch_message_id TEXT REFERENCES messages(id) ON DELETE SET NULL
);

CREATE TABLE messages (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  parent_id TEXT REFERENCES messages(id),
  role TEXT NOT NULL,
  content_json TEXT NOT NULL,
  token_count INTEGER,
  pinned INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'complete',
  job_id TEXT,
  usage_json TEXT,
  terminal_reason TEXT,
  position INTEGER,
  updated_at TEXT,
  source_message_id TEXT REFERENCES messages(id) ON DELETE SET NULL
);

CREATE TABLE downloads (
  id TEXT PRIMARY KEY,
  catalog_entry_id TEXT,
  url TEXT NOT NULL,
  destination TEXT NOT NULL,
  partial_path TEXT NOT NULL,
  expected_sha256 TEXT NOT NULL,
  total_bytes INTEGER,
  received_bytes INTEGER NOT NULL DEFAULT 0,
  etag TEXT,
  state TEXT NOT NULL,
  error_json TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE benchmarks (
  id TEXT PRIMARY KEY,
  hardware_fingerprint TEXT NOT NULL,
  engine_id TEXT NOT NULL,
  engine_version TEXT NOT NULL,
  model_hash TEXT NOT NULL,
  settings_json TEXT NOT NULL,
  metrics_json TEXT NOT NULL,
  stable INTEGER NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE outputs (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  file_path TEXT NOT NULL UNIQUE,
  thumbnail_path TEXT,
  source_job_id TEXT,
  metadata_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE jobs (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  state TEXT NOT NULL,
  engine_id TEXT,
  device_id TEXT,
  request_json TEXT NOT NULL,
  result_json TEXT,
  error_json TEXT,
  created_at TEXT NOT NULL,
  started_at TEXT,
  finished_at TEXT
);
```

Indexes cover conversation update time, message conversation/time, download state, output kind/time, model kind, and benchmark lookup dimensions.

## IPC contract

Commands are versioned at the Rust type level. Breaking payload changes create a new command name during migration.

| Command | Request | Response |
| --- | --- | --- |
| `get_app_snapshot` | none | settings summary, running engines, active jobs |
| `get_hardware_snapshot` | none | devices, memory, disks, capabilities, warnings |
| `refresh_hardware` | none | fresh hardware snapshot |
| `get_settings` | none | complete non-secret settings |
| `update_settings` | typed patch | validated settings |
| `list_models` | none | model summaries |
| `import_model` | dialog-granted GGUF path | indexed model and duplicate state |
| `scan_model_folder` | scan ID and dialog-granted folder | scan summary |
| `cancel_model_scan` | scan ID | accepted/current state |
| `reverify_model` | model ID | updated model summary |
| `remove_model_record` | model ID | metadata removal result |
| `get_engine_status` | none | package/session lifecycle, identities, backend version, and loaded context capacity |
| `get_engine_health` | none | owned loopback readiness/identity result |
| `start_engine` | model ID, optional context/threads | ready engine status |
| `stop_engine` | engine session ID | final engine status |
| `get_engine_logs` | engine session ID | bounded redacted log snapshot |
| `start_chat_generation` | job/conversation/user/assistant/session IDs, prompt version, bounded messages, output limit | durable draft/final state plus non-thinking streamed text and usage |
| `cancel_chat_generation` | job ID | cancellation accepted |
| `submit_job` | typed job request | job ID |
| `cancel_job` | job ID | accepted/current state |
| `list_prompts` | search/filter | prompt summaries |
| `import_prompt` | granted path | profile and immutable version |
| `create_prompt` | stable name and document | profile and immutable version 1 |
| `save_prompt` | profile ID/base version/content | new immutable version |
| `get_prompt_version` | immutable version ID | exact source, content, metadata, and provenance |
| `duplicate_prompt` | source version and optional name | new profile/version with provenance |
| `set_prompt_pinned` | profile ID and pin state | updated prompt summary |
| `delete_prompt` | profile ID | soft-delete result |
| `export_prompt` | version ID and original/normalized mode | file name and document content |
| `compile_prompt` | immutable version ID | exact selected-prompt content and approximate token estimate |
| `list_conversations` | search/page | conversation summaries |
| `get_conversation` | conversation ID | binding/settings plus ordered messages |
| `rename_conversation` | conversation ID and title | mutation result without message bodies |
| `set_conversation_pinned` | conversation ID and pin state | mutation result without message bodies |
| `delete_conversation` | conversation ID | cascade deletion result |
| `export_conversation` | conversation ID | bounded Markdown file name, media type, transcript, and provenance |
| `branch_conversation` | source/new conversation IDs and optional branch message | independent copied branch with remapped message parentage and source provenance |
| `get_diagnostics` | redaction level | diagnostics bundle preview |

Events use `{ eventVersion, sequence, emittedAt, payload }` envelopes. Model scan progress plus engine state/log events are implemented; the remaining names are contracts for later phases:

- `hardware://updated`
- `engine://state-changed`
- `engine://log-line`
- `job://state-changed`
- `job://progress`
- `model://scan-progress`
- `chat://token`
- `chat://usage`
- `chat://state-changed`
- `download://progress`
- `download://state-changed`
- `telemetry://sample`
- `settings://changed`

Streaming events include a job ID and monotonically increasing sequence. The UI ignores duplicate or stale sequences. High-frequency telemetry is throttled in Rust; tokens may be batched for renderer efficiency.

## Lifecycle model

Valid engine transitions are explicit:

```text
NotInstalled -> Installed -> Starting -> LoadingModel -> Ready -> Busy
Busy -> Ready
Ready|Busy -> Stopping -> Stopped
Starting|LoadingModel|Ready|Busy -> Crashed -> Recovering -> Starting
Any active state -> Error
```

Recovery is bounded. A crash may restart at most twice within ten minutes by default. OOM fallback is a separate, user-configurable policy and records every changed setting.

## Internal transport and ports

Adapters reserve a loopback port by binding port `0`, retaining the listener until immediately before spawn where backend behavior permits. The llama.cpp adapter binds only `127.0.0.1`, passes a random API key through the cleared child environment, waits on `/health`, challenges `/props` with a wrong key, then requires the correct key to report the canonical selected model and pinned build. Port conflicts retry without killing the occupant; the renderer never receives the endpoint or token.

## Testing architecture

Unit tests cover pure policy and parsing. Integration tests launch copied deterministic executables for bounded logs, probes, natural exit, crash, and forced stop. One ignored network test downloads the official pinned package and verifies its real build output and file lifecycle. A second ignored environment-selected test loads a real tensor-bearing GGUF, checks health, requires a known visible answer with model thinking disabled, streams usage, cancels a second active request, stops the server, and asserts that no owned child remains; it passed locally with Qwen3 4B Q4_K_M on 2026-07-14. Hardware probes support injected fixtures so CI does not require a GPU or NPU. Database tests use temporary files and run every migration from an empty and previous-version database. A small redistributable tensor-bearing GGUF is still needed before real-model coverage can run in normal CI.
