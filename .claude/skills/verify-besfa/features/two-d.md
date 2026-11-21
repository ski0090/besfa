# 2D games

A user makes a 2D game from the hub's **Create project** dialog (or `besfa new --template 2d`)
and edits it in a flat view: shapes lie on the XY plane, sizes are in pixels, a
turn is about Z, and the camera is a `Camera2d`.

## Sub-features

- `2d-template` makes a game whose scene holds a spinning `Square` and a 2D `Camera`.
- `2d-spawn` adds a rectangle (100x100), a circle (radius 50), an empty or a 2D camera, placed in pixels.
- `2d-refuse` refuses 3D kinds (cube, sphere, plane, light, terrain, sky) with an error.
- `2d-yaw` turns a placement's `yaw` about Z, not Y.
- `2d-save` saves the shapes and the camera's orthographic projection, which comes back on load.
- `2d-shots` renders a `top` view from +Z over that many pixels, and the player's view.
- `2d-sprite` places a PNG as a sprite (`spawn` `kind: sprite` with `path`, or `instantiate` a `.png`),
  cuts a cell out of a sheet with `SpriteImage`'s `atlas`, saves only `SpriteImage`, and draws it again on load.
- `2d-aim` hands a `pointer` after `play` to the game's `Window::cursor_position`, which
  `Camera::viewport_to_world_2d` turns into a world point.
- `2d-step` plays exactly N frames with `step`, one fixed timestep (1/64 s) each, and pauses again.

## How to get to it (user POV)

- Hub **New project** opens **Create project**: under **Template** pick **2D**, then **➕ Create**.
  Or `besfa new <name> --template 2d` in a terminal.
- Hierarchy **➕** menu in a 2D game: **Empty**, **Rectangle**, **Circle**, **Camera**.
- Scene view: both drags pan, the wheel zooms, handles move and scale on X and Y and rotate about Z.
- **File > Save scene** or **Ctrl+S**.

## Driving it with besfa MCP

Preconditions:

- A game from `new-game.ps1 -Parent <run folder> -Template 2d`, built and started.
  `entities` with `timeout_secs: 300` lists `Square` and `Camera`.

- **Spawn and refuse.** `send` `[{"command":"spawn","kind":"rectangle","name":"VerifyRect","translation":[200,0,0],"yaw":45},{"command":"spawn","kind":"circle","name":"VerifyDot","translation":[-200,0,0]},{"command":"spawn","kind":"cube","name":"NoCube"}]`.
  Two `spawned` reports, then `{"command":2,"error":"a 2D game does not show Cube","type":"done"}`.
  The tool reports the batch as an error because of that last line. That is the expected refusal.
- **Turn about Z.** `entities` with the rectangle's id. `Transform` `rotation` is about
  `[0, 0, 0.3827, 0.9239]`, and the entity has a `Mesh2d` and `MeshShape` `{"Rectangle":{"size":[100.0,100.0]}}`.
- **Save.** `send` `save`. `assets/scenes/main.scn.ron` holds `"VerifyRect"` with `Rectangle(` and
  `rotation: (0.0, 0.0, 0.38268346, 0.9238795)`, `"VerifyDot"` with `Circle(` `radius: 50.0`, and the
  `Camera` block with `Camera2d` and `Projection": Orthographic(`.
- **See it.** `send` `[{"command":"screenshot","file":"shots/top.png","target":[0,0,0],"top":800},{"command":"screenshot","file":"shots/game.png","game":true}]`.
  `top.png` is 1024x1024 and shows a grey circle left, the blue square in the middle and a
  grey diamond (the turned rectangle) right. `game.png` is 1280x720 with the same three at one pixel per unit.
- **Persist.** `stop`, `start`, `entities`: `Square`, `Camera`, `VerifyRect`, `VerifyDot`, all `scene: true`.
- **Proof.** Keep the reports, the saved scene file, both PNGs and the post-restart `entities`.
- **Sprites.** Draw test PNGs locally (Pillow), never with `generate_image`. Put them in `<project>/assets/sprites/`.
  `send` `[{"command":"spawn","kind":"sprite","path":"sprites/crate.png","translation":[-200,0,10]},{"command":"instantiate","path":"sprites/sheet.png","translation":[200,0,10]}]`
  gives two `spawned` entities named `crate` and `sheet`. `set` the sheet's `besfa_editor_plugin::scene::SpriteImage` to
  `{"path":"sprites/sheet.png","anchor":[0,-0.5],"size":[96,96],"atlas":{"cell":[32,32],"columns":4,"rows":2,"index":5}}`.
  After `save`, the scene file holds `SpriteImage` blocks and no `bevy_sprite::sprite::Sprite` for them. After `stop`, `start`,
  `play`, a `game` screenshot shows the crate at its own size and cell 5 of the sheet, 96 px, standing on y 0.
- **Aim.** The game needs a system that reads `Window::cursor_position` of the `PrimaryWindow` and
  `viewport_to_world_2d` through its active camera into a reflected component on a named entity. Before `play` the window
  has no cursor (the pointer drives the scene view). After `play`, `pointer` `move` to `[740,300]` reads world `[100,60]`
  with the template camera at the origin, and `[140,610]` reads `[-500,-250]`.
- **Step.** Count frames whose `Time<Virtual>` delta is not zero, `FixedUpdate` runs and `elapsed_secs_f64` in a reflected
  component. From a fresh start, `step` `frames: 10` reads 10, 10, 0.15625; real time adds nothing; `seconds: 0.5` adds
  32 frames (0.65625 s). A command sent after a `step` in the same batch runs after its frames.

## Gotchas

- Units are pixels: a translation of `[200,0,0]` is 200 px right of the middle. A
  3D-sized placement such as `[2,0,0]` lands inside the square.
- A spawn of a kind the game cannot show fails the whole tool call, even when the
  other commands in the batch succeeded. Read each report line.
- Markers (`"markers":true`) and prefab thumbnails are still drawn for 3D, so they read
  badly in a 2D game. Leave them out of a 2D proof.
- A plugin before protocol 19 does not know rectangles and circles. The MCP refuses the
  command with `Command N needs protocol 19` instead of sending it. `step` needs 20, sprites 21.
- After `reset`, `entities` returns the list it holds until the game reports anew, and the
  game respawns its own entities under new ids: wait about a second and list again
  before `entities` with an id, or it answers with the old entity's last report.
- Sprites are not picked by a click in the scene view, which ray casts meshes: select them in
  the Hierarchy or by id.
