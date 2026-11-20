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

## Gotchas

- Units are pixels: a translation of `[200,0,0]` is 200 px right of the middle. A
  3D-sized placement such as `[2,0,0]` lands inside the square.
- A spawn of a kind the game cannot show fails the whole tool call, even when the
  other commands in the batch succeeded. Read each report line.
- Markers (`"markers":true`) and prefab thumbnails are still drawn for 3D, so they read
  badly in a 2D game. Leave them out of a 2D proof.
- A plugin before protocol 19 does not know rectangles and circles. The MCP refuses the
  command with `Command N needs protocol 19` instead of sending it.
