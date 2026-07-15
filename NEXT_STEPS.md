# NeuraLoc-Core Next Steps

This plan covers the next checkpoint only: a usable local GGUF chat path with model/prompt selection, streaming, persistence, and a verified download catalog. It is ordered by dependency and avoids beginning image, speech, TTS, OpenVINO, or broad API work.

## Checkpoint Progress

Completed on 2026-07-13: shared model-library preparation, step 1 local discovery/import, and the basic bounded GGUF metadata portion of step 3. NeuraLoc-Core now has migration-backed model records, guarded native file/folder selection, recursive cancellable scans with sequenced progress events, path/file-identity deduplication, missing/invalid states, metadata-only removal, and a functional installed-model UI.

Step 2's CPU runtime gate and the basic Step 4/6 chat path are complete. On 2026-07-14 the concrete adapter loaded the user's Qwen3 4B Q4_K_M GGUF with pinned llama.cpp `b9986`, passed authenticated health/identity checks, streamed a bounded response with usage, cancelled a second request, stopped, and confirmed zero owned child processes. Chat now lists ready indexed models, remembers the last choice, loads/switches/unloads the selected model, gates the composer on the matching ready session, streams sequenced token batches, cancels the active generation, and shows the loaded context capacity with exact completed-turn and explicitly approximate in-progress usage. Step 5's secure prompt parser, immutable repository, migration, and typed command layer are complete. Immediate next work is the Prompt Library interface and immutable Chat prompt binding, then Step 7 conversation/message persistence, enforced context strategies, advanced selector fit behavior, and the verified catalog.

## Dependency Map

```text
Shared storage, IPC, event, path, and test conventions
    |
    +--> 1. Local model discovery/import
    |       |
    |       +--> 3. GGUF metadata detection ----+
    |                                           |
    +--> 2. llama.cpp install/lifecycle --------+--> 4. Model selector
    |                                           |        |
    +--> 5. Prompt Markdown import/selector ----+--------+--> 6. Streaming chat
    |                                                               |
    +---------------------------------------------------------------+--> 7. Conversation persistence
    |
    +--> verified downloader + catalog policy --> 8. Model download catalog
                                                        |
                                                        +--> reuse import/metadata pipeline
```

Steps 1 and 2 can proceed in parallel after the shared conventions are in place. Step 5 can proceed in parallel with steps 2 and 3. Steps 4, 6, 7, and 8 should respect the dependencies shown above.

## Shared Preparation (Completed for the Model Library)

Keep this preparation small and land it with step 1 rather than creating a separate architecture project.

- Add repositories beside `storage` for models, prompts, conversations, messages, downloads, and engine packages as they become active. Commands stay thin and do not contain SQL.
- Add migration `0002_*`; do not edit `0001_foundation.sql`. Include only fields/tables required by the next phase, such as model verification state, GGUF metadata JSON, file modification time/fingerprint, and installed engine package records.
- Introduce a central path/grant service before accepting renderer-provided paths. Canonicalize existing files, reject device/traversal paths, validate allowed extensions plus magic bytes, and distinguish metadata removal from file deletion.
- Add the Tauri dialog plugin only for user-selected files/folders. The renderer should receive granted paths through the dialog and pass typed requests to Rust; it should not gain general filesystem permissions.
- Turn `EventEnvelope` into an emitter utility with per-stream monotonically increasing sequence numbers. Add event names only when a feature consumes them.
- Add small fixtures: a minimal valid GGUF header, malformed/truncated files, a deterministic fake engine, and temporary SQLite databases.
- Establish command contract tests and repository tests before adding UI state.

## 1. Local Model Discovery and Import (Completed)

Dependencies: shared preparation.

### Backend

- Add `ModelRepository` and typed model records/statuses. Reuse the existing `models` table and extend it through migration 2 rather than replacing it.
- Implement commands such as `list_models`, `import_model`, `scan_model_folder`, `reverify_model`, and `remove_model_record`.
- Support individual `.gguf` files and recursive folder scanning with cancellation, bounded concurrency, and progress events.
- Perform cheap validation first: canonical file path, regular file, GGUF magic, size, readable header, modification time, and duplicate path/file identity.
- Store imports immediately with `metadata_pending`, `ready`, `invalid`, `missing`, or equivalent verification state so discovery does not block on full hashing.
- Stream SHA-256 in a bounded buffer only when required for catalog matching or explicit verification. Never load a model file into memory.
- Reconcile moved/missing files at startup or on explicit refresh without silently deleting metadata.

### Frontend

- Replace the Model Manager empty state with installed-model rows, search/filter, import file/folder commands, scanning progress, validation errors, and an explicit remove-record/delete-file choice.
- Keep catalog/recommended tabs disabled or clearly empty until step 8.

### Acceptance gate

- Importing or scanning a folder indexes valid GGUF files, rejects malformed/non-GGUF files with actionable errors, deduplicates paths, survives restart, and can be cancelled.
- Tests cover large-size handling without large fixtures, duplicates, missing files, symlinks/reparse points, malformed headers, cancellation, and migration from version 1.

## 2. llama.cpp Backend Installation and Lifecycle Management

Dependencies: shared preparation. It can be built alongside step 1.

### Package installation

- Define a versioned engine-package manifest with platform, architecture, acceleration route, source URL, expected files, size, and SHA-256. Ship one pinned known-good manifest for Windows x64 CPU and add CUDA only after validation. CPU manifest complete; CUDA remains pending.
- Add `engine_packages` metadata in migration 2 or 3 with version, route, install path, checksum, state, and verification timestamp. Completed in migration 3 with a complete installed-file inventory.
- Implement verified download to `.partial`, checksum validation, traversal-safe archive extraction, atomic install directory promotion, and cleanup/retry. Reuse this downloader in step 8. Completed for fresh package downloads; resume/progress remain catalog-checkpoint work.
- Add commands for package status, install/update, verify, and uninstall. Respect `internetAccess`; allow a manually selected package for offline setup. Status/install/offline import/verify/uninstall and Model Manager controls are complete; update selection awaits a second manifest.

### Process/lifecycle hardening

- Implement a concrete llama.cpp adapter behind `InferenceEngine` and `ChatEngine`. Lifecycle/health, bounded SSE streaming, usage extraction, and request cancellation are complete for the pinned CPU route.
- Harden `ProcessManager`: natural exits, lifecycle/exit metadata, bounded/redacted logs, typed IPC exposure, lifecycle/log events, owned probes, grace/force-stop, and shutdown cleanup are implemented. Crash recovery and protocol-level request draining remain.
- Launch by canonical executable path with fixed argument arrays and an allow-listed environment. Completed; the API key is environment-only and no shell is used.
- Reserve loopback port `0`, authenticate/identify the owned server, poll health with a bounded timeout, and never kill another process by name or port. Completed with a wrong-key challenge plus authenticated `/props` model/build checks.
- Implement explicit transitions through installed, starting, loading, ready, busy, stopping, stopped, crashed, and error states. Process-backed states are implemented; busy awaits generation and recovering awaits crash policy.
- Guarantee `stop_all` on application exit and add deterministic process tests for successful exit, log redaction, crash, timeout/force-stop, owned probes, and shutdown. An ignored environment-selected real-model load/stream/stop/no-orphan test is complete; a redistributable fixture plus load-cancel/crash cases remain.

### Acceptance gate

- A verified llama.cpp package installs, reports its pinned build, and is entirely Rust-controlled with health, ownership, logs, cancellation, and stop behavior. The gate passed locally with Qwen3 4B Q4_K_M on 2026-07-14, including streamed generation and confirmation that no owned child remained.

## 3. GGUF Metadata Detection (Basic Inspection Completed)

Dependencies: step 1. The preferred implementation may use the pinned llama.cpp inspection capability from step 2; otherwise select and pin a maintained GGUF parser after validating it against current GGUF versions.

- Parse only bounded header/metadata sections and reject impossible counts, offsets, strings, and allocation sizes.
- Extract architecture/family, parameter hints where available, quantization/file type, context length, embedding dimensions, layer count, tokenizer/chat template, model name, and vision/projector requirements.
- Preserve unknown metadata keys in bounded diagnostic JSON instead of failing valid newer files.
- Normalize metadata into typed columns used for filtering plus a versioned raw/normalized JSON payload for forward compatibility.
- Compute conservative RAM/VRAM estimates with explicit confidence and assumptions. Backend compatibility remains unknown until the installed engine validates it.
- Add `inspect_gguf`/reverify service behavior and update imported records transactionally.

### Acceptance gate

- Common GGUF variants display useful metadata without reading tensor data; malformed/truncated/hostile headers fail safely; newer unknown keys do not corrupt indexing.

## 4. Model Selector

Dependencies: steps 1 and 3; step 2 is required before a model can be marked runnable.

Basic selector status: completed on 2026-07-14. Chat is backed by persisted model records, groups ready and unavailable models, remembers the last selected ID separately from conversations, reuses the matching ready session, switches by stopping only the owned prior session, exposes Model Manager, and keeps the composer disabled until the selected session is ready. Advanced fit groups, projector/backend explanations, load estimates, and Rust-side preference persistence remain.

- Build a reusable selector backed by model summaries, not raw database rows.
- Show display name, family, quantization, size, context, verification state, installed backend readiness, estimated fit, and missing-projector warnings.
- Group choices as Ready, Tight/Experimental, Missing backend, Missing file, and Invalid. Do not allow incompatible models to start silently.
- Persist a global last-used model preference separately from each conversation's immutable model binding.
- Connect the selector to Chat and expose an install/import action without navigating away from user context.
- On selection, request a load estimate and then start/load through the engine service with visible progress and cancellation.

### Acceptance gate

- Selection survives restart, explains disabled options, reacts to model/backend/hardware changes, and never enables the composer until a verified model session is ready.

## 5. System-Prompt Markdown Import and Selector

Dependencies: shared repository/path work from step 1. Can run in parallel with steps 2 and 3.

Backend status: completed on 2026-07-15. Migration 4, the bounded YAML 1.2 parser, path/dialog grant checks, immutable profile/version repository, duplicate/no-op detection, provenance, soft deletion, original/normalized export, exact-content compilation, and typed Tauri commands are implemented and covered by eight prompt tests. The Prompt Library interface and Chat selector/binding are the active slice.

### Import and persistence

- Implement `PromptRepository` over existing `prompt_profiles` and `prompt_versions` tables.
- Add typed commands for list/search, import, create, update-as-new-version, get version, duplicate, soft delete, and export.
- Support UTF-8 `.md` and `.txt`, optional BOM, and preserved source line endings/content.
- Parse only a leading `---` YAML front-matter block with a maintained parser configured for bounded input/nesting and aliases disabled where supported.
- Validate known metadata, preserve unknown keys as inert data, hash canonical validated metadata plus exact prompt content, and make byte-equivalent reimport a no-op.
- Never allow prompt metadata to enable tools, network, model downloads, or filesystem access.

### Selector

- Replace the hard-coded prompt dropdown with profile/version summaries, search, pinned/recent items, a default no-custom-prompt option, and import/manage actions.
- Bind a selected immutable prompt version to a new conversation. Prompt changes in an existing conversation require an explicit new-version/new-conversation/branch decision.

### Acceptance gate

- Markdown/text imports round-trip, versions are immutable, duplicate hashes are rejected cleanly, malformed YAML is actionable, historical conversation references remain readable, and prompt selection sits beside model selection.

## 6. Streaming Chat

Dependencies: steps 2, 4, and 5 plus working event sequencing.

Basic streaming status: completed on 2026-07-14 using the no-custom-prompt path, with context visibility added on 2026-07-15. Rust owns the authenticated llama.cpp transport, validates bounded messages/output limits, parses bounded SSE data, batches sequenced token events, emits usage and terminal state, and cancels one active generation. Chat renders ephemeral user/assistant messages, ignores stale event sequences, and presents the loaded context capacity plus exact or explicitly approximate live usage. Prompt compilation, durable jobs/messages, retry, enforced context management, OOM fallback, and crash recovery remain before the full acceptance gate is complete.

### Backend

- Implement `start_chat_generation` and `cancel_chat_generation` around a durable job ID. Rust owns all llama.cpp transport; the renderer never calls the loopback server directly.
- Compile the selected system-prompt version and conversation messages into the model's chat template without hidden personality text.
- Stream token chunks through `chat://token` envelopes containing conversation ID, message ID, job ID, and sequence. Batch small chunks on a short interval to protect renderer performance.
- Emit state, usage, completion, cancellation, and structured error events. Ignore or reject stale job IDs and guarantee a single terminal event.
- Enforce output/context limits and bounded request sizes. Add explicit cancellation and a conservative one-time OOM fallback only after the basic path is reliable.

### Frontend

- Enable the composer only for a ready model session.
- Render user/assistant messages, incremental output, stop/retry controls, load/generation status, structured errors, and tokens-per-second/usage after completion.
- Keep token buffers keyed by job/message and ignore duplicate or stale sequences.

### Acceptance gate

- First token arrives incrementally, cancellation stops generation promptly, the UI stays responsive, duplicate/out-of-order events do not duplicate text, and engine crashes produce a recoverable visible error.

## 7. Conversation Persistence

Dependencies: step 6 contracts, step 4 model identity, and step 5 prompt-version identity. Implement the repository before declaring streaming chat complete so messages are crash recoverable.

- Add `ConversationRepository` and `MessageRepository` using the existing tables, with explicit transactions for conversation creation, user messages, assistant draft/finalization, branch parentage, and timestamps.
- Persist the user message before launching generation. Create an assistant draft tied to the job, append/finalize in bounded checkpoints or persist a final content record, and mark interrupted drafts on startup.
- Store exact model ID, prompt version ID, effective generation settings, context strategy, token counts, usage, and terminal reason.
- Implement list/page/search, open, rename, pin, delete, branch, retry, and export commands.
- Load conversation history lazily and keep large content out of global Zustand state.
- Decide deletion semantics explicitly: deleting a conversation cascades messages; deleting model/prompt metadata must preserve readable historical references or be blocked/soft-deleted.

### Acceptance gate

- Conversations and branches survive restart, partial generations are identifiable/recoverable, pagination is deterministic, foreign-key behavior is tested, and exports preserve role/content/settings provenance.

## 8. Model Download Catalog

Dependencies: step 1 import pipeline, step 3 metadata validation, the verified downloader from step 2, and the existing privacy setting.

- Define a versioned JSON catalog schema containing exact asset URL, filename, format, quantization, size, SHA-256, license, family, context, projector relationship, engine/version constraints, and conservative memory coefficients.
- Bundle a last-known-good catalog and verify remote catalog signatures before replacing it. A failed refresh must retain the previous verified catalog.
- Restrict downloads to HTTPS and approved hosts, cap redirects, reject private/link-local destinations, check disk reserve, and require explicit user action while `internetAccess` is enabled.
- Use the existing `downloads` table for queued/downloading/paused/verifying/completed/failed states, HTTP range resume, ETag checks, progress events, retry, and partial cleanup.
- After checksum verification, atomically move the file into the model library and invoke the same import/GGUF metadata pipeline used for local files.
- Build catalog UI with search/filter, license/size/quantization, hardware fit explanation, installed state, download progress, pause/retry, and no unsupported popularity claims.
- Start with a small curated GGUF list and document catalog signing/key-rotation procedure before enabling remote refresh by default.

### Acceptance gate

- A catalog model can download, pause, resume after restart, verify, install, index, and appear in the model selector. Tampered assets, bad signatures, insufficient disk, changed ETags, and network loss fail safely and visibly.

## Recommended Checkpoints

1. **Model library checkpoint (completed 2026-07-13):** shared preparation + local discovery/import + basic GGUF metadata.
2. **Runtime checkpoint (completed 2026-07-14 for CPU):** verified llama.cpp install + lifecycle/logging + real Qwen load/stream/stop/no-orphan validation.
3. **Prompt checkpoint (next):** Markdown import/versioning + prompt selector; the model selector is already connected.
4. **Chat checkpoint (in progress):** basic streaming/cancellation is complete; conversation persistence, context management, retry, and crash recovery remain.
5. **Catalog checkpoint:** signed metadata + resumable verified downloads.

At every checkpoint run frontend build/tests, Rust format/check/test/clippy, migration tests from empty and prior schemas, fake-engine lifecycle tests where applicable, and a Tauri debug smoke test. Do not move to the next checkpoint with orphaned processes, destructive migration changes, unbounded file reads, or renderer-owned native/network access.
