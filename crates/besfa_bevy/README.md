# besfa_bevy

Bevy integration layer for Besfa.

This crate owns the embedded preview Bevy app and the Bevy-side runtime IPC
plugin. It deliberately sits between the engine/editor domain and the concrete
`besfa_runtime` executable.

## Modules

- `preview.rs`: `BesfaPreviewPlugin`, preview scene setup, grid drawing,
  runtime/editor camera setup, Scene file spawning, and time-driven scene
  animation.
- `scene_file.rs`: `Scene.besfa.json` loader and fallback scene definition.
- `external_preview.rs`: runtime-owned D3D12 shared render target for embedded
  editor and selected camera previews.
- `runtime_ipc.rs`: `BesfaRuntimeIpcPlugin` composition.
- `runtime_ipc/transport.rs`: localhost TCP handshake, command reads, event
  writes.
- `runtime_ipc/systems.rs`: Bevy systems that handle commands, create preview
  scene entities, draw selection gizmos, and emit events.
- `runtime_ipc/snapshot.rs`: preview ECS metadata to `scene_snapshot` payloads.
- `runtime_ipc/resources.rs`: runtime IPC resources, command queues, and client
  registry.

## Preview Backend

The preview runtime is pinned to DX12 through Bevy/wgpu on Windows and keeps its
Bevy host window offscreen and out of the taskbar. The visible preview lives in
the Flutter editor: an editor-only preview camera renders into a shared D3D12
texture that is wrapped as a Bevy image render target. The runtime publishes a
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
updates and `pick_entity` for selecting an entity from normalized preview
viewport coordinates. Scene snapshots include transform metadata when an entity
has a Bevy `Transform` component.

When an entity with a `Transform` is selected, the runtime draws a local X/Y/Z
axis gizmo at that entity's origin. The axes follow the entity rotation so the
editor preview can distinguish local orientation from the fixed viewport axis
overlay.
The same local axis gizmo can be manipulated through
`begin_transform_axis_drag`, `update_transform_axis_drag`, and
`end_transform_axis_drag`: the runtime hit-tests the projected axis lines and
applies drag movement along the selected local axis.

The scene's runtime camera remains a selectable scene entity, but it is not used
for the editor Scene View render target. The editor preview camera is an
internal runtime component controlled through `editor_camera_input` IPC commands
and does not appear in scene snapshots. Its orientation is broadcast through
`editor_camera_state` events so Flutter overlays can stay aligned to the
current Scene View camera.

The runtime also owns a separate selected-camera preview surface. When the
selected scene entity is a camera, an internal preview camera mirrors that
camera's transform and renders into the `camera_preview_surface_ready` target
used by the Inspector.
The `align_selected_camera_to_editor` command copies the editor preview
camera's current `Transform` into the selected scene camera so the user can
promote the Scene View framing into the runtime camera.

## Scene Playback

The preview runtime starts with Bevy virtual game time paused. Rendering,
editor camera input, picking, and gizmos continue to work while game time is
paused, but time-driven scene systems such as `PreviewSpinner` do not advance.
The `play_scene` IPC command unpauses virtual time. The `stop_scene` command
pauses virtual time, clears selection and drag state, and reloads the active
Scene file so play-mode changes return to the authored initial state.

The active Scene file defaults to `Scene.besfa.json` in the runtime working
directory. If `PreviewRuntimeOptions.scene_path` is set, that path is used
instead. Missing or invalid Scene files fall back to the built-in preview scene.
The current file format is JSON:

```json
{
  "version": 1,
  "entities": [
    {"id": "world", "name": "World", "kind": "world"},
    {
      "id": "preview_cube",
      "name": "Preview Cube",
      "kind": "mesh",
      "parent_id": "world",
      "mesh": {"primitive": "cube", "size": {"x": 1.4, "y": 1.4, "z": 1.4}},
      "transform": {"translation": {"x": 0.0, "y": 0.7, "z": 0.0}},
      "spin_y_radians_per_second": 0.6
    }
  ]
}
```

## Usage

```rust
besfa_bevy::run_preview_runtime();
```

For IPC-enabled launches, pass `PreviewRuntimeOptions` with a
`RuntimeIpcConfig`.
