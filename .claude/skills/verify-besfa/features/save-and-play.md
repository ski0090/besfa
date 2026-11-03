# Save and play

A user saves the scene to `assets/scenes/main.scn.ron`, closes and reopens the
project to find it as saved, and presses Play to run the game from that state.

## Sub-features

- `save-scene` writes every scene entity to the scene file.
- `save-persist` brings a saved scene back after the game restarts.
- `save-before-load` refuses to save before the scene file has loaded, so the save cannot overwrite the file with an empty scene.
- `play` unpauses the game, and time-driven things start to move.

## How to get to it (user POV)

- **File > Save scene** or **Ctrl+S**.
- Reopening the project, or **⏹ Stop** after Play, which relaunches the game from the file.
- Toolbar **▶ Play**, which saves unsaved changes first.

## Driving it with besfa MCP

Preconditions:

- Baseline game started and `entities` answered.

- **Change and save.** `send` `[{"command":"spawn","kind":"sphere","name":"VerifyBall","translation":[2,1,0]},{"command":"save"}]`.
  The replies are `spawned` and then `{"error":null,"type":"saved"}`.
- **Read it back.** `assets/scenes/main.scn.ron` holds a block with `"bevy_ecs::name::Name": "VerifyBall"`
  and `MeshShape": Sphere(`.
- **Persist.** `stop`, `start` the same project, then `entities` with `name: "VerifyBall"` and
  `timeout_secs: 300`. It is listed with `scene: true`, under a new id.
- **See it.** `send` `{"command":"screenshot","file":"shots/after-save.png","target":[0,0.5,0],"eye":[6,4,8]}`.
  The reply is a `screenshot` report with `error: null`. The PNG shows the blue template cube and a grey sphere to its right.
- **Play.** `send` `{"command":"play"}`. It returns no reports. Then `send` the same `screenshot` with
  `"file":"shots/after-play.png"`. That PNG shows the template cube turned, compared with `after-save.png`.
- **The player's view.** `send` `{"command":"screenshot","file":"shots/game.png","game":true}`, before or
  after `play`. The PNG is 1280x720 and shows what the template's Camera sees, with any UI the game spawned.
  To check UI, give the game a `Startup` system that spawns a `Node` with a `BackgroundColor`, and pass it text
  through `start`'s `args`.
- **Proof.** Keep the reports, the scene-file excerpt, the post-restart `entities`, and the PNG.

## Gotchas

- `save` is refused until the scene file has loaded. `entities` waits for that, so call
  it once after every `start`.
- The `saved` report is not proof. Read the file, then restart and list the entity.
- The screenshot is written under `<project>/shots/`. Copy it out before cleanup deletes the project.
- After `play`, the template's Cube spins, so do not diff that screenshot against one taken before `play`.
