# Besfa

**English** | [한국어](README.ko.md)

> 📖 **[User manual](docs/manual/README.md)** (in Korean): installing, creating a project, and every part of the editor, step by step with screenshots.

Besfa is a desktop editor for the [Bevy](https://bevy.org/) game engine on Windows. Your game runs inside the editor: you look at it in the viewport, place and change things while it is paused, play it, and save the scene back to a file. The editor UI is built with egui, project management is the `besfa` CLI, and both are written in Rust.

![The Besfa editor](docs/images/editor-overview.png)

## What it does

- **Projects**: a project hub and the `besfa` CLI create Bevy game projects and open existing ones. Bevy is prebuilt once and shared, so a game compiles only its own code.
- **Run in the editor**: the game runs paused in edit mode in the viewport; Play runs it, Stop goes back. Code changes rebuild it.
- **Scene editing**: a hierarchy of the scene's entities, an inspector for their reflected components, a scene view with a camera, move/rotate/scale handles and snapping, undo and redo, saved to the game's scene file.
- **Assets**: place glTF models and prefabs, see thumbnails, reimport changed models, save and apply prefabs, preview a model on its own with its clips.
- **World building**: sculpt and paint terrain, grow foliage, scatter props with a brush, set the sky.
- **Animation**: animation state machines over a model's clips.
- **Multiplayer**: play a dedicated server with clients joined to it.
- **Agents**: `besfa mcp` lets Claude Code run the game without the editor and edit its scene.

## How it fits together

The editor and the game are separate processes. The editor starts the game with `besfa_editor_plugin`, which renders into a texture the editor shows and talks to the editor over the game's stdin and stdout. Games depend only on Besfa crates from this repository, and a release build ships `besfa_components` without the editor.

| Crate | Role |
| --- | --- |
| `besfa_editor` | The egui desktop editor |
| `besfa_cli` | The `besfa` command: create projects, prebuild Bevy, run the MCP server |
| `besfa_protocol` | Messages between the editor and the game |
| `besfa_editor_plugin` | The Bevy plugin the editor runs games with |
| `besfa_components` | Components games ship: scene file, terrain, sky, animation state machines |
| `besfa_net` | Dedicated servers and their connections |
| `besfa_mcp` | The MCP server for agents |

## Documentation

- [User manual](docs/manual/README.md) (Korean): how to use the editor
- [Project overview](docs/PROJECT_OVERVIEW.md), [architecture](docs/ARCHITECTURE.md) and the other design documents in [docs](docs) (Korean): how it is built
