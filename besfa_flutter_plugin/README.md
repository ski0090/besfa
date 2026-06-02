# besfa_flutter_plugin

Flutter-facing native bridge for Besfa.

The Dart API in `lib/besfa_flutter_plugin.dart` wraps Rust FFI functions from
`besfa_flutter_plugin/rust`. The editor uses this package to read native ABI
metadata and launch, stop, and inspect the standalone preview runtime process.

## Responsibilities

- Expose the native ABI version to Dart.
- Keep a small FFI smoke path for bridge health checks.
- Launch `besfa_runtime` as a child process.
- Launch `besfa_runtime` with IPC arguments for editor/runtime communication.
- Redirect runtime stdout/stderr to a log file and expose the path to Dart.
- Create, attach, refresh, and dispose Windows preview textures for the editor
  viewport.
- Report runtime process state and the last runtime bridge error.

## Runtime Discovery

Runtime launch looks for `besfa_runtime.exe` beside the editor executable and
under the workspace `target/debug` or `target/release` directories.
When a runtime process is launched, stdout and stderr are written to
`target/besfa_runtime.log` in the workspace when possible, or to
`besfa_runtime.log` in the runtime working directory.
IPC runtime launch is a fresh editor session launch: before starting the new
runtime, the bridge stops its tracked child process and, on Windows, terminates
stale `besfa_runtime.exe` processes whose executable path matches the runtime
that will be launched.

Development overrides:

- `BESFA_RUNTIME_PATH`: absolute path to the runtime executable.
- `BESFA_RUNTIME_WORKING_DIR`: working directory for the child process.

## Preview Texture

On Windows, `createPreviewTexture` registers a native Flutter GPU surface
texture backed by a D3D12 shared resource handle. This remains useful as a
standalone smoke surface for validating Flutter `Texture` plumbing.

For runtime preview, `attachPreviewSurface` opens the named shared handle
published by `besfa_runtime` over IPC and registers it as
`kFlutterDesktopGpuSurfaceTypeDxgiSharedHandle`. The runtime owns the render
target and Bevy renders directly into it; the editor owns the Flutter texture
registration, asks Flutter to sample fresh frames with
`markPreviewTextureFrameAvailable`, and unregisters it with
`disposePreviewTexture`.

## Development

```powershell
flutter analyze
flutter test
```
