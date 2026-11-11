# Besfa verification map

This folder is the maintained source for verifying what a Besfa user can do in
a scene. Read this index, then use the matching feature file as the recipe.
Every recipe drives the game side of an editor feature through the besfa MCP
server, which sends the same `besfa_protocol` commands the editor's UI sends.

## Baseline preconditions

- A throwaway project from `new-game.ps1`, patched to the checkout under test,
  built, started with `mcp__besfa__start`, and answering `mcp__besfa__entities`
  with the template's `Cube`, `Light` and `Camera` (see `../SKILL.md`, Launch).
- `doctor.ps1 -Project <project>` prints no `FAIL`.
- Never drive a project the user's editor has open, or one this run did not create.

## Driving conventions

- Start each recipe from a freshly started game unless it says otherwise. Ids
  change on every start, so take them from `spawned` reports or `entities`.
- Send a recipe's commands in one `send` batch where they belong together. A
  `done` report with an `error` fails the step.
- Paths in commands are relative to `<project>/assets/`. Paths in `screenshot`
  and `thumbnail` are relative to `<project>`.

## Proof and skip reporting

- Keep the batch and its reports, and the side effect read back from disk.
- Anything that must persist gets a second view after `stop` and `start`.
- Copy screenshots to `target/verify-besfa/<run name>/` and look at them.
- Report a step you could not reach with the command you tried and the unmet
  precondition. Do not report a sub-feature verified through a different entry point
  than the one changed.

## Feature entry contract

Each feature file has an H1, one paragraph on what the user sees, then exactly
four H2s in this order: `Sub-features`, `How to get to it (user POV)`,
`Driving it with besfa MCP`, `Gotchas`.

## Features

- [Scene entities](./scene-entities.md): create, edit components, duplicate, delete and restore.
- [Save and play](./save-and-play.md): save the scene, persist across a restart, play.
- [Terrain](./terrain.md): create ground, sculpt and paint it, save its `.terrain` file.
- [Assets and prefabs](./assets-prefabs.md): place a glTF model, thumbnail, save and place a prefab.
- [Panels the game draws](./panels.md): the Hierarchy in the viewport, clicked with `pointer` commands.

Not mapped yet (add a file when a change touches them): scatter brush, sky,
animation state machines, model preview, undo/redo (editor-side history), net
Play (server and clients), the project hub.
