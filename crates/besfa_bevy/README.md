# besfa_bevy

Bevy integration layer for Besfa.

This crate owns the embedded preview Bevy app and the Bevy-side runtime IPC
plugin. It deliberately sits between the engine/editor domain and the concrete
`besfa_runtime` executable.

## Modules

- `preview.rs`: `BesfaPreviewPlugin`, preview scene setup, grid drawing, camera,
  light, and cube animation.
- `external_preview.rs`: runtime-owned D3D12 shared render target for embedded
  editor preview.
- `runtime_ipc.rs`: `BesfaRuntimeIpcPlugin` composition.
- `runtime_ipc/transport.rs`: localhost TCP handshake, command reads, event
  writes.
- `runtime_ipc/systems.rs`: Bevy systems that handle commands, create preview
  scene entities, and emit events.
- `runtime_ipc/snapshot.rs`: preview ECS metadata to `scene_snapshot` payloads.
- `runtime_ipc/resources.rs`: runtime IPC resources, command queues, and client
  registry.

## Preview Backend

The preview runtime is pinned to DX12 through Bevy/wgpu on Windows and keeps its
Bevy host window offscreen and out of the taskbar. The visible preview lives in
the Flutter editor: the preview camera renders into a shared D3D12 texture that
is wrapped as a Bevy image render target. The runtime publishes a
`preview_surface_ready` IPC event with a named shared handle so the Flutter
editor can attach the same GPU resource as a `Texture`.

The direct render bridge uses `wgpu-hal` and `windows` versions that match
Bevy's `wgpu` stack. Updating those crates independently can create duplicate
D3D12 wrapper types and break raw HAL interop.

## Runtime Editing

The IPC plugin handles editor commands against the live preview world. The
first editing command is `create_entity` with `kind: "cube"`, which spawns a
cube under the requested parent, selects it, and requests a fresh
`scene_snapshot`. The runtime also accepts `set_transform` for translation
updates and includes transform metadata in scene snapshots when an entity has a
Bevy `Transform` component.

## Usage

```rust
besfa_bevy::run_preview_runtime();
```

For IPC-enabled launches, pass `PreviewRuntimeOptions` with a
`RuntimeIpcConfig`.
