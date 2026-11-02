---
name: verify-besfa
description: Prove a Besfa change works by running a real game the way the editor does, headless, through the besfa MCP server (start, send editor commands, list entities, save, screenshot) against a throwaway project built from this checkout's crates. Use after changing besfa_editor_plugin, besfa_components, besfa_protocol or besfa_mcp, or to check scene editing, saving, terrain, assets or scatter end to end. The egui editor UI itself is a secondary surface (see the end).
---

# Verify Besfa

Besfa is a desktop editor whose game runs as a separate process. The editor
starts the game in edit mode and talks to it over stdin/stdout with
`besfa_protocol` commands and reports. `besfa_mcp` starts the game the same way
(`cargo run --features bevy/dynamic_linking,besfa_editor`, `BESFA_EDIT_MODE=1`)
but without a window (`BESFA_HEADLESS=1`), so an agent can drive the game side
of every editor feature without stealing the user's focus. This skill drives
that. The feature map in [features/README.md](features/README.md) lists the
recipes.

All commands below run from the checkout root (the folder with the workspace
`Cargo.toml`). If you are verifying a worktree, run them from the worktree, so the game
is patched to its crates.

## Launch

1. **Pick a run folder and name.** Use your session's scratchpad if you have one, else
   `$env:TEMP\besfa-verify`. The game's name must be unique per run
   (`verify_<HHmmss>`), because every Besfa game builds into one shared
   target, `%LOCALAPPDATA%\Besfa\target`, and two runs with the same name would
   overwrite each other's exe.
2. **Make the project.**
   ```powershell
   pwsh -NoProfile -File .claude/skills/verify-besfa/new-game.ps1 -Parent <run folder>
   ```
   It builds this checkout's `besfa` CLI (the installed `~/.cargo/bin/besfa.exe`
   can trail the template, so never use it here and never reinstall it: the
   user's editor runs it for its prebuild). It also runs `besfa new` and appends a
   `[patch]` that points `besfa_components` and `besfa_editor_plugin` at this
   checkout. It prints the project path. That is `<project>` below.
3. **Build in the background** (Bash tool with `run_in_background`, no `timeout` wrapper,
   which would kill rustc mid-crate):
   ```bash
   cd <project> && cargo build --features bevy/dynamic_linking,besfa_editor 2>&1 | grep -v Compiling | tail -20
   ```
   Expect about 1-3 min when Bevy is already in the shared target. Cargo
   prints `Blocking waiting for file lock on build directory` while another session's
   game builds, and waits. That is fine. A Bevy version change or a
   cold target takes 15-35 min.
4. **Start.** `mcp__besfa__start` with `project` = `<project>`. It returns as soon as cargo
   starts.
5. **Ready = the scene file has loaded.** `mcp__besfa__entities` with `timeout_secs: 300`
   returns the template's `Cube`, `Light` and `Camera`. Until then the game is
   still linking or loading, and `save` is refused.

## Doctor

Run this first whenever anything looks off (no answer, wrong behavior, a build that seems stale):

```powershell
pwsh -NoProfile -File .claude/skills/verify-besfa/doctor.ps1 -Project <project>
```

It checks, read-only: the manifest is patched to this checkout. The scene file
exists. `target/debug/besfa.exe` is newer than the last CLI/MCP commit. Whether
another build is compiling into the shared target, and whether `besfa_editor.exe` is running.
Whether this run's game exe is up. Every `FAIL` line must be fixed before driving. A
`WARN` about another build only means waiting.

`mcp__besfa__send` errors also diagnose: "The game is not up yet" means it is
still building. "The game exited" comes with the log, so read the first error
lines it shows.

## Drive

Use the `mcp__besfa__*` tools (this repo's `.mcp.json` serves them from
`cargo run -p besfa_mcp`):

- `entities`: no args lists scene entities (`id`, `name`, `parent`, `scene` =
  kept by the scene file, `asset`). Filter with `name`, `asset` or `parent`. With `id`, it returns that
  entity's components as JSON and selects it.
- `send` with `commands: [...]` runs editor commands in order and returns the reports. Its tool
  description lists every command and its fields. The ones the map uses:
  `spawn`, `set`, `instantiate`, `duplicate`, `delete`, `restore`, `save`,
  `save_prefab`, `screenshot`, `thumbnail`, `terrain_stroke`, `terrain_fill`,
  `heights`, `scatter`, `scatter_erase`, `play`, `key`.
- `stop` kills the game and cargo. Unsaved changes are gone.

Address entities by the ids that `spawned` reports or that `entities` lists. Ids
change on every start, so look them up again after a restart. A failed command
comes back as `{"type":"done","command":N,"error":...}`. Treat that as a failure
of the step. Do not retry it with different input until it passes.

**When the change is in `crates/besfa_mcp` or `crates/besfa_cli`,** the session's
MCP server was built before your change. Build a copy and drive it over stdio
with the helper:

```bash
CARGO_TARGET_DIR=<run folder>/mcp_target cargo build -q -p besfa_mcp
python -I .claude/skills/verify-besfa/mcp.py <run folder>/mcp_target/debug/besfa_mcp.exe <<'EOF'
{"name":"start","arguments":{"project":"<project>"}}
{"name":"entities","arguments":{"timeout_secs":300}}
EOF
```

Each input line is one tool call. The helper prints each result (with `ERROR ` in front when it failed) and stops
the game when its input ends. Use the same helper when the session has no
`mcp__besfa__` tools.

## Evidence

Put proof in `<checkout>/target/verify-besfa/<run name>/`. `target/` is git-ignored and
lives outside the throwaway project, so cleanup cannot delete it. For each
feature you verify, keep:

- the command batch you sent and the reports it returned (`<feature>.reports.txt`),
- the stored side effect, read back from disk. For a save, that is the entity's
  block from `assets/scenes/main.scn.ron` (or the `.terrain` file, prefab file or PNG).
  A `saved` report alone is not proof,
- a second view after a restart for anything that must persist: `stop`, `start`,
  `entities` shows it again,
- a `screenshot` (written relative to `<project>`, so copy it out) when the change
  is visible. Read the PNG and look at it: a "saved" report says nothing about
  what renders.

Proof standards: drive the path a user's action takes (the editor command the
UI sends), not test-only setters. Capture the action and the resulting state. Check side
effects on disk, not only the reports.

## Cleanup

1. `mcp__besfa__stop` (or end the helper's input). If a game is still up, the doctor names its pid.
   Kill it by pid from PowerShell with `taskkill /PID <pid> /T /F`. Never kill by image name, because other
   sessions run games and MCP servers too. Never use `taskkill` from Git Bash, which
   mangles `/T /F`.
2. Copy anything still inside `<project>` that you want as evidence. Then delete `<project>`
   and `<run folder>/mcp_target` if you made it.
3. Remove this run's artifacts from the shared target. They are ~170 MB per name,
   and only this run's unique name matches them:
   ```powershell
   $n = '<run name>'; $d = "$env:LOCALAPPDATA\Besfa\target\debug"
   Remove-Item -Recurse -Force "$d\$n.*","$d\deps\$n*","$d\incremental\$n-*","$d\.fingerprint\$n-*" -ErrorAction SilentlyContinue
   ```
   Never run `cargo clean` in a game project: it wipes the shared target that
   every game and the user's editor use.
4. Confirm `target/verify-besfa/<run name>/` still holds the evidence.

## Isolation

- One game per MCP server. Several sessions can each run their own, since
  names are unique and cargo locks the shared target.
- Never point `start` at a project the user's editor has open (both would write its
  scene file), at `C:\codes\duskhold`, or at any project the user owns. Use throwaway projects only.
- duskhold models (`C:\codes\duskhold\assets\props\*.glb`, `assets\enemies\zombie-runner.glb`
  with idle/run/walk clips) may be **copied** into `<project>/assets/models/` as test assets.

## Gotchas

- Edit mode pauses virtual time, so anything time-driven (foliage wind, clips,
  spinning cube) is frozen until `play`. After `play`, the template's spinning Cube
  changes pixels, so delete it before diffing screenshots.
- `screenshot` waits for models to load, but in a debug build a model spawned just
  before can still be compiling pipelines. Shoot again after a second if a model is missing.
- Reports are written on change only. `send` already collects them per batch, so
  do not expect a report to repeat. A command that only succeeds, such as `play` or `restore`,
  returns no report at all. Only a failure shows up, as a `done` report with an `error`.
- Placing a prefab and then changing its file: Bevy ignores the asset's
  `Modified` for ~3 frames after `Added`. Wait ~1 s between them.
- A workspace build that sees an older `besfa_protocol` than the file shows
  ("no variant named ..."): `touch crates/besfa_protocol/src/lib.rs`.
- `new-game.ps1` builds this checkout's CLI into the workspace `target/`, but it does not
  install it. A change to the project template is tested by running
  `new-game.ps1` again.

## The editor UI (secondary, not driven by this recipe)

For hub, panel, menu or dialog behavior, run `cargo run -p besfa_editor` with
`APPDATA` pointed at a scratch folder holding `Besfa/recent_projects.json` (a JSON list of
project paths), so the user's recent list stays untouched. Click
through Windows UI Automation (`InvokePattern` on buttons, `TogglePattern` on
selectable rows, an empty-named `ComboBox`), and capture with `PrintWindow`,
never a screen grab. Launching takes the user's focus, and its startup prebuild can take 15 min, so do it
only when the change is in `crates/besfa_editor` and a kittest
(`egui_kittest`, see the tests in `crates/besfa_editor/src/*.rs`) cannot show it.
This path was not part of this skill's proof run.
