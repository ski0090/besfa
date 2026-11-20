# Scene files

A game can have more scene files than `scenes/main.scn.ron`. The user opens one
in the editor by double-clicking it under `scenes/` in the Assets tab, saves into
the scene that is open, and a game that asks to switch scenes (`SwitchScene`) does
so when Play starts, not while the user edits.

## Sub-features

- `scene-open` starts the game in another scene file (`BESFA_SCENE`, which the editor sets).
- `scene-new` opens a scene file that does not exist as an empty scene, and the first `save` writes it.
- `scene-save-target` saves into the open scene and leaves `main.scn.ron` alone.
- `scene-switch-held` holds a game's `SwitchScene` in edit mode and sends it at Play.
- `scene-switch` replaces the scene's entities with the new file's when the switch happens.

## How to get to it (user POV)

- Assets tab: double-click a `.scn.ron` under `scenes/`. The editor restarts the game in it,
  and the window title names it. It refuses while the game plays or the scene has unsaved changes.
- **File > Save scene** writes the open scene.
- **▶ Play** in a game that writes `SwitchScene` (a gate, a level select, a save loader).
- New scene files are made by copying `main.scn.ron` (the editor has no New scene yet, see `docs/TODO.md`).

## Driving it with besfa MCP

Preconditions:

- Baseline game built and started once. This recipe was proven in a 2D game
  (`-Template 2d`). In a 3D game spawn a `cube` instead of the `circle`; that variant has not been run.
- Record `sha256sum <project>/assets/scenes/main.scn.ron` before you start.

- **Open a new scene.** `stop`, then `start` with `scene: "scenes/verify.scn.ron"`. `entities` with
  `timeout_secs: 300` answers `[]`.
- **Save into it.** `send` `[{"command":"spawn","kind":"circle","name":"VerifyLevel","translation":[0,100,0]},{"command":"save"}]`.
  `spawned`, then `{"error":null,"type":"saved"}`. `assets/scenes/verify.scn.ron` exists and holds
  only `"VerifyLevel"`. `main.scn.ron` has the same sha256 as before.
- **Switch at Play.** This needs game code. `stop`, then append to `<project>/src/main.rs`:
  ```rust
  /// verify-besfa: `-- --switch-to scenes/x.scn.ron` asks for that scene at Startup.
  fn switch_from_args(mut switches: MessageWriter<besfa_components::SwitchScene>) {
      let args: Vec<String> = std::env::args().collect();
      if let Some(i) = args.iter().position(|a| a == "--switch-to") {
          switches.write(besfa_components::SwitchScene(args[i + 1].clone()));
      }
  }
  ```
  and add `.add_systems(Startup, switch_from_args)` to the `App`. `start` with
  `args: ["--switch-to","scenes/verify.scn.ron"]` rebuilds only the game crate. `entities` still lists
  `main.scn.ron`'s entities: the switch is held. `send` `{"command":"play"}` (no reports), then
  `entities` lists only `VerifyLevel`.
- **Proof.** Keep the reports, both scene files, the two hashes, the edited `main.rs`, and both
  `entities` answers around `play`.

## Gotchas

- `start` with `scene` needs the game's plugin at protocol 18 or later. An older one silently opens
  `main.scn.ron`, so check that `entities` answers `[]` for a new file.
- Ids change on every start, and also when the scene switches. Look them up again.
- After Play switched scenes, `save` writes the scene now in the world, not the one you opened.
  Stop and start before saving the opened scene again.
- Not covered here: the editor's Assets tab double-click, its window title and its refusal while
  playing or unsaved (editor side, see SKILL.md's editor UI section), and `KeepOnSceneSwitch`
  (game code, covered by `besfa_components` unit tests).
