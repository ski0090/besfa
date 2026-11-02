---------------------------- MODULE SaveHandshake ----------------------------
(***************************************************************************)
(* The editor's promise about scene changes: an edit the user made is      *)
(* never thrown away unless the user chose Discard.                        *)
(*                                                                         *)
(* Models the editor (crates/besfa_editor/src/editor.rs, scene.rs), the    *)
(* game in edit mode (crates/besfa_editor_plugin/src/edit_mode.rs) and the *)
(* game's stdin and stdout between them, which keep their order, plus      *)
(* every way the editor ends the game process. An edit is any change the   *)
(* scene file keeps; which one does not matter, only whether the file has  *)
(* it when the game process ends.                                          *)
(***************************************************************************)
EXTENDS Naturals, Sequences

CONSTANTS
    MaxEdits,       \* edits the user makes in one run
    MaxRestarts,    \* game relaunches in one run
    SaveCanFail,    \* the game's save can fail (disk error, scene not loaded yet)
    SaveCanTimeOut, \* the game can take longer than SAVE_TIMEOUT (5 s) to answer a save
    PatientUser     \* the user acts only while nothing is in flight between editor and game

VARIABLES
    \* The editor
    dirty,      \* Scene.dirty: the title's " *"; leaving asks first only while it is set
    pending,    \* Editor.pending_save: "none" or what follows the save in flight
    playing,    \* Editor.playing
    starting,   \* Editor.starting: a restart is under way; the toolbar is disabled
    updating,   \* Editor.update: `cargo update -p besfa_editor_plugin` runs
    relaunch,   \* Editor.relaunch: "none", or launch "edit"/"play" once the game exited
    waitRebuild,\* Editor.rebuild_after_save: a rebuild waits for the scene to save
    dialog,     \* Editor.dialog: "none", or Dialog::Unsaved before "Close"/"Leave"
    hasGame,    \* Editor.game.is_some(), until the editor reads the game's exit
    ui,         \* "editor", "closed" (window closed) or "hub" (back to the hub)
    \* The pipes
    toGame,     \* the game's stdin: [k |-> "edit"/"drag", id |-> n], [k |-> "save"/"play"/"reset", id |-> 0]
    toEditor,   \* the game's stdout: [k |-> "saved", ok |-> b], [k |-> "edited", ok |-> TRUE]
    \* The game process
    game,       \* "edit" (paused), "play" or "down"
    killed,     \* taskkill was sent; until it lands the game still runs commands
    scene,      \* edits in the game's world
    file,       \* edits in assets/scenes/main.scn.ron
    \* Bookkeeping for the rule, not in the code
    made,       \* edits the user made that exist in this edit session
    waived,     \* edits the user chose to Discard
    cause,      \* what is ending the game
    lost,       \* each cause that ended the game while an edit was missing from the file
    nextEdit,
    restarts

vars == <<dirty, pending, playing, starting, updating, relaunch, dialog, hasGame, ui,
          toGame, toEditor, game, killed, scene, file, made, waived, cause, lost,
          nextEdit, restarts, waitRebuild>>

Save == [k |-> "save", id |-> 0]
Reset == [k |-> "reset", id |-> 0]
Unsaved == (made \ file) \ waived

\* The game process ends now, by `why`; what the file lacks is lost.
Lose(why) == lost' = IF Unsaved # {} THEN lost \cup {why} ELSE lost

Init ==
    /\ dirty = FALSE /\ pending = "none" /\ playing = FALSE /\ starting = FALSE
    /\ updating = FALSE /\ relaunch = "none" /\ dialog = "none" /\ hasGame = TRUE
    /\ ui = "editor" /\ toGame = <<>> /\ toEditor = <<>> /\ game = "edit"
    /\ killed = FALSE /\ scene = {} /\ file = {} /\ made = {} /\ waived = {}
    /\ cause = "none" /\ lost = {} /\ nextEdit = 1 /\ restarts = 0 /\ waitRebuild = FALSE

(* Editor::restart: stop the game (taskkill, which lands later) and launch *)
(* `mode` once it exited. Leaves the variables it does not set to the      *)
(* caller.                                                                 *)
Restart(mode, why) ==
    /\ starting' = TRUE
    /\ relaunch' = mode
    /\ killed' = hasGame
    /\ cause' = why

(* The window closes or the editor goes back to the hub. Either way the    *)
(* Editor and its game go with it.                                         *)
End(where, why) ==
    /\ ui' = where
    /\ game' = "down"
    /\ Lose(why)

(* Editor::save: sends Save while there is an edit session. *)
SendSave == toGame' = IF hasGame /\ ~playing THEN Append(toGame, Save) ELSE toGame

(* Editor::after_save: what follows a save, with `d` the dirty mark after *)
(* it. A restart, a Play or a plugin update goes only once the save       *)
(* cleared the mark. Otherwise the game keeps the changes, and a rebuild  *)
(* waits for the scene to save.                                           *)
AfterSave(then, d) ==
    CASE then \in {"Rebuild", "Play", "NetPlay", "Update"} /\ d ->
            /\ starting' = FALSE
            /\ waitRebuild' = (waitRebuild \/ then = "Rebuild")
            /\ UNCHANGED <<updating, relaunch, killed, cause, ui, game, lost, playing, toGame>>
      [] then = "Rebuild" /\ ~d ->
            /\ Restart("edit", "rebuild")
            /\ UNCHANGED <<updating, ui, game, lost, playing, toGame, waitRebuild>>
      [] then = "NetPlay" /\ ~d ->
            /\ Restart("play", "netplay")
            /\ UNCHANGED <<updating, ui, game, lost, playing, toGame, waitRebuild>>
      \* Editor::play again, which now plays.
      [] then = "Play" /\ ~d ->
            /\ toGame' = Append(toGame, [k |-> "play", id |-> 0])
            /\ playing' = TRUE
            /\ starting' = FALSE
            /\ UNCHANGED <<updating, relaunch, killed, cause, ui, game, lost, waitRebuild>>
      [] then = "Update" /\ ~d ->
            /\ updating' = TRUE
            /\ UNCHANGED <<starting, relaunch, killed, cause, ui, game, lost, playing, toGame,
                           waitRebuild>>
      [] then \in {"Close", "Leave"} /\ ~d ->
            /\ End(IF then = "Close" THEN "closed" ELSE "hub", IF then = "Close" THEN "close" ELSE "leave")
            /\ UNCHANGED <<starting, updating, relaunch, killed, cause, playing, toGame, waitRebuild>>
      [] OTHER ->
            UNCHANGED <<starting, updating, relaunch, killed, cause, ui, game, lost, playing,
                        toGame, waitRebuild>>

(* Editor::rebuild past its guards: save first and wait, or restart. *)
Rebuild ==
    /\ waitRebuild' = FALSE
    /\ IF hasGame /\ dirty
         THEN /\ starting' = TRUE
              /\ SendSave
              /\ pending' = "Rebuild"
              /\ UNCHANGED <<relaunch, killed, cause>>
         ELSE /\ Restart("edit", "rebuild")
              /\ UNCHANGED <<toGame, pending>>

-----------------------------------------------------------------------------
(* What the user does *)

(* A patient user waits until the game has run every command, the editor *)
(* has read every report, and no save, restart or plugin update is under  *)
(* way. It separates what goes wrong whatever the timing from races.      *)
YouMayAct ==
    \/ ~PatientUser
    \/ /\ toGame = <<>> /\ toEditor = <<>> /\ pending = "none"
       /\ ~starting /\ ~updating /\ ~killed

(* Inspector fields, Hierarchy add/duplicate/delete, Undo and Redo:        *)
(* Editor::edit and Editor::replay send the command and set the mark. Not  *)
(* blocked by `starting`: only the toolbar is.                             *)
YouEdit ==
    /\ YouMayAct
    /\ ui = "editor" /\ dialog = "none" /\ hasGame /\ ~playing
    /\ nextEdit <= MaxEdits
    /\ toGame' = Append(toGame, [k |-> "edit", id |-> nextEdit])
    /\ made' = made \cup {nextEdit}
    /\ dirty' = TRUE
    /\ nextEdit' = nextEdit + 1
    /\ UNCHANGED <<pending, playing, starting, updating, relaunch, dialog, hasGame, ui,
                   toEditor, game, killed, scene, file, waived, cause, lost, restarts, waitRebuild>>

(* A handle drag, terrain stroke or scatter stroke in the scene view: the *)
(* editor sends the pointer input down the same stdin; the game changes   *)
(* its world and reports it (Edited, Moved, TerrainPainted, Scattered),   *)
(* which sets the mark when the editor reads it.                          *)
YouDragInSceneView ==
    /\ YouMayAct
    /\ ui = "editor" /\ dialog = "none" /\ hasGame /\ ~playing
    /\ nextEdit <= MaxEdits
    /\ toGame' = Append(toGame, [k |-> "drag", id |-> nextEdit])
    /\ made' = made \cup {nextEdit}
    /\ nextEdit' = nextEdit + 1
    /\ UNCHANGED <<dirty, pending, playing, starting, updating, relaunch, dialog, hasGame, ui,
                   toEditor, game, killed, scene, file, waived, cause, lost, restarts, waitRebuild>>

(* Ctrl+S, File > Save scene. *)
YouSave ==
    /\ YouMayAct
    /\ ui = "editor" /\ dialog = "none" /\ hasGame /\ ~playing
    /\ SendSave
    /\ UNCHANGED <<dirty, pending, playing, starting, updating, relaunch, dialog, hasGame, ui,
                   toEditor, game, killed, scene, file, made, waived, cause, lost, nextEdit, restarts, waitRebuild>>

(* Toolbar Play, Standalone: Editor::play saves first and plays once the *)
(* game reports the save.                                                  *)
YouPressPlay ==
    /\ YouMayAct
    /\ ui = "editor" /\ dialog = "none" /\ ~starting /\ ~playing /\ hasGame
    /\ IF dirty
         THEN /\ starting' = TRUE
              /\ SendSave
              /\ pending' = "Play"
              /\ UNCHANGED playing
         ELSE /\ toGame' = Append(toGame, [k |-> "play", id |-> 0])
              /\ playing' = TRUE
              /\ UNCHANGED <<starting, pending>>
    /\ UNCHANGED <<dirty, updating, relaunch, dialog, hasGame, ui, toEditor, game, killed,
                   scene, file, made, waived, cause, lost, nextEdit, restarts, waitRebuild>>

(* Toolbar Play with "Server + N clients": waits for the save, then restarts. *)
YouPressNetPlay ==
    /\ YouMayAct
    /\ ui = "editor" /\ dialog = "none" /\ ~starting /\ ~playing
    /\ IF hasGame /\ dirty
         THEN /\ starting' = TRUE
              /\ SendSave
              /\ pending' = "NetPlay"
              /\ UNCHANGED <<relaunch, killed, cause>>
         ELSE /\ Restart("play", "netplay")
              /\ UNCHANGED <<toGame, pending>>
    /\ UNCHANGED <<dirty, playing, updating, dialog, hasGame, ui, toEditor, game, scene, file,
                   made, waived, lost, nextEdit, restarts, waitRebuild>>

(* Toolbar Stop, Editor::stop: relaunches the game in edit mode from the  *)
(* file, or, when no new process is needed, sends Reset and clears the     *)
(* editor's scene and mark at once; the game reloads the file once it gets *)
(* to the command.                                                         *)
YouPressStop ==
    /\ YouMayAct
    /\ ui = "editor" /\ dialog = "none" /\ ~starting /\ playing
    /\ \/ /\ Restart("edit", "stop")
          /\ UNCHANGED <<dirty, playing, toGame>>
       \/ /\ hasGame
          /\ playing' = FALSE
          /\ dirty' = FALSE
          /\ toGame' = Append(toGame, Reset)
          /\ UNCHANGED <<starting, relaunch, killed, cause>>
    /\ UNCHANGED <<pending, updating, dialog, hasGame, ui, toEditor, game, scene, file, made,
                   waived, lost, nextEdit, restarts, waitRebuild>>

(* A file under src/ changed: Editor::rebuild. Not blocked by the dialog. *)
YouChangeCode ==
    /\ YouMayAct
    /\ ui = "editor" /\ ~playing /\ ~starting
    /\ Rebuild
    /\ UNCHANGED <<dirty, playing, updating, dialog, hasGame, ui, toEditor, game, scene, file,
                   made, waived, lost, nextEdit, restarts>>

(* The plugin status icon (a check mark or warning): Editor::update_plugin *)
(* saves unsaved changes and waits for that, then runs cargo update, then  *)
(* restarts.                                                                *)
YouClickPluginIcon ==
    /\ YouMayAct
    /\ ui = "editor" /\ dialog = "none" /\ ~starting /\ ~updating
    /\ starting' = TRUE
    /\ IF hasGame /\ ~playing /\ dirty
         THEN /\ SendSave
              /\ pending' = "Update"
              /\ UNCHANGED updating
         ELSE /\ updating' = TRUE
              /\ UNCHANGED <<toGame, pending>>
    /\ UNCHANGED <<dirty, playing, relaunch, dialog, hasGame, ui, toEditor,
                   game, killed, scene, file, made, waived, cause, lost, nextEdit, restarts, waitRebuild>>

PluginUpdated ==
    /\ updating
    /\ updating' = FALSE
    /\ Restart("edit", "update")
    /\ UNCHANGED <<dirty, pending, playing, dialog, hasGame, ui, toGame, toEditor, game,
                   scene, file, made, waived, lost, nextEdit, restarts, waitRebuild>>

(* The window's close button or Alt+F4: asks only while the mark is set. *)
YouCloseWindow ==
    /\ YouMayAct
    /\ ui = "editor"
    /\ IF dirty
         THEN /\ dialog = "none"
              /\ dialog' = "Close"
              /\ UNCHANGED <<ui, game, lost>>
         ELSE /\ End("closed", "close")
              /\ UNCHANGED dialog
    /\ UNCHANGED <<dirty, pending, playing, starting, updating, relaunch, hasGame, toGame,
                   toEditor, killed, scene, file, made, waived, cause, nextEdit, restarts, waitRebuild>>

(* File > Back to Project Hub: Editor::request_leave. *)
YouGoToHub ==
    /\ YouMayAct
    /\ ui = "editor" /\ dialog = "none"
    /\ IF dirty
         THEN /\ dialog' = "Leave"
              /\ UNCHANGED <<ui, game, lost>>
         ELSE /\ End("hub", "leave")
              /\ UNCHANGED dialog
    /\ UNCHANGED <<dirty, pending, playing, starting, updating, relaunch, hasGame, toGame,
                   toEditor, killed, scene, file, made, waived, cause, nextEdit, restarts, waitRebuild>>

YouCancel ==
    /\ YouMayAct
    /\ ui = "editor" /\ dialog # "none"
    /\ dialog' = "none"
    /\ UNCHANGED <<dirty, pending, playing, starting, updating, relaunch, hasGame, ui, toGame,
                   toEditor, game, killed, scene, file, made, waived, cause, lost, nextEdit, restarts, waitRebuild>>

YouDiscard ==
    /\ YouMayAct
    /\ ui = "editor" /\ dialog # "none"
    /\ dialog' = "none"
    /\ waived' = made
    /\ ui' = IF dialog = "Close" THEN "closed" ELSE "hub"
    /\ game' = "down"
    /\ UNCHANGED <<dirty, pending, playing, starting, updating, relaunch, hasGame, toGame,
                   toEditor, killed, scene, file, made, cause, lost, nextEdit, restarts, waitRebuild>>

(* The dialog's Save, shown only with an edit session that is not playing. *)
YouChooseSave ==
    /\ YouMayAct
    /\ ui = "editor" /\ dialog # "none" /\ hasGame /\ ~playing
    /\ dialog' = "none"
    /\ SendSave
    /\ pending' = dialog
    /\ UNCHANGED <<dirty, playing, starting, updating, relaunch, hasGame, ui, toEditor, game,
                   killed, scene, file, made, waived, cause, lost, nextEdit, restarts, waitRebuild>>

-----------------------------------------------------------------------------
(* What the game does: edit_mode.rs run_commands, one command at a time, in order *)

GameRunsCommand ==
    /\ game # "down" /\ toGame # <<>> /\ Head(toGame).k # "reset"
    /\ LET c == Head(toGame) IN
         /\ toGame' = Tail(toGame)
         /\ CASE c.k = "edit" ->
                    /\ scene' = scene \cup {c.id}
                    /\ UNCHANGED <<file, toEditor, game>>
              [] c.k = "drag" ->
                    /\ scene' = scene \cup {c.id}
                    /\ toEditor' = Append(toEditor, [k |-> "edited", ok |-> TRUE])
                    /\ UNCHANGED <<file, game>>
              [] c.k = "save" ->
                    /\ file' = scene
                    /\ toEditor' = Append(toEditor, [k |-> "saved", ok |-> TRUE])
                    /\ UNCHANGED <<scene, game>>
              [] c.k = "play" ->
                    /\ game' = "play"
                    /\ UNCHANGED <<scene, file, toEditor>>
    /\ UNCHANGED <<dirty, pending, playing, starting, updating, relaunch, dialog, hasGame, ui,
                   killed, made, waived, cause, lost, nextEdit, restarts, waitRebuild>>

GameFailsToSave ==
    /\ SaveCanFail
    /\ game # "down" /\ toGame # <<>> /\ Head(toGame).k = "save"
    /\ toGame' = Tail(toGame)
    /\ toEditor' = Append(toEditor, [k |-> "saved", ok |-> FALSE])
    /\ UNCHANGED <<dirty, pending, playing, starting, updating, relaunch, dialog, hasGame, ui,
                   game, killed, scene, file, made, waived, cause, lost, nextEdit, restarts, waitRebuild>>

(* edit_mode.rs reset: the world goes back to the file. What the scene had *)
(* and the file lacked is lost; edits queued after the Reset still come.   *)
GameResets ==
    /\ game # "down" /\ toGame # <<>> /\ Head(toGame).k = "reset"
    /\ toGame' = Tail(toGame)
    /\ game' = "edit"
    /\ scene' = file
    /\ made' = (made \ scene) \cup file
    /\ lost' = IF (scene \ file) \ waived # {} THEN lost \cup {"stop"} ELSE lost
    /\ UNCHANGED <<dirty, pending, playing, starting, updating, relaunch, dialog, hasGame, ui,
                   toEditor, killed, file, waived, cause, nextEdit, restarts, waitRebuild>>

(* taskkill lands: the process and its stdin are gone. *)
GameIsKilled ==
    /\ killed /\ game # "down"
    /\ game' = "down"
    /\ toGame' = <<>>
    /\ Lose(cause)
    /\ UNCHANGED <<dirty, pending, playing, starting, updating, relaunch, dialog, hasGame, ui,
                   toEditor, killed, scene, file, made, waived, cause, nextEdit, restarts, waitRebuild>>

-----------------------------------------------------------------------------
(* What the editor does on its own *)

(* poll_game: Scene::handle, then Editor::on_report. Later in the same   *)
(* frame, before any input, Editor::ui runs a rebuild that waited once    *)
(* the mark is clear.                                                     *)
EditorReadsReport ==
    /\ ui = "editor" /\ hasGame /\ toEditor # <<>>
    /\ LET r == Head(toEditor)
           d == IF r.k = "edited" THEN TRUE ELSE IF r.ok THEN FALSE ELSE dirty
       IN /\ toEditor' = Tail(toEditor)
          /\ dirty' = d
          /\ CASE r.k = "saved" /\ pending # "none" ->
                    /\ pending' = "none"
                    /\ AfterSave(pending, d)
               [] waitRebuild /\ ~d /\ ~playing /\ ~starting ->
                    /\ waitRebuild' = FALSE
                    /\ Restart("edit", "rebuild")
                    /\ UNCHANGED <<pending, updating, ui, game, lost, playing, toGame>>
               [] OTHER ->
                    UNCHANGED <<pending, starting, updating, relaunch, killed, cause, ui, game, lost,
                                playing, toGame, waitRebuild>>
    /\ UNCHANGED <<dialog, hasGame, scene, file, made, waived, nextEdit, restarts>>

(* SAVE_TIMEOUT passed without an answer: after_save runs anyway. *)
SaveTimesOut ==
    /\ SaveCanTimeOut
    /\ ui = "editor" /\ pending # "none"
    /\ pending' = "none"
    /\ AfterSave(pending, dirty)
    /\ UNCHANGED <<dirty, dialog, hasGame, toEditor, scene, file, made, waived, nextEdit,
                   restarts>>

(* Output::Exit, then relaunch_when_exited and Editor::launch, which resets *)
(* the scene (and its mark) and loads the file.                             *)
EditorRelaunches ==
    /\ ui = "editor" /\ hasGame /\ game = "down" /\ toEditor = <<>> /\ relaunch # "none"
    /\ game' = relaunch
    /\ playing' = (relaunch = "play")
    /\ relaunch' = "none"
    /\ starting' = FALSE
    /\ killed' = FALSE
    /\ dirty' = FALSE
    /\ scene' = file
    /\ made' = file
    /\ waived' = {}
    /\ cause' = "none"
    /\ restarts' = restarts + 1
    /\ waitRebuild' = FALSE
    /\ UNCHANGED <<pending, updating, dialog, hasGame, ui, toGame, toEditor, file, lost, nextEdit>>

Finished ==
    /\ ui # "editor"
    /\ UNCHANGED vars

Next ==
    \/ YouEdit \/ YouDragInSceneView \/ YouSave \/ YouPressPlay \/ YouPressNetPlay
    \/ YouPressStop \/ YouChangeCode \/ YouClickPluginIcon \/ PluginUpdated
    \/ YouCloseWindow \/ YouGoToHub \/ YouCancel \/ YouDiscard \/ YouChooseSave
    \/ GameRunsCommand \/ GameResets \/ GameFailsToSave \/ GameIsKilled
    \/ EditorReadsReport \/ SaveTimesOut \/ EditorRelaunches \/ Finished

Spec == Init /\ [][Next]_vars

Bounds == restarts <= MaxRestarts /\ Len(toGame) <= 4 /\ Len(toEditor) <= 3

-----------------------------------------------------------------------------
(* The rule, once per way the editor ends the game: that way never ends it *)
(* while the scene file lacks an edit the user did not Discard.            *)

NoLossOnStop == "stop" \notin lost
NoLossOnRebuild == "rebuild" \notin lost
NoLossOnNetPlay == "netplay" \notin lost
NoLossOnPluginUpdate == "update" \notin lost
NoLossOnClose == "close" \notin lost
NoLossOnLeave == "leave" \notin lost

TypeOK ==
    /\ dirty \in BOOLEAN /\ playing \in BOOLEAN /\ hasGame \in BOOLEAN
    /\ game \in {"edit", "play", "down"} /\ ui \in {"editor", "closed", "hub"}
    /\ pending \in {"none", "Rebuild", "Play", "NetPlay", "Update", "Close", "Leave"}
    /\ dialog \in {"none", "Close", "Leave"}
    /\ file \subseteq 1..MaxEdits /\ scene \subseteq 1..MaxEdits
=============================================================================
