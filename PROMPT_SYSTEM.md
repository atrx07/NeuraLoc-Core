# Prompt System

## Goals

System prompts are first-class, versioned documents. NeuraLoc-Core preserves imported text, makes every composed layer visible, and binds conversations to immutable prompt versions.

## Import format

Supported files are UTF-8 `.md` and `.txt`. UTF-8 BOM is removed. Other invalid encodings produce an actionable error rather than replacement characters. Line endings are preserved in the stored source content.

Markdown may begin with YAML front matter delimited by `---` on its own line. Only the first leading block is metadata. The parser uses a real YAML parser with aliases disabled and limits on document size and nesting.

Supported metadata:

```yaml
name: Deep Reasoning Profile
version: 1.0
description: Analytical profile for architecture and coding
tags: [coding, reasoning]
recommended_models: [qwen, llama]
temperature: 0.7
top_p: 0.9
top_k: 40
context_reserve: 4096
collection: Engineering
```

Unknown keys are preserved in `extra` metadata and shown during import. They do not silently become application settings. Metadata values are schema-validated and bounded.

Without front matter, the filename stem becomes the display name and the entire file becomes prompt content. Default generation values remain inherited rather than being copied into the prompt.

## Identity and versioning

A prompt profile is a stable identity used for search, tags, collections, and favorites. Its versions are immutable content records.

- New import creates a profile and version 1.
- Reimporting byte-equivalent parsed content is a no-op detected by SHA-256.
- Editing creates the next integer version in one transaction.
- Imported source text is never rewritten in place.
- Deleting a profile soft-deletes it; versions referenced by conversations remain readable.
- Duplicate creates a new profile whose first version records its source profile/version.
- Export can emit original source or normalized front matter only with an explicit choice.

The source hash covers exact prompt content plus canonicalized validated metadata. The original raw document may also be retained as a file when the user enables source preservation.

## Composition

The compilation order is fixed and inspectable:

1. Application tool-policy layer, when tools are enabled.
2. Selected user prompt version.
3. Optional project instructions.
4. Optional memory/context layer.
5. Conversation messages.
6. Current user message.

Only layers 1-4 form the system content sent to engines that support one system role. For templates with different role semantics, the chat-template adapter maps the same logical layers without changing their text.

The Compiled Prompt panel shows layer boundaries, exact resulting content, token estimates by layer, source version IDs, and enabled state. Optional application layers can be disabled unless required to enforce an enabled tool's security protocol. Security is enforced outside the prompt regardless.

No hidden personality text is injected.

## Selection and conversation binding

The Chat toolbar always shows adjacent model and system-prompt selectors. A conversation stores both `prompt_profile_id` indirectly through `prompt_version_id` and the exact immutable version ID.

Changing prompts in an existing conversation offers three explicit actions:

- Apply the new version from the next user message and record a prompt-change marker.
- Start a new conversation with the selected version.
- Duplicate the current branch and bind the duplicate to the selected version.

Historical messages are never retroactively relabeled with a new prompt.

## Generation defaults

Prompt metadata may recommend temperature, top-p, top-k, context reserve, and model families. Resolution order is:

```text
explicit per-message override
> conversation settings
> selected prompt recommendations
> model defaults
> application defaults
```

The UI identifies the source of every effective setting. Prompt recommendations cannot force a model download, device selection, tool permission, or network access.

## Token estimation

The selected model's tokenizer is authoritative when available. Before an engine is loaded, NeuraLoc-Core uses a family-specific tokenizer package or labels a conservative character-based estimate as approximate. The context manager reserves output and tool-result space before admitting history.

## Parsing and validation tests

Tests cover BOM/no-BOM files, CRLF preservation, absent and malformed front matter, YAML limits, unknown keys, duplicate hashes, concurrent edits, immutable historical versions, composition order, disabled layers, metadata precedence, and conversation prompt changes.
