# Security

## Trust boundaries

NeuraLoc-Core separates five trust levels:

1. The signed application and bundled catalog.
2. Native engines installed from verified release assets.
3. User-selected local files and folders.
4. Downloaded model files and metadata.
5. Web content and model-generated text, which are always untrusted data.

System prompts and model output cannot grant permissions, alter application policy, or call tools without a Rust-side permission decision.

## IPC

- Normal desktop communication uses Tauri IPC and opens no application HTTP port.
- Every command accepts a typed payload and validates lengths, enum values, identifiers, and paths.
- Tauri capabilities grant the renderer only the commands it needs.
- The webview does not receive raw process handles, database handles, secrets, or unrestricted filesystem APIs.
- Content Security Policy permits packaged assets and required Tauri protocols; remote script execution is prohibited.

## Filesystem policy

All paths cross a central path-policy service. It canonicalizes the nearest existing ancestor, rejects traversal and device paths, checks the intended operation against a user grant, and validates the final destination remains under the authorized root.

Imported files are opened as data. File names are normalized, reserved Windows names are rejected, and extensions are only an initial filter. Basic magic bytes and container structure are inspected before indexing.

The application never loads an entire multi-gigabyte model merely to hash it. Checksums stream through a bounded buffer. Writes use a sibling temporary file and atomic rename.

## Process policy

- Only the process manager may start native executables.
- Executables are addressed by canonical path and arguments are passed as arrays.
- User text is never interpolated into a command shell.
- Environment variables are allow-listed per adapter.
- Every process receives an ownership ID and is tracked by PID plus creation metadata.
- Shutdown acts only on tracked handles; NeuraLoc-Core never kills by image name or occupied port.
- Captured output is bounded, encoded defensively, and scrubbed before export.

## Downloads and model supply chain

- The model catalog is versioned and signature-verified before use.
- Entries point to explicit assets and include expected size and SHA-256.
- Downloads use `.partial` files, enforce disk reserves, support HTTP range resume, and verify the complete checksum before atomic installation.
- Redirects are limited and each destination must use HTTPS and pass the host policy.
- Repository code is never downloaded or executed. `trust_remote_code` is never enabled.
- Archive extraction rejects absolute paths, traversal, symlinks, excessive file counts, and decompression bombs.
- Failed verification quarantines or removes the partial asset and produces a specific error.

## Optional local API

The API is disabled by default. Enabling it binds `127.0.0.1` unless the user separately confirms LAN mode. LAN mode displays the exposure and requires authentication configuration. API keys are generated with cryptographic randomness, stored in Windows Credential Manager, and shown only at creation or rotation.

The API has request-size limits, rate limits, bounded concurrency, explicit CORS policy, and no administrative model/download/file commands in the initial version.

## Network access

Network operations are classified as catalog refresh, model download, optional web search, or user-approved URL open. The privacy dashboard shows each class and its current state. There is no analytics endpoint.

Web-search content is length-limited, labeled with source boundaries, and inserted as untrusted context below the system prompt. Retrieved text cannot change tool permissions or application policy. Private, loopback, link-local, and metadata-service addresses are blocked to prevent SSRF.

## Tool permissions

Permissions bind an exact tool, scope, conversation or project, and expiry:

- Ask every time
- Allow for this conversation
- Allow for this project
- Deny

File writes require a selected destination root. Command execution requires an executable and argument preview and is not part of the default tool set. Prompt imports never create grants.

## Secrets, logs, and diagnostics

Secrets use the OS credential store. Logs rotate by size and age, and sensitive values are redacted before persistence. Exported diagnostics default to replacing user names and absolute paths with stable placeholders. The preview is shown before export.

## Threats explicitly addressed

| Threat | Control |
| --- | --- |
| Path traversal | canonical path policy and authorized-root containment |
| Malicious model repository | explicit assets only, no repository code execution |
| Tampered download | catalog signature, expected size, SHA-256 verification |
| Prompt injection | retrieved/model text remains untrusted; permissions enforced in Rust |
| Port collision | choose another port; never kill unrelated processes |
| Orphan process | ownership registry, exit cleanup, stale-process validation |
| Secret leakage | OS credential store and diagnostic redaction |
| Renderer compromise | least-privilege Tauri capabilities and strict CSP |
| LAN exposure | loopback default, warning plus explicit opt-in and auth |

## Reporting

Security issues should include the NeuraLoc-Core version, reproduction steps, and a redacted diagnostics bundle. Do not attach model files, API keys, private prompts, or conversation databases.
