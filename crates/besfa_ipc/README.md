# besfa_ipc

Shared protocol crate for communication between the Flutter editor and the
Bevy preview runtime.

The first protocol is intentionally small: localhost TCP with newline-delimited
JSON. The editor opens a port, launches the runtime with that port and a random
token, sends `hello`, and then exchanges command/response messages and runtime
events.

## Wire Format

Every message is one JSON object followed by `\n`.

```json
{"type":"hello","protocol_version":1,"token":42424242}
```

Commands are sent by the editor:

```json
{"type":"command","id":1,"method":"reload_scene","params":{}}
```

Entity creation includes the entity kind and optional display/parent metadata:

```json
{"type":"command","id":2,"method":"create_entity","params":{"kind":"cube","name":"Cube","parent_entity_id":"world"}}
```

Transform updates target one runtime entity and currently carry translation:

```json
{"type":"command","id":3,"method":"set_transform","params":{"entity_id":"cube_1","translation":{"x":1.0,"y":0.5,"z":-2.0}}}
```

Viewport picks use normalized coordinates inside the runtime preview surface:

```json
{"type":"command","id":4,"method":"pick_entity","params":{"viewport_x":0.5,"viewport_y":0.5}}
```

Editor camera navigation input is separate from scene entity transforms:

```json
{"type":"command","id":5,"method":"editor_camera_input","params":{"rotate_delta_x":12.0,"rotate_delta_y":-4.0,"move_forward":1.0,"speed_multiplier":4.0,"delta_seconds":0.016}}
```

Selected scene cameras can be aligned to the current editor camera:

```json
{"type":"command","id":6,"method":"align_selected_camera_to_editor","params":{}}
```

Selected entity transform axes can be picked and dragged from the Scene View:

```json
{"type":"command","id":7,"method":"begin_transform_axis_drag","params":{"viewport_x":0.5,"viewport_y":0.5}}
{"type":"command","id":8,"method":"update_transform_axis_drag","params":{"viewport_x":0.55,"viewport_y":0.5}}
{"type":"command","id":9,"method":"end_transform_axis_drag","params":{}}
```

Responses are sent by the runtime:

```json
{"type":"response","id":1,"ok":true,"result":{}}
```

Events are pushed by the runtime:

```json
{"type":"event","event":"scene_snapshot","payload":{"root":{"id":"world","name":"World","kind":"world","children":[{"id":"cube_1","name":"Cube 1","kind":"mesh","transform":{"translation":{"x":0.0,"y":0.5,"z":0.0}},"children":[]}]}}}
```

## Commands

- `open_project`: records the project path for the runtime session.
- `reload_scene`: asks the runtime to rebuild or refresh its preview scene.
- `select_entity`: asks the runtime to update its selected entity.
- `pick_entity`: asks the runtime to ray-pick and select an entity from
  normalized preview viewport coordinates.
- `create_entity`: asks the runtime to create an entity in the active scene.
  The current runtime supports `kind: "cube"` and returns `entity_id`.
- `set_transform`: asks the runtime to update an entity transform. The current
  payload supports `translation`.
- `editor_camera_input`: applies editor-only Scene View camera navigation. It
  can carry pointer rotation deltas, local forward/right/world-up movement
  intent, a speed multiplier, and elapsed movement time.
- `align_selected_camera_to_editor`: copies the editor-only Scene View camera
  transform into the currently selected scene camera.
- `begin_transform_axis_drag`: hit-tests the selected entity's local X/Y/Z
  axis gizmo from normalized Scene View coordinates and returns
  `{"axis":"x"}`, `{"axis":"y"}`, `{"axis":"z"}`, or `{"axis":null}`.
- `update_transform_axis_drag`: moves the selected entity along the active
  local axis drag and returns the updated `translation`.
- `end_transform_axis_drag`: clears the active local axis drag session.

## Events

- `runtime_ready`: emitted after the runtime accepts the handshake.
- `scene_snapshot`: hierarchy snapshot for editor panels.
- `log`: runtime message suitable for editor status/log surfaces.
- `frame_stats`: frame rate and frame time telemetry.
- `preview_surface_ready`: runtime-owned shared preview surface descriptor.
- `camera_preview_surface_ready`: runtime-owned shared preview surface
  descriptor for selected scene camera previews.
- `editor_camera_state`: editor-only Scene View camera basis vectors used by
  viewport overlays such as the world-space axis gizmo.

`preview_surface_ready` currently carries Windows DirectX metadata:

```json
{"shared_handle_name":"Local\\BesfaPreviewSurface-1234","width":640,"height":360,"format":"bgra8_unorm"}
```

`camera_preview_surface_ready` carries the same shape for the selected camera
preview texture:

```json
{"shared_handle_name":"Local\\BesfaCameraPreviewSurface-1234","width":320,"height":180,"format":"bgra8_unorm"}
```

`editor_camera_state` carries camera basis vectors in world space:

```json
{"right":{"x":1.0,"y":0.0,"z":0.0},"up":{"x":0.0,"y":1.0,"z":0.0},"forward":{"x":0.0,"y":0.0,"z":-1.0}}
```

## Ownership

This crate owns protocol names, serializable payloads, codec helpers, and
error shapes. It should not depend on Bevy, Flutter, or platform-specific FFI.
