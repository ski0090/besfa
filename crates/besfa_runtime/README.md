# besfa_runtime

Embedded preview runtime executable for Besfa.

The runtime starts the Bevy preview app from `besfa_bevy` with its Bevy host
window placed offscreen and hidden from the taskbar. The editor normally
launches this process through the Flutter plugin and displays the shared GPU
surface in its viewport.

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
still runs the offscreen preview loop, but no editor can attach the shared
surface.
