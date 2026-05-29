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
- Create and dispose Windows preview textures for the editor viewport.
- Report runtime process state and the last runtime bridge error.

## Runtime Discovery

Runtime launch looks for `besfa_runtime.exe` beside the editor executable and
under the workspace `target/debug` or `target/release` directories.

Development overrides:

- `BESFA_RUNTIME_PATH`: absolute path to the runtime executable.
- `BESFA_RUNTIME_WORKING_DIR`: working directory for the child process.

## Preview Texture

On Windows, `createPreviewTexture` registers a native Flutter GPU surface
texture backed by a D3D12 shared resource handle. The first implementation is a
static smoke surface that validates Flutter `Texture` plumbing before Bevy frame
copy and synchronization are added.

The Windows bridge returns Flutter texture ids to Dart and keeps the native
resource alive until `disposePreviewTexture` unregisters it. The surface uses
`kFlutterDesktopGpuSurfaceTypeDxgiSharedHandle` so the editor can stay aligned
with the Windows embedder's DirectX path.

## Development

```powershell
flutter analyze
flutter test
```
