---
name: verify-scene-saving
description: Model-check with TLA+ that the Besfa editor never throws away a scene edit the user did not Discard, across saving, Play/Stop, net Play, rebuilds on code change, the plugin update icon, closing the window and going back to the hub. Use whenever a change touches the editor's save handshake with the game (Editor::save/save_then/after_save/play/restart/rebuild/update_plugin/launch, Scene::handle/reset, the unsaved-changes dialog, SAVE_TIMEOUT) or the game's Save/Saved handling in edit_mode.rs.
---

# Verify scene saving

**The rule.** An edit the user made in the editor is never lost unless the user
chose Discard.

**The harm.** If it breaks, a scene change the user saw in the viewport, with no
warning, is gone the next time the game starts from `assets/scenes/main.scn.ron`.

The editor and the game are separate processes. The editor sends commands
down the game's stdin, the game answers with reports on stdout, and the editor
learns that a save happened only when the `saved` report arrives.
[SaveHandshake.tla](SaveHandshake.tla) models both sides, the two pipes, and
every way the editor ends the game process. TLC, the TLA+ model checker, tries
every order of user actions, game steps, report reads, taskkills, save
failures and timeouts within the bounds below.

## Run it

```powershell
pwsh -NoProfile -File .claude/skills/verify-scene-saving/check.ps1
```

It runs 18 TLC jobs, one per rule and assumption set, in about 25 s. It prints `PASS` or `FAIL` for each,
with the counterexample as a path of action names. It exits 1 if any result differs from
[expected.txt](expected.txt). Full TLC output with every state of each
counterexample goes to `target/verify-scene-saving/<assumptions>-<rule>.out`.
Read those results with the pstack `/pstack:statechart` skill. It turns each
counterexample into its shortest path in plain words, and charts the states when
a rule holds.

**Setup.** You need Java 17 or later (Java 21 is installed) and `tla2tools.jar` v1.7.4 at
`C:\codes\.tools\tla\tla2tools.jar`, or at the path in `$env:TLA2TOOLS`. If the jar is missing, it comes from
`https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar`
(2,274,532 bytes, sha256 `936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88`).
Ask the user before downloading it.

## Rules and assumptions

Each rule names the way the game ends: `NoLossOnPluginUpdate`, `NoLossOnStop`,
`NoLossOnRebuild` (code changed), `NoLossOnNetPlay`, `NoLossOnClose` (window
closed), `NoLossOnLeave` (back to the hub). Each rule says that ending the game that way never
leaves an edit the user did not Discard missing from the file.

Three assumption sets separate design bugs from races:

| Set | The user | The game's save |
| --- | --- | --- |
| `patient-reliable` | acts only when nothing is in flight | always succeeds in time |
| `patient-failing` | acts only when nothing is in flight | may fail, or take longer than `SAVE_TIMEOUT` (5 s) |
| `hasty-reliable` | acts any time | always succeeds in time |

A FAIL under `patient-reliable` is a design bug: it loses edits however
carefully the user works.

## Findings at the sync commit

F1, F2 and F6 are fixed. F3, F4 and F5 are open races. Each finding comes with the path TLC found, in the code's words.

- **F1, fixed: the plugin icon discarded unsaved edits.** Clicking the plugin status icon
  (✔ or ⚠) ran `cargo update` and restarted the game without saving. Now
  `Editor::update_plugin` first calls `save_then(AfterSave::Update)`.
- **F2, fixed: a failed or slow save still restarted.** `after_save(Rebuild | Play)` restarted
  whether the save worked or `SAVE_TIMEOUT` (5 s) ran out. Standalone Play queued Save and Play
  without waiting, and Stop then restarted from a file the failed save left behind.
  Now `play` waits for its save too (`save_then(AfterSave::Play)`, then `play` again). `after_save` goes ahead
  with a rebuild, a Play or a plugin update only once the save cleared the `*`.
  Otherwise the game keeps the changes and the log says why. A rebuild put off this way sets
  `rebuild_after_save`, and `Editor::ui` rebuilds in the frame the scene saves.
  These fixes took every `patient-failing` rule to PASS.
- **F3, edits during a restart are lost.** You change code while clean, so `restart` sends taskkill.
  Then you edit in the Inspector before the game exits, and the game dies with the edit.
  `starting` disables only the toolbar, and taskkill takes seconds. TLC shows this path for
  Rebuild once scene-view drags are left out of the model. Otherwise F5 is shorter.
- **F4, a save answered after a newer edit clears the mark.** You press Ctrl+S, then edit. The game saves
  without the edit and reports `saved`, and `Scene::handle` sets `dirty = false` on any
  `Saved { error: None }`. Closing or going to the hub now asks nothing. A Play, rebuild or update waiting on
  a save also takes that stale answer and goes ahead, so a Stop after that Play loses the edit.
  TLC shows these paths for Close, Leave and Stop once scene-view drags are left out of the model.
  Otherwise F5 is shorter.
- **F5, a drag still in flight.** This is the shortest `hasty-reliable` path for every rule.
  A handle drag or brush stroke goes to the game as input, and the `*` waits for its
  `Edited`/`Moved`/`TerrainPainted`/`Scattered` report. Closing, Play then Stop, a rebuild or an
  update within that frame or two does not count it.
- **F6, fixed: Play then Stop at once.** taskkill could land before the game ran the Save that
  `play` queued. Since F2's fix Play waits for that save, with the toolbar disabled, so Stop cannot come first.

Every remaining loss needs an action within one save round trip (F4), within a taskkill (F3),
or within a frame (F5).

## Coverage and sync

Last confirmed against commit **`05abc04`**. The model describes these:

| Model action | Code |
| --- | --- |
| `YouEdit` | `Editor::edit`, `Editor::replay` (editor.rs) |
| `YouDragInSceneView` | scene-view input sent with `Editor::send`; `Report::Edited`/`Moved`/`TerrainPainted`/`Scattered` in `Editor::on_report` and `Scene::handle` |
| `YouSave` | `Editor::save` (Ctrl+S, File > Save scene, `save_after_ui`) |
| `YouPressPlay`, `YouPressStop` | `Editor::play`, the toolbar's Play/Stop in `Editor::toolbar`; `AfterSave::Play`, pending `"Play"` |
| `YouPressNetPlay` | `Editor::play` with `net_clients > 0`; the same `AfterSave::Play`, pending `"NetPlay"` in the model |
| `YouChangeCode`, `Rebuild` | `Editor::rebuild` (the `SourceWatch` trigger in `Editor::ui`) |
| `YouClickPluginIcon`, `PluginUpdated` | `Editor::plugin_status`, `update_plugin` (and its `AfterSave::Update` arms in `after_save`), `poll_update`, `after_update` |
| `YouCloseWindow` | `close_requested` handling in `Editor::ui`, `Editor::close` |
| `YouGoToHub` | `Editor::request_leave`, and `App::ui` dropping the editor (main.rs) |
| `YouCancel`, `YouDiscard`, `YouChooseSave` | the `Dialog::Unsaved` modal in editor.rs |
| `EditorReadsReport` | `Editor::poll_game` → `Scene::handle` (scene.rs) → `Editor::on_report` `Saved` arm → `after_save`; then the same frame's `rebuild_after_save` check in `Editor::ui` |
| `AfterSave` | `Editor::after_save`, `rebuild_after_save` (`waitRebuild`) |
| `SaveTimesOut` | the `pending_save` deadline in `Editor::ui`, `SAVE_TIMEOUT` |
| `Restart`, `GameIsKilled`, `EditorRelaunches` | `Editor::restart`, `CliProcess::stop` (process.rs), `relaunch_when_exited`, `launch`, `Scene::reset` |
| `GameRunsCommand`, `GameFailsToSave` | `run_commands` in `crates/besfa_editor_plugin/src/edit_mode.rs`, `Command::Save` → `Report::Saved` |

Check whether a change touches the model with
`git diff 05abc04 -- crates/besfa_editor/src/editor.rs crates/besfa_editor/src/scene.rs crates/besfa_editor/src/process.rs crates/besfa_editor/src/main.rs crates/besfa_editor_plugin/src/edit_mode.rs`.
If the diff touches a function in the table, re-read that function and
compare it with its action before trusting a PASS. Move the sync commit
forward only after you have confirmed that every action still matches the code at the new commit.

## Reading a result

- **A result that matches `expected.txt`.** Nothing new. A PASS covers only the model within its bounds.
- **A new FAIL.** Take the path from the `.out` file, step by step, and find
  each action's code in the table. If the code can take every step in that order,
  it is a real bug: report it with the path in the code's words, and do not weaken the
  rule or the model to make it pass. If a step is impossible in the code, the
  model is wrong. Fix that action to match the code, and say so.
- **A new PASS on a former FAIL.** The finding should be fixed. Confirm in the code
  that the fix is real, not that the model drifted. Then update `expected.txt`
  and this section's findings.
- **An intended change to the covered code.** Change the matching action the same
  way, rerun, and update `expected.txt` only for results the change is meant to move.
  Each rule here has been seen failing on a broken model: drop
  the save in `YouPressPlay`, `YouChangeCode`, `YouPressNetPlay` or `YouClickPluginIcon`, the
  `dirty` check in `YouCloseWindow` or `YouGoToHub`, keep the mark after a failed save, or let
  one of `AfterSave`'s Rebuild, Play, NetPlay or Update arms go ahead after a failed or late save.
  A rule that can no longer fail on such a break has stopped checking anything.
- **`ERROR` or a deadlock.** The model is broken (a typo, an action that sets
  a variable twice or not at all). Read the `.out` file.

## Scope

**In the model:** the editor's dirty mark, pending save and its timeout, Play/Stop, net Play, rebuild, plugin
update, the unsaved-changes dialog, close and hub. The game runs commands in stdin order and
saves or fails, and both pipes keep their order. taskkill lands at any later moment.

**Left out:** game crashes. Edits made while playing, which by design are not saved. What
a net Play's server and clients do. `save_prefab` and prefab apply. The terrain
`.terrain` file written before the scene file. World saves (`save_world`). Whether the
scene file write itself is atomic. The prebuild's busy state. Edits are tokens, so
the model checks whether an edit reached the file, not its content.

**Bounds:** 2 edits, 1 relaunch, at most 4 commands on stdin and 3 reports on stdout.
Every path above needs at most 2 edits and 1 restart.
