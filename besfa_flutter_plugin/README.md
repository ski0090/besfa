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
- Report runtime process state and the last runtime bridge error.

## Runtime Discovery

Runtime launch looks for `besfa_runtime.exe` beside the editor executable and
under the workspace `target/debug` or `target/release` directories.

Development overrides:

- `BESFA_RUNTIME_PATH`: absolute path to the runtime executable.
- `BESFA_RUNTIME_WORKING_DIR`: working directory for the child process.

## Development

```powershell
flutter analyze
flutter test
```
