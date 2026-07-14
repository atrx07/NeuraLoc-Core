# NeuraLoc-Core

NeuraLoc-Core is a privacy-first Windows desktop application for discovering, managing, and running local AI models through verified native inference engines. The application uses React and TypeScript for the interface, Tauri 2 for the desktop boundary, Rust for orchestration, and SQLite for durable metadata.

Current version: `0.1.0`, local-chat checkpoint in progress. Hardware/settings functionality, local GGUF indexing, bounded metadata inspection, the verified pinned llama.cpp Windows x64 CPU package, owned model launch/stop, loopback health/identity checks, the Chat model selector, bounded streaming generation/cancellation, usage events, and retained logs are implemented. A real opt-in Qwen3 4B load/stream/stop test passed on 2026-07-14. Prompt persistence, durable conversation history, advanced context handling, and the download catalog remain ahead. See `STATUS.md` for the exact implementation state and `NEXT_STEPS.md` for the dependency-aware plan.

## Requirements

- Windows 11
- Node.js 22 or newer (verified with `v24.18.0`)
- npm (verified with `11.16.0`)
- Rust stable MSVC toolchain (verified with Rust/Cargo `1.97.0`)
- Microsoft Visual Studio 2022 Build Tools with Desktop development with C++ and a Windows SDK
- Microsoft Edge WebView2 Runtime

Verify the native toolchain in a new PowerShell window:

```powershell
rustc --version
cargo --version
rustup show active-toolchain
node --version
npm.cmd --version
```

The Rust toolchain should end in `x86_64-pc-windows-msvc`. If `rustc` or `cargo` is not found, add `%USERPROFILE%\.cargo\bin` to the user `PATH`, restart PowerShell/Codex, or set it for the current session:

```powershell
$env:Path = "$HOME\.cargo\bin;$env:Path"
```

## Install

From the standalone project directory:

```powershell
Set-Location C:\Users\atrx07\atrx\NeuraLoc-Core
npm.cmd ci
```

`npm ci` uses the committed lock file and is preferred for a reproducible checkout. Use `npm.cmd install` only when intentionally changing JavaScript dependencies.

## Run the Desktop App

```powershell
Set-Location C:\Users\atrx07\atrx\NeuraLoc-Core
$env:Path = "$HOME\.cargo\bin;$env:Path"
npm.cmd run tauri -- dev
```

Tauri starts Vite on `http://localhost:1420` and opens the native NeuraLoc-Core window. Port 1420 is configured as strict, so stop another process using it before launching development mode.

## Browser UI Preview

```powershell
npm.cmd run dev
```

Open `http://localhost:1420`. Browser mode uses representative hardware data and in-memory settings. It does not test native hardware probes, SQLite, child processes, filesystem access, or Tauri IPC.

## Verification

Run the frontend checks:

```powershell
npm.cmd run build
npm.cmd run test
```

Run the Rust checks:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
```

Clippy currently reports expected dead-code warnings for future engine, scheduler, event, and process interfaces; it exits successfully.

Build an unpackaged Windows debug executable:

```powershell
$env:Path = "$HOME\.cargo\bin;$env:Path"
npm.cmd run tauri -- build --debug --no-bundle
```

Output:

```text
C:\Users\atrx07\atrx\NeuraLoc-Core\src-tauri\target\debug\neuraloc-core.exe
```

Build the configured NSIS release bundle when release packaging is needed:

```powershell
$env:Path = "$HOME\.cargo\bin;$env:Path"
npm.cmd run tauri -- build
```

Release packaging is not yet code-signed and should not be treated as a production distribution.

## Data and Privacy

On first native launch, NeuraLoc-Core resolves the platform application-data directory and creates `neuraloc-core.db` plus workload-specific folders under `models` and `outputs`, along with `prompts`, `downloads`, `cache`, and `logs`. SQLite uses WAL mode and foreign keys. Browser preview does not use this data directory or native model imports.

Normal desktop communication uses Tauri IPC. Network features default off, and no local API server is currently implemented. Models and large outputs remain ordinary files; SQLite stores metadata.

## Project Guide

- `project.md`: product goals, scope, principles, and delivery definition.
- `STATUS.md`: implemented behavior, architecture, schema, commands, tests, warnings, executable, and unfinished work.
- `NEXT_STEPS.md`: ordered next-phase implementation plan.
- `ARCHITECTURE.md`: intended system boundaries and contracts.
- `SECURITY.md`: trust boundaries and security policy.
- `HARDWARE_ACCELERATION.md`: detection, fit estimates, and routing policy.
- `PROMPT_SYSTEM.md`: prompt import, versioning, composition, and binding design.
- `MODEL_CATALOG.md`: catalog and supply-chain design.
- `DEVELOPMENT.md`: concise contributor commands and engineering rules.
- `ROADMAP.md`: longer product phases beyond the next checkpoint.
