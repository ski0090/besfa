# besfa_core

Shared Rust domain constants for Besfa.

This crate should stay small and dependency-light. It currently owns the ABI
version and basic engine metadata used by the Flutter FFI bridge and runtime
integration layers.

## Responsibilities

- Define `ABI_VERSION` for native/editor compatibility checks.
- Expose `EngineInfo::current()` for shared engine identity.
- Keep data types portable across native bridge crates.

## Non-goals

- Bevy integration lives in `crates/besfa_bevy`.
- Runtime process launch lives in `besfa_flutter_plugin/rust`.
- Editor UI models live in `besfa_editor`.
