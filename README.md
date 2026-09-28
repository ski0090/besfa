# Besfa

**English** | [한국어](README.ko.md)

A desktop editor for the Bevy game engine. The editor UI is built with Flutter, and project management is handled by the `besfa` CLI (Rust). See [docs/PROJECT_OVERVIEW.md](docs/PROJECT_OVERVIEW.md) for details.

## Requirements

- Windows
- [Rust](https://rustup.rs/) (`cargo` must be on PATH)
- [Flutter](https://docs.flutter.dev/get-started/install/windows/desktop) (with Windows desktop support set up)

## Installation

### 1. Install the CLI

The editor looks for the `besfa` executable on PATH.

```bash
cargo install --path crates/besfa_cli
```

This installs `~/.cargo/bin/besfa.exe`. Verify:

```bash
besfa --version
```

Run the same command again after changing the CLI code.

### 2. Run the editor

```bash
cd apps/editor_ui
flutter run -d windows
```

Restart the editor after installing the CLI so it picks up the PATH change.

## Creating a project

- Editor: in the Project Hub, click **New project** → enter a new folder path → **Create**
- CLI: `besfa new C:\Projects\my_game`

The target folder must not exist yet; its name becomes the project name.

## Tests

```bash
cargo test
cd apps/editor_ui && flutter test
```
