# besfa_editor

Flutter desktop editor for Besfa.

The editor follows a lightweight Feature-Sliced Design layout:

- `app`: application bootstrap and app-wide configuration.
- `pages`: route-level screens that compose slices and widgets.
- `features`: user-facing behaviors, state, and actions.
- `widgets`: composed UI blocks for editor regions.
- `shared`: reusable UI and utilities with no feature ownership.

## Current Features

- Launch, stop, and reload the standalone Bevy preview runtime.
- Attach the runtime-owned Windows shared preview surface in the viewport when
  available.
- Connect to the runtime over localhost TCP IPC.
- Render the runtime `scene_snapshot` in the Scene panel.
- Forward scene tree selection to the runtime with `select_entity`.
- Display runtime status, log messages, and frame stats in the editor shell.

## Development

```powershell
flutter analyze
flutter test
flutter build windows
```

The runtime binary is discovered by the native plugin under the workspace
`target` directory or beside the editor executable. Override discovery with
`BESFA_RUNTIME_PATH` and `BESFA_RUNTIME_WORKING_DIR` when needed.
