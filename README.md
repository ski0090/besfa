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

## License

Apache-2.0
