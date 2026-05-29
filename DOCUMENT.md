# Documentation Rules

Besfa is split across Flutter packages and Rust crates, so public boundaries
must be documented when they change.

## Public API Comments

- Add `///` documentation to Rust items that are exposed outside their module
  with `pub`, especially items re-exported from `lib.rs` with `pub use`.
- Document public Rust structs, enums, functions, constants, enum variants, and
  public fields when they describe an API contract.
- Add Dart `///` documentation to public classes, enums, methods, getters, and
  fields that are consumed across feature/package boundaries.
- Internal helpers may stay undocumented when their name and local context are
  enough. Prefer comments for contracts, wire format, ownership, and lifecycle,
  not for obvious implementation details.

## Project README Files

- Each project or crate should keep its own `README.md`.
- When behavior, ownership, commands, IPC payloads, or launch assumptions change,
  update the matching project README in the same change.
- If a project README disagrees with code or root documentation, update it before
  finishing the task.
- Root `README.md` should stay as the high-level map. Project READMEs should own
  local details.

## Current README Ownership

- `README.md`: workspace overview, project map, common commands.
- `besfa_editor/README.md`: Flutter editor and FSD layout.
- `besfa_flutter_plugin/README.md`: Dart-facing plugin API and runtime launch.
- `besfa_flutter_plugin/rust/README.md`: Rust FFI export boundary.
- `crates/besfa_core/README.md`: shared engine metadata.
- `crates/besfa_ipc/README.md`: IPC protocol, commands, events, payload shapes.
- `crates/besfa_bevy/README.md`: Bevy preview and runtime IPC plugins.
- `crates/besfa_runtime/README.md`: runtime executable CLI.
