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

Responses are sent by the runtime:

```json
{"type":"response","id":1,"ok":true,"result":{}}
```

Events are pushed by the runtime:

```json
{"type":"event","event":"scene_snapshot","payload":{"root":{"id":"world","name":"World","kind":"world","children":[]}}}
```

## Commands

- `open_project`: records the project path for the runtime session.
- `reload_scene`: asks the runtime to rebuild or refresh its preview scene.
- `select_entity`: asks the runtime to update its selected entity.
- `create_entity`: asks the runtime to create an entity in the active scene.
  The current runtime supports `kind: "cube"` and returns `entity_id`.

## Events

- `runtime_ready`: emitted after the runtime accepts the handshake.
- `scene_snapshot`: hierarchy snapshot for editor panels.
- `log`: runtime message suitable for editor status/log surfaces.
- `frame_stats`: frame rate and frame time telemetry.
- `preview_surface_ready`: runtime-owned shared preview surface descriptor.

`preview_surface_ready` currently carries Windows DirectX metadata:

```json
{"shared_handle_name":"Local\\BesfaPreviewSurface-1234","width":640,"height":360,"format":"bgra8_unorm"}
```

## Ownership

This crate owns protocol names, serializable payloads, codec helpers, and
error shapes. It should not depend on Bevy, Flutter, or platform-specific FFI.
