# besfa_flutter_plugin_ffi

Rust FFI crate used by `besfa_flutter_plugin`.

This crate exposes C ABI functions consumed from Dart through `dart:ffi`. It is
responsible for process-level runtime control and must keep the exported symbols
stable enough for the Dart wrapper to map native status codes safely.

## Exported Runtime Functions

- `besfa_flutter_plugin_abi_version`
- `besfa_flutter_plugin_add`
- `besfa_runtime_start`
- `besfa_runtime_start_with_ipc`
- `besfa_runtime_stop`
- `besfa_runtime_status`
- `besfa_runtime_last_error_code`
- `besfa_runtime_log_path`

## Notes

The FFI layer launches the runtime as a separate process. IPC is intentionally
kept out of this crate beyond passing `--ipc-port` and `--ipc-token` arguments
to the runtime executable.
IPC launches are treated as fresh editor sessions: the bridge clears its
tracked child process first and, on Windows, terminates stale runtime processes
that point at the same executable path before spawning the new runtime.

The runtime child process writes stdout and stderr to a native log file. The
FFI layer owns that file path and exposes it through `besfa_runtime_log_path`
so Dart can tail native process output without taking over process ownership.
