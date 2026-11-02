# Terrain

A user adds flat ground, raises and lowers it with a brush, paints its surface
layers, and saves. The heights go to a `.terrain` file beside the scene.

## Sub-features

- `terrain-create` spawns 256 m of flat ground under a free `terrain/terrain-N.terrain` name.
- `terrain-sculpt` runs raise, lower, smooth and flatten strokes along a path.
- `terrain-paint` paints a layer.
- `terrain-undo` restores a stroke's before or after state.
- `terrain-save` writes the heights to the `.terrain` file on save.

## How to get to it (user POV)

- Hierarchy **➕ > Terrain**.
- With the terrain selected: the Inspector's **Raise**, **Lower**, **Smooth**, **Flatten** and **Paint**
  tools, then a left-drag in the scene view. **Ctrl+Z** undoes a stroke.
- **File > Save scene**.

## Driving it with besfa MCP

Preconditions:

- Baseline game started and `entities` answered.

- **Create.** `send` `{"command":"spawn","kind":"terrain","name":"Ground"}`. A `spawned` id `T`.
- **Sculpt.** `send` `{"command":"terrain_stroke","id":T,"tool":"raise","points":[[-10,0],[10,0]],"radius":8,"strength":2}`.
  The reply is a `terrain_painted` report with stroke number `S`.
- **Measure.** `send` `{"command":"heights","points":[[0,0],[0,40]]}`. A `heights` report: about 2 at
  `[0,0]` (the ridge), about 0 at `[0,40]`.
- **Undo.** `send` `{"command":"terrain_restore","stroke":S,"before":true}`, then `heights` again: both about 0.
  Redo with `"before":false`.
- **Save.** `send` `save`. `<project>/assets/terrain/` now has a `terrain-*.terrain` file, and
  `main.scn.ron` names it in the terrain's block.
- **Proof.** Keep the `heights` reports before and after, the file listing, and a `screenshot` with
  `"top":80` over `[0,0,0]` that shows the ridge.

## Gotchas

- `raise` and `lower` take `strength` in meters at the brush middle. `smooth`, `flatten` and `paint` take 0 to 1.
  `paint` needs `layer`.
- `heights` returns `null` where no terrain is. A null means the point lies outside the 256 m square, not
  that the ground is at zero.
- The `.terrain` file only appears on `save`. Before that the heights live in the game.
