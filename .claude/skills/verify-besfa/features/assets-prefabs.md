# Assets and prefabs

A user places glTF models from the Assets tab, sees their thumbnails, saves a
group as a prefab, and places that prefab again.

## Sub-features

- `asset-place` instantiates a `.glb` at a position, turn and scale.
- `asset-thumbnail` renders a model's picture for the Assets tab.
- `prefab-save` writes an entity and its children to `assets/prefabs/<name>.scn.ron`.
- `prefab-place` instantiates a saved prefab.

## How to get to it (user POV)

- Assets tab: drag a model into the viewport, or its **➕** (Add to scene).
- Assets tab rows show thumbnails, which the game renders.
- Hierarchy **📦** (Save as prefab), **Prefab name**, **Save**. Then place the prefab like a model.

## Driving it with besfa MCP

Preconditions:

- Baseline game started and `entities` answered.
- A model copied into the project before `start`. `Copy-Item C:\codes\duskhold\assets\props\oak-tree.glb <project>\assets\models\`
  (create `assets\models` first). If duskhold is absent, any `.glb` works. Record which one you used.

- **Place.** `send` `{"command":"instantiate","path":"models/oak-tree.glb","translation":[3,0,0],"yaw":45,"scale":1.2,"name":"Oak"}`.
  A `spawned` id `M`. `entities` with `asset: "oak-tree"` lists `Oak`. With `parts: true` it also lists the model's parts under it.
- **Thumbnail.** `send` `{"command":"thumbnail","path":"models/oak-tree.glb","file":"shots/oak-thumb.png"}`.
  A `thumbnail` report with no error. The PNG shows the tree alone.
- **Save prefab.** `send` `{"command":"save_prefab","id":M,"path":"prefabs/verify-oak.scn.ron"}`.
  A `prefab_saved` report. `<project>/assets/prefabs/verify-oak.scn.ron` exists and names `oak-tree.glb`.
- **Place prefab.** `send` `{"command":"instantiate","path":"prefabs/verify-oak.scn.ron","translation":[-3,0,0]}`.
  A second tree. A `screenshot` with `"target":[0,1,0],"eye":[0,4,12]` shows two oaks.
- **Proof.** Keep the reports, the prefab file, and both PNGs.

## Gotchas

- Copy the model in before `start`. A file added later is picked up by the asset watcher, but the first
  `instantiate` can race it.
- A freshly placed model is invisible for about 1.5 s in a debug build while its pipelines compile, so
  a screenshot taken too early misses it.
- Wait about 1 s between placing a prefab and rewriting its file, or the other instances do not respawn.
- Never copy assets out of duskhold's project into the repo. Copy them only into the throwaway project.
