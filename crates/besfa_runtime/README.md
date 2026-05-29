# besfa_runtime

Standalone preview runtime executable for Besfa.

The runtime starts the Bevy preview app from `besfa_bevy`. The editor normally
launches this process through the Flutter plugin, but it can also be run
directly during development.

## Commands

```powershell
cargo run -p besfa_runtime
```

With IPC enabled:

```powershell
cargo run -p besfa_runtime -- --ipc-port 49152 --ipc-token 42424242
```

## CLI Options

- `--ipc-port <port>` or `--ipc-port=<port>`: localhost TCP port to bind.
- `--ipc-token <token>` or `--ipc-token=<token>`: handshake token expected from
  the editor.

Both IPC options must be present for IPC to start. Without them, the runtime
opens only the standalone preview window.
