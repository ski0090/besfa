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

Note: Bevy 0.18 uses `wgpu`, which does not expose a native D3D11 backend. The standalone preview window is pinned to DX12 on Windows to avoid Vulkan-specific noise, while the later Flutter texture path still needs a D3D11 renderer/interop bridge because the Flutter Windows embedder consumes D3D11 textures.

## License

Apache-2.0
