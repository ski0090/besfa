//! Holds the game still until the editor says play, and takes the editor's
//! other commands.
//!
//! The editor sets `BESFA_EDIT_MODE` and the game starts with virtual time
//! paused: Startup systems build the scene, `Update` sees no time pass and
//! `FixedUpdate` does not run. The editor writes one command per line to
//! stdin, a JSON object such as `{"command":"play"}`; see [`Command`]. The
//! editor ends a play session by killing the process, which resets the scene.

use std::sync::{Mutex, PoisonError, mpsc};

use bevy::prelude::*;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    edit::{self, SpawnKind},
    inspect::{Changes, Selected, report_saved, select, send_report},
    scene::{SCENE_PATH, asset_file, save, save_prefab, scene_file},
    scene_view::{self, PointerEvent, SceneView, Tool},
    viewport,
};

pub(crate) fn requested() -> bool {
    std::env::var_os("BESFA_EDIT_MODE").is_some()
}

// ponytail: pauses virtual time only, so systems that ignore `Time` or read
// `Time<Real>` keep running. Gate gameplay on an edit/play `State` if that
// starts to matter.
pub(crate) struct EditModePlugin;

/// Lines the editor wrote to stdin, one command each.
#[derive(Resource)]
pub(crate) struct EditorCommands(pub(crate) Mutex<mpsc::Receiver<String>>);

impl Plugin for EditModePlugin {
    fn build(&self, app: &mut App) {
        let (sender, receiver) = mpsc::channel();
        // ponytail: the editor talks over the game's stdin and stdout (see
        // `inspect` for the replies). A besfa_protocol crate can take over
        // once the editor and the game need shared message types.
        std::thread::spawn(move || {
            for line in std::io::stdin().lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    return;
                }
            }
        });

        app.insert_resource(EditorCommands(Mutex::new(receiver)))
            .init_resource::<Selected>()
            .init_resource::<Changes>()
            .add_systems(PreStartup, pause)
            .add_systems(PreUpdate, run_commands);
    }
}

fn pause(mut time: ResMut<Time<Virtual>>) {
    time.pause();
}

/// One line from the editor. Entities are the ids `inspect` reports, and
/// components their reflected type paths.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "command", rename_all = "snake_case")]
enum Command {
    /// Unpauses virtual time once, so a game that pauses itself stays
    /// paused, and hands the view to the game's cameras.
    Play,
    /// Picks the entity whose components `inspect` reports; no id clears it.
    Select {
        id: Option<u64>,
    },
    /// Writes the scene file and reports whether that worked.
    Save,
    Set {
        id: u64,
        component: String,
        value: Value,
    },
    Insert {
        id: u64,
        component: String,
    },
    Remove {
        id: u64,
        component: String,
    },
    /// Spawns a scene entity, reports it and selects it.
    Spawn {
        kind: SpawnKind,
    },
    /// Copies an entity, reports the copy and selects it.
    Duplicate {
        id: u64,
    },
    /// Hides an entity and its children until `restore`.
    Delete {
        id: u64,
    },
    Restore {
        id: u64,
    },
    /// Drops a deleted entity for good, once no undo can restore it.
    Despawn {
        id: u64,
    },
    /// Spawns an entity showing the asset at `path`, relative to the asset
    /// directory, then reports and selects it.
    Instantiate {
        path: String,
    },
    /// Writes an entity and its scene children to `path`, relative to the
    /// asset directory, as a prefab.
    SavePrefab {
        id: u64,
        path: String,
    },
    /// The viewport's pointer, for the scene view.
    Pointer(PointerEvent),
    /// What dragging a handle does.
    Tool {
        tool: Tool,
    },
    /// Frames the selected entity in the scene view.
    Focus,
    /// Renders into the editor's new viewport texture of this name.
    Viewport {
        name: String,
    },
}

/// Exclusive, because commands change and save the whole world.
fn run_commands(world: &mut World) {
    let lines: Vec<String> = {
        let commands = world.resource::<EditorCommands>();
        let receiver = commands.0.lock().unwrap_or_else(PoisonError::into_inner);
        receiver.try_iter().collect()
    };
    for line in lines {
        match serde_json::from_str(&line) {
            Ok(command) => run(world, command),
            Err(error) => warn!("Unknown editor command {line}: {error}"),
        }
    }
}

fn run(world: &mut World, command: Command) {
    // Counted whether or not they apply: the editor counts what it sent.
    if matches!(
        command,
        Command::Set { .. } | Command::Insert { .. } | Command::Remove { .. }
    ) {
        world.resource_mut::<Changes>().0 += 1;
    }
    let result = match command {
        Command::Play => {
            world.resource_mut::<Time<Virtual>>().unpause();
            scene_view::stop(world);
            info!("Playing.");
            Ok(())
        }
        Command::Select { id } => {
            world.resource_mut::<Selected>().0 = id.and_then(Entity::try_from_bits);
            Ok(())
        }
        Command::Save => {
            let result = save(world, &scene_file());
            match &result {
                Ok(()) => info!("Saved {SCENE_PATH}"),
                Err(error) => error!("Could not save {SCENE_PATH}: {error}"),
            }
            report_saved(result.err());
            return;
        }
        Command::Set {
            id,
            component,
            value,
        } => entity(id).and_then(|entity| edit::set(world, entity, &component, &value)),
        Command::Insert { id, component } => {
            entity(id).and_then(|entity| edit::insert(world, entity, &component))
        }
        Command::Remove { id, component } => {
            entity(id).and_then(|entity| edit::remove(world, entity, &component))
        }
        Command::Spawn { kind } => {
            let entity = edit::spawn(world, kind);
            spawned(world, entity);
            Ok(())
        }
        Command::Duplicate { id } => entity(id)
            .and_then(|entity| edit::duplicate(world, entity))
            .map(|copy| spawned(world, copy)),
        Command::Instantiate { path } => {
            edit::instantiate(world, &path).map(|entity| spawned(world, entity))
        }
        Command::Delete { id } => entity(id)
            .and_then(|entity| edit::delete(world, entity))
            .map(|deleted| {
                let selected = world.resource::<Selected>().0;
                if selected.is_some_and(|selected| deleted.contains(&selected)) {
                    select(world, None);
                }
            }),
        Command::Restore { id } => entity(id).and_then(|entity| edit::restore(world, entity)),
        Command::Despawn { id } => entity(id).and_then(|entity| edit::despawn(world, entity)),
        Command::SavePrefab { id, path } => {
            let result = asset_path(&path)
                .and_then(|path| entity(id).map(|entity| (entity, path)))
                .and_then(|(entity, file)| save_prefab(world, entity, &file));
            match &result {
                Ok(()) => info!("Saved {path}"),
                Err(error) => error!("Could not save {path}: {error}"),
            }
            send_report(serde_json::json!({
                "type": "prefab_saved",
                "path": path,
                "error": result.err(),
            }));
            return;
        }
        Command::Pointer(event) => {
            if let Some(mut view) = world.get_resource_mut::<SceneView>() {
                view.events.push(event);
            }
            Ok(())
        }
        Command::Tool { tool } => {
            if let Some(mut view) = world.get_resource_mut::<SceneView>() {
                view.tool = tool;
            }
            Ok(())
        }
        Command::Focus => {
            if let Some(mut view) = world.get_resource_mut::<SceneView>() {
                view.focus = true;
            }
            Ok(())
        }
        Command::Viewport { name } => viewport::reopen(world, &name),
    };
    if let Err(error) = result {
        error!("Could not apply the editor's change: {error}");
    }
}

/// Tells the editor about an entity it asked for, then selects it.
fn spawned(world: &mut World, entity: Entity) {
    send_report(serde_json::json!({ "type": "spawned", "id": entity.to_bits() }));
    select(world, Some(entity));
}

/// A file under the asset directory, named the way asset paths are.
fn asset_path(path: &str) -> Result<std::path::PathBuf, String> {
    let inside = !path.starts_with('/')
        && !path.contains(':')
        && path
            .split(['/', '\\'])
            .all(|part| !part.is_empty() && part != "..");
    if inside && path.ends_with(".scn.ron") {
        Ok(asset_file(path))
    } else {
        Err(format!(
            "{path} is not a .scn.ron path inside the asset directory"
        ))
    }
}

fn entity(id: u64) -> Result<Entity, String> {
    Entity::try_from_bits(id).ok_or_else(|| format!("{id} is not an entity id"))
}

#[cfg(test)]
mod tests {
    use bevy::time::TimePlugin;

    use super::*;

    fn editor_app() -> (App, mpsc::Sender<String>) {
        let mut app = App::new();
        app.add_plugins((TimePlugin, EditModePlugin));
        // Replaces stdin with the test's channel.
        let (sender, receiver) = mpsc::channel();
        app.insert_resource(EditorCommands(Mutex::new(receiver)));
        (app, sender)
    }

    fn paused(app: &App) -> bool {
        app.world().resource::<Time<Virtual>>().is_paused()
    }

    #[test]
    fn starts_paused_and_plays_once_asked() {
        let (mut app, editor) = editor_app();
        app.update();
        assert!(paused(&app));

        editor.send(r#"{"command":"play"}"#.into()).unwrap();
        app.update();
        assert!(!paused(&app));

        // The request is used up: the game's own pause sticks.
        app.world_mut().resource_mut::<Time<Virtual>>().pause();
        app.update();
        assert!(paused(&app));
    }

    #[test]
    fn selects_an_entity_by_id_and_clears_on_a_bare_select() {
        let (mut app, editor) = editor_app();
        let cube = app.world_mut().spawn_empty().id();

        editor
            .send(format!(r#"{{"command":"select","id":{}}}"#, cube.to_bits()))
            .unwrap();
        app.update();
        assert_eq!(app.world().resource::<Selected>().0, Some(cube));

        editor.send(r#"{"command":"select"}"#.into()).unwrap();
        app.update();
        assert_eq!(app.world().resource::<Selected>().0, None);
    }

    #[test]
    fn reads_every_command() {
        let command = |line: &str| serde_json::from_str::<Command>(line).unwrap();

        assert_eq!(
            command(r#"{"command":"set","id":4,"component":"a::B<c::D, e::F>","value":{"x":1}}"#),
            Command::Set {
                id: 4,
                component: "a::B<c::D, e::F>".into(),
                value: serde_json::json!({ "x": 1 }),
            }
        );
        assert_eq!(
            command(r#"{"command":"spawn","kind":"cube"}"#),
            Command::Spawn {
                kind: SpawnKind::Cube
            }
        );
        assert_eq!(command(r#"{"command":"save"}"#), Command::Save);
        assert_eq!(
            command(r#"{"command":"pointer","event":"scroll","delta":-120}"#),
            Command::Pointer(PointerEvent::Scroll { delta: -120.0 })
        );
        assert_eq!(
            command(r#"{"command":"tool","tool":"rotate"}"#),
            Command::Tool { tool: Tool::Rotate }
        );
        assert!(serde_json::from_str::<Command>("play").is_err());
    }

    #[test]
    fn keeps_prefabs_inside_the_asset_directory() {
        assert!(asset_path("prefabs/tree.scn.ron").is_ok());
        assert!(asset_path("../tree.scn.ron").is_err());
        assert!(asset_path("C:/tree.scn.ron").is_err());
        assert!(asset_path("/tree.scn.ron").is_err());
        assert!(asset_path("prefabs/tree.txt").is_err());
        assert!(asset_path("prefabs//tree.scn.ron").is_err());
    }

    #[test]
    fn spawns_and_selects_from_a_command() {
        let (mut app, editor) = editor_app();
        app.register_type::<Transform>();

        editor
            .send(r#"{"command":"spawn","kind":"empty"}"#.into())
            .unwrap();
        app.update();

        let spawned = app.world().resource::<Selected>().0.expect("selected");
        assert_eq!(app.world().get::<Name>(spawned).unwrap().as_str(), "Entity");
        let set = serde_json::json!({
            "command": "set",
            "id": spawned.to_bits(),
            "component": "bevy_transform::components::transform::Transform",
            "value": { "translation": [1, 2, 3] },
        });
        editor.send(set.to_string()).unwrap();
        editor
            .send(r#"{"command":"remove","id":1,"component":"no::Such"}"#.into())
            .unwrap();
        app.update();
        assert_eq!(
            app.world().get::<Transform>(spawned).unwrap().translation,
            Vec3::new(1.0, 2.0, 3.0)
        );
        assert_eq!(app.world().resource::<Changes>().0, 2, "failures count too");
    }
}
