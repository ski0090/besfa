# besfa_editor

Flutter desktop editor for Besfa.

The editor follows a lightweight Feature-Sliced Design layout:

- `app`: application bootstrap and app-wide configuration.
- `pages`: route-level screens that compose slices and widgets.
- `features`: user-facing behaviors, state, and actions.
- `widgets`: composed UI blocks for editor regions.
- `shared`: reusable UI and utilities with no feature ownership.

## Current Features

- Boot an editor-owned Scene Runtime automatically when the editor opens.
- Restart the Scene Runtime and reload the active scene from the top bar.
- Add cube entities to the live runtime scene through IPC.
- Attach the runtime-owned Windows shared preview surface in the viewport when
  available.
- Show a world-space X/Y/Z axis gizmo in the Scene View.
- Connect to the runtime over localhost TCP IPC.
- Render the runtime `scene_snapshot` in the Scene panel.
- Forward scene tree selection to the runtime with `select_entity`.
- Forward Scene View clicks to the runtime with `pick_entity` so objects can be
  selected directly from the viewport.
- Show selected runtime entity metadata and editable position fields in the
  Inspector panel.
- Display runtime status, log messages, and frame stats in the editor shell.
- Show a collapsible bottom runtime log panel. The collapsed panel shows the
  latest log line; the expanded panel scrolls through runtime logs and can copy
  all logs to the clipboard. The panel includes runtime IPC log events and
  native stdout/stderr lines tailed from the runtime process log file.
- Recover the Scene Runtime automatically when the tracked process exits.

## Development

```powershell
flutter analyze
flutter test
flutter build windows
```

The runtime binary is discovered by the native plugin under the workspace
`target` directory or beside the editor executable. Override discovery with
`BESFA_RUNTIME_PATH` and `BESFA_RUNTIME_WORKING_DIR` when needed.

The editor treats the runtime as a resident Scene View backend rather than a
manual preview window. On startup it launches the runtime with IPC arguments,
waits up to 20 seconds for `runtime_ready`, and keeps the viewport available
for scene editing.
Scene editing commands, such as adding a cube, are sent to the runtime over IPC
and reflected back through `scene_snapshot`. Position edits use `set_transform`
and viewport clicks use `pick_entity`; both are confirmed by the next runtime
snapshot.
