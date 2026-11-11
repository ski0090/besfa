# Panels the game draws

The Hierarchy on the left of the editor's viewport is drawn by the game itself
(`besfa_editor_plugin`'s `panels` module) into the viewport, when the editor
names it in `BESFA_PANELS`. Clicks reach it as the editor's `pointer` commands.

## Sub-features

- A click on a row selects the entity (`selected`); a click on the selected
  row clears the selection.
- The header's delete and duplicate buttons, the add menu and Save as prefab
  come back as `panel` reports for the editor to act on; the game changes
  nothing itself.
- The scene view, the model preview and the game's cameras keep to the part of
  the viewport right of the panel; a click there still picks, and a click on
  the panel never reaches the scene.
- The cameras' narrowed viewports never reach the scene file.

## How to get to it (user POV)

Open a project in the editor: the Hierarchy is the left part of the viewport,
in edit mode and during Play. A project whose plugin predates protocol 15 shows
`Update besfa_editor_plugin in the game to show the Hierarchy.` there instead.

## Driving it with besfa MCP

The MCP server never asks for panels, so this recipe runs the game itself,
windowless, with `BESFA_PANELS=hierarchy`: the game then draws the panel into
the scene view's 1280x720 picture and takes `pointer` commands in its pixels.
Build the project first (SKILL.md, Launch, step 3) and stop any
`mcp__besfa__start` game of the same project, since both write its scene file.

```bash
python .claude/skills/verify-besfa/panels.py <project>
```

It prints one `ok` or `FAIL` line per check and exits 1 on a failure: the row
click and its second click, the delete button and the add menu as `panel`
reports, a pick in the scene view right of the panel, a click on the empty
panel that changes no selection, and Play, a `screenshot` with `game`, Stop
and Save leaving the scene file as the save before Play wrote it.

## Gotchas

- Positions are for the template scene at scale 1: the first row at (50, 41),
  the header's buttons at y 11, the scene view's middle at (754, 360). A
  changed panel layout or width moves them.
- Not covered: what the panel looks like in the editor, and the editor's side
  of a `panel` report (its unit tests cover that). To see it, open the project
  in an editor built from the checkout and capture its window (SKILL.md, the
  editor UI); a game-drawn panel is not in the editor's accessibility tree, so
  UI Automation cannot click its rows.
- The first `save` rewrites the template's scene file in the game's own
  layout, so compare a save with a later save, not with the template.
