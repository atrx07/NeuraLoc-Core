# Development

## Prerequisites

- Windows 11
- Node.js 22 or newer
- Rust stable MSVC toolchain
- Microsoft C++ Build Tools and Windows SDK
- WebView2 runtime

After installing Rust, open a new terminal and verify the toolchain is discoverable:

```powershell
rustc --version
cargo --version
rustup show active-toolchain
```

If those commands are not found, add `%USERPROFILE%\.cargo\bin` to the user `PATH`
and restart the terminal or Codex app.

## Commands

```powershell
npm.cmd install
npm.cmd run dev
npm.cmd run build
npm.cmd run test
npm.cmd run tauri dev
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
```

The networked package integration remains opt-in and downloads only the pinned official archive into a temporary application-data directory:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml engine_packages::service::tests::installs_verifies_and_uninstalls_the_pinned_package -- --ignored --exact
```

Real-model load/stream/stop validation is also opt-in. It uses existing local files, never modifies the selected GGUF, and leaves no server running:

```powershell
$env:NEURALOC_TEST_LLAMA_SERVER="C:\path\to\llama-server.exe"
$env:NEURALOC_TEST_GGUF="C:\path\to\model.gguf"
cargo test --manifest-path src-tauri/Cargo.toml engines::llama_cpp::tests::loads_streams_and_stops_a_real_local_model -- --ignored --exact
```

The browser Vite build uses a typed demo bridge when Tauri IPC is unavailable. This supports UI development only; process, filesystem, SQLite, and hardware behavior must be verified in Tauri.

## Engineering rules

- Commands remain thin and typed.
- Child processes are launched only through `ProcessManager`.
- SQL lives in migrations or repositories.
- Hardware and backend decisions go through the capability matrix and scheduler.
- Long operations accept cancellation and emit progress events.
- Tests use small fixtures and injected hardware probes; normal CI never downloads models or engine packages. The ignored package test validates the official runtime build separately, and the ignored real-model test runs only when explicit local paths are supplied.

## Packaging

Production packaging uses `npm.cmd run tauri build`. Native engine packages and the catalog are versioned separately and verified during installation. Portable mode is a data-location option, not an unsigned repack of the application.
