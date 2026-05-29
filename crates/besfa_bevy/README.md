# besfa_bevy

Bevy integration layer for Besfa.

This crate owns the standalone preview Bevy app and the Bevy-side runtime IPC
plugin. It deliberately sits between the engine/editor domain and the concrete
`besfa_runtime` executable.

## Modules

- `preview.rs`: `BesfaPreviewPlugin`, preview scene setup, grid drawing, camera,
  light, and cube animation.
- `runtime_ipc.rs`: `BesfaRuntimeIpcPlugin` composition.
- `runtime_ipc/transport.rs`: localhost TCP handshake, command reads, event
  writes.
- `runtime_ipc/systems.rs`: Bevy systems that handle commands and emit events.
- `runtime_ipc/snapshot.rs`: preview ECS metadata to `scene_snapshot` payloads.
- `runtime_ipc/resources.rs`: runtime IPC resources, command queues, and client
  registry.

## Preview Backend

The standalone preview window is pinned to DX12 through Bevy/wgpu on Windows.
Flutter's Windows embedder consumes D3D11 textures, so embedded texture preview
will require a later interop bridge instead of reusing this window directly.

## Usage

```rust
besfa_bevy::run_preview_runtime();
```

For IPC-enabled launches, pass `PreviewRuntimeOptions` with a
`RuntimeIpcConfig`.
