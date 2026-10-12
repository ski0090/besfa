# Besfa

**English** | [한국어](README.ko.md)

A desktop editor for the Bevy game engine. The editor UI is built with egui, and project management is handled by the `besfa` CLI, both in Rust. See [docs/PROJECT_OVERVIEW.md](docs/PROJECT_OVERVIEW.md) for details.

## Requirements

- Windows
- [Rust](https://rustup.rs/) (`cargo` must be on PATH)

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
cargo run -p besfa_editor
```

Restart the editor after installing the CLI so it picks up the PATH change.

## Creating a project

- Editor: in the Project Hub, click **New project** → enter a new folder path → **Create**
- CLI: `besfa new C:\Projects\my_game`

The target folder must not exist yet; its name becomes the project name.

## Updating

### The editor and the CLI

```bash
git pull
cargo install --path crates/besfa_cli
```

Then restart the editor (`cargo run -p besfa_editor`). Reinstalling the CLI matters when `crates/besfa_cli` changed.

When the editor starts, it runs `besfa prebuild` in the background: it moves the shared Bevy build to the latest `besfa_editor_plugin` on GitHub and builds it. The toolbar shows its progress, and **Play** waits until it reads **Bevy is ready**.

### The plugin in a game project

A game depends on `besfa_editor_plugin` from GitHub, and its `Cargo.lock` pins one commit, so the game keeps that plugin until you update it. The icon at the right of the editor toolbar compares the project's commit with the one the Bevy prebuild uses:

- Green check: the same commit.
- Amber warning: a different commit. Hover over it to see what clicking does.

![The amber icon in the toolbar, with its tooltip](docs/images/plugin-update-available.png)

Click the icon to update. The editor runs `cargo update -p besfa_editor_plugin` in the project, which leaves every other dependency alone, then stops the game and rebuilds it with the new plugin. Cargo's output appears in the **Log** panel (`Process exited with code 1.` is the old game being stopped), and the icon turns green.

![The green icon after the update, and cargo's output in the Log panel](docs/images/plugin-updated.png)

Without the editor, run the same command in the project folder:

```bash
cargo update -p besfa_editor_plugin
```

The icon is hidden for a project with no `Cargo.lock` yet, or one that uses the plugin from a local path.

Plugin changes reach games only after they are pushed to `main` on GitHub. After a push, restart the editor so the prebuild moves to the new commit, then click the icon in each project.

## Tests

```bash
cargo test
```
