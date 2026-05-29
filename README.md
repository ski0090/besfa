# Besfa

Besfa is a lightweight Flutter editor shell for Bevy projects.

The early architecture is intentionally split:

- `besfa_editor`: Flutter desktop editor.
- `besfa_flutter_plugin`: Flutter plugin boundary for native integration.
- `besfa_flutter_plugin/rust`: Rust FFI crate built with `native_toolchain_rust`.
- `crates/besfa_core`: editor/runtime shared domain model.
- `crates/besfa_bevy`: Bevy integration layer.
- `crates/besfa_runtime`: preview/runtime entry point.

First milestone: keep the editor UI, native bridge, and Bevy runtime loosely coupled so the preview path can evolve from a separate runtime window to Windows texture/D3D interop later.

## Project Guides

Each workspace project owns a short README for its boundary:

- `besfa_editor/README.md`: Flutter editor layout, Feature-Sliced Design ownership, and preview controls.
- `besfa_flutter_plugin/README.md`: Flutter-facing native bridge API and runtime launch behavior.
- `crates/besfa_core/README.md`: shared engine metadata and ABI constants.
- `crates/besfa_ipc/README.md`: TCP newline-delimited JSON protocol, commands, events, and payload examples.
- `crates/besfa_bevy/README.md`: Bevy preview plugin, IPC plugin, and render backend notes.
- `crates/besfa_runtime/README.md`: standalone runtime executable and CLI options.

Documentation rules for contributors and agents live in `DOCUMENT.md`.

## Preview Runtime

Note: Bevy 0.18 uses `wgpu`, which does not expose a native D3D11 backend. The standalone preview window is pinned to DX12 on Windows to avoid Vulkan-specific noise, while the later Flutter texture path still needs a D3D11 renderer/interop bridge because the Flutter Windows embedder consumes D3D11 textures.

Development preview launch can be overridden with `BESFA_RUNTIME_PATH` and `BESFA_RUNTIME_WORKING_DIR` when the runtime binary is not beside the editor executable or under the workspace `target` directory.

Runtime IPC starts as TCP on `127.0.0.1` with newline-delimited JSON. The editor launches the runtime with `--ipc-port` and `--ipc-token`, sends a `hello` message, and treats the preview as ready after receiving `runtime_ready`.

## Common Commands

```powershell
cargo test
cargo run -p besfa_runtime
flutter analyze .\besfa_editor
flutter test .\besfa_editor
flutter build windows .\besfa_editor
```

## License

Apache-2.0
