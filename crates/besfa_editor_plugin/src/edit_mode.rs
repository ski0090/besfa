//! Holds the game still until the editor says play, and takes the editor's
//! other commands.
//!
//! The editor sets `BESFA_EDIT_MODE` and the game starts with virtual time
//! paused: Startup systems build the scene, `Update` sees no time pass and
//! `FixedUpdate` does not run. The editor writes one command per line to
//! stdin, a JSON object such as `{"command":"play"}`; see
//! [`besfa_protocol::Request`]. A request with a `seq` is answered with a
//! `done` report once it has run. The editor ends a play session with
//! `reset`, or by killing the process when the game must be rebuilt.

use std::{
    collections::{HashSet, VecDeque},
    sync::{Mutex, PoisonError, mpsc},
};

use besfa_components::TerrainData;
use besfa_protocol::{Command, Placement, Report, Request, VERSION};
use bevy::{
    ecs::resource::IsResource, input::InputSystems, prelude::*, world_serialization::DynamicWorld,
};

use crate::{
    edit, ground, input,
    inspect::{self, Changes, Selected, select, send_report},
    panels, part_changes,
    preview::{self, ModelPreview},
    reimport,
    scene::{
        SCENE_PATH, asset_file, instance_prefab, project_file, save, save_instance, save_prefab,
    },
    scene_view::{self, SceneView},
    shot::{self, Shot},
    terrain, thumbnail, viewport,
};

// ponytail: pauses virtual time only, so systems that ignore `Time` or read
// `Time<Real>` keep running. Gate gameplay on an edit/play `State` if that
// starts to matter.
pub(crate) struct EditModePlugin;

/// Lines the editor wrote to stdin, one command each.
#[derive(Resource)]
pub(crate) struct EditorCommands(pub(crate) Mutex<mpsc::Receiver<String>>);

impl Plugin for EditModePlugin {
    fn build(&self, app: &mut App) {
        // The first line, so the editor knows what this plugin understands.
        send_report(&Report::Hello { version: VERSION });
        let (sender, receiver) = mpsc::channel();
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
            // Before Bevy reads this frame's input, which the commands write.
            .add_systems(PreUpdate, run_commands.before(InputSystems));
    }

    /// Before any schedule runs, so what PreStartup spawns, like a physics
    /// plugin's context, is made again by a reset rather than kept twice.
    fn cleanup(&self, app: &mut App) {
        let world = app.world_mut();
        let all = non_resources(world);
        world.insert_resource(Baseline(all));
    }
}

fn pause(mut time: ResMut<Time<Virtual>>) {
    time.pause();
}

/// The entities that exist before any schedule runs: Bevy's own and the
/// plugins', which a reset keeps.
#[derive(Resource)]
struct Baseline(HashSet<Entity>);

/// Resources are entities too, and Bevy warns when they are despawned.
fn non_resources(world: &mut World) -> HashSet<Entity> {
    world
        .query_filtered::<Entity, Without<IsResource>>()
        .iter(world)
        .collect()
}

/// Ends play in this process: what the game and Startup made goes, the files
/// the scene is built from load again, and the startup schedules run as they
/// do in a new process. Everything the plugin kept about the old scene starts
/// over, and the editor gets the reports a new game sends.
fn reset(world: &mut World) {
    let keep = world.remove_resource::<Baseline>().unwrap();
    for entity in non_resources(world).difference(&keep.0) {
        let _ = world.try_despawn(*entity);
    }
    world.insert_resource(keep);

    world.insert_resource(Selected::default());
    world.insert_resource(SceneView::default());
    world.insert_resource(ModelPreview::default());
    world.insert_resource(terrain::Strokes::default());
    world.insert_resource(reimport::Reloads::default());
    world.insert_resource(shot::Shots::default());
    world.insert_resource(thumbnail::Thumbnails::default());
    world.remove_resource::<input::LastPointer>();
    world.insert_resource(Time::<Virtual>::default());

    reload_files(world);
    send_report(&Report::Hello { version: VERSION });
    send_report(&inspect::systems(world));
    send_report(&inspect::addable_components(world));
    world.run_schedule(PreStartup);
    world.run_schedule(Startup);
    world.run_schedule(PostStartup);
}

/// Asks for the scene and the terrain files again, as the last save left
/// them, since play may have changed what is loaded. The scene's old asset
/// goes first: it would spawn at once, before the new one arrives.
fn reload_files(world: &mut World) {
    let server = world.resource::<AssetServer>().clone();
    if let Some(scene) = server.get_handle::<DynamicWorld>(SCENE_PATH) {
        world
            .resource_mut::<Assets<DynamicWorld>>()
            .remove(scene.id());
    }
    server.reload(SCENE_PATH);
    let terrains: Vec<_> = world
        .resource::<Assets<TerrainData>>()
        .iter()
        .filter_map(|(id, _)| server.get_path(id))
        .collect();
    for path in terrains {
        server.reload(path);
    }
}

/// Exclusive, because commands change and save the whole world.
///
/// A command after pointer input waits for the next frame, once the scene
/// view has taken the input in this frame's `Update`: a stroke the pointer
/// paints is then on the ground before anything that asks about it, and
/// done before its report.
fn run_commands(world: &mut World, mut waiting: Local<VecDeque<String>>) {
    {
        let commands = world.resource::<EditorCommands>();
        let receiver = commands.0.lock().unwrap_or_else(PoisonError::into_inner);
        waiting.extend(receiver.try_iter());
    }
    let mut pointed = false;
    while let Some(line) = waiting.pop_front() {
        match serde_json::from_str::<Request>(&line) {
            Ok(Request { seq, command }) => {
                if pointed && !command.is_input() {
                    waiting.push_front(line);
                    return;
                }
                pointed |= matches!(command, Command::Pointer(_));
                let result = run(world, command);
                if let Some(seq) = seq {
                    send_report(&Report::Done {
                        seq,
                        error: result.err(),
                    });
                }
            }
            Err(error) => warn!("Unknown editor command {line}: {error}"),
        }
    }
}

/// Runs `command`, logging why it failed if it did.
fn run(world: &mut World, command: Command) -> Result<(), String> {
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
            // First, so a camera inside the model is not handed back.
            preview::close(world);
            scene_view::stop(world);
            info!("Playing.");
            Ok(())
        }
        Command::Reset => {
            reset(world);
            Ok(())
        }
        Command::Select { id } => {
            world.resource_mut::<Selected>().0 = id.and_then(Entity::try_from_bits);
            Ok(())
        }
        Command::Save => {
            let result = terrain::save(world).and_then(|()| save(world, &asset_file(SCENE_PATH)));
            match &result {
                Ok(()) => info!("Saved {SCENE_PATH}"),
                Err(error) => error!("Could not save {SCENE_PATH}: {error}"),
            }
            send_report(&Report::Saved {
                error: result.clone().err(),
            });
            return result;
        }
        Command::Set {
            id,
            component,
            value,
        } => entity(id).and_then(|entity| {
            edit::set(world, entity, &component, &value)?;
            part_changes::edited(world, entity, &component);
            Ok(())
        }),
        Command::Insert { id, component } => entity(id).and_then(|entity| {
            edit::insert(world, entity, &component)?;
            part_changes::edited(world, entity, &component);
            Ok(())
        }),
        Command::Remove { id, component } => entity(id).and_then(|entity| {
            edit::remove(world, entity, &component)?;
            part_changes::edited(world, entity, &component);
            Ok(())
        }),
        Command::Spawn { kind, place } => {
            spawn_placed(world, &place, |world| Ok(edit::spawn(world, kind)))
        }
        Command::Duplicate { id } => entity(id)
            .and_then(|entity| edit::duplicate(world, entity))
            .map(|copy| spawned(world, copy)),
        Command::Instantiate { path, place } => {
            spawn_placed(world, &place, |world| edit::instantiate(world, &path))
        }
        Command::TerrainStroke {
            id,
            tool,
            points,
            radius,
            strength,
            layer,
        } => entity(id)
            .and_then(|entity| {
                terrain::stroke(world, entity, tool, &points, radius, strength, layer)
            })
            .map(|stroke| send_report(&Report::TerrainPainted { id, stroke })),
        Command::DropToSurface { ids } => {
            let (moves, failures) = ground::drop_to_surface(world, &ids);
            send_report(&Report::Moved { moves });
            if failures.is_empty() {
                Ok(())
            } else {
                Err(failures.join(" "))
            }
        }
        Command::Scatter {
            models,
            center,
            radius,
            count,
            spacing,
            scale,
            rotate,
            align,
            seed,
        } => {
            let spread = ground::Spread {
                center: center.into(),
                radius,
                count,
                spacing,
                scale,
                rotate,
                align,
                seed,
            };
            ground::scatter(world, &models, spread).map(|added| {
                send_report(&Report::Scattered {
                    added,
                    removed: Vec::new(),
                });
            })
        }
        Command::ScatterErase { center, radius } => {
            let removed = ground::erase(world, center.into(), radius);
            send_report(&Report::Scattered {
                added: Vec::new(),
                removed,
            });
            Ok(())
        }
        Command::ScatterBrush { brush } => {
            if let Some(mut view) = world.get_resource_mut::<SceneView>() {
                view.brush_at = None;
                view.scatter = brush;
            }
            Ok(())
        }
        Command::TerrainBrush { brush } => {
            if let Some(mut view) = world.get_resource_mut::<SceneView>() {
                view.brush = brush;
            }
            Ok(())
        }
        Command::TerrainFill { id, height, layer } => entity(id)
            .and_then(|entity| terrain::fill(world, entity, height, layer))
            .map(|stroke| send_report(&Report::TerrainPainted { id, stroke })),
        Command::TerrainRestore { stroke, before } => terrain::restore(world, stroke, before),
        Command::TerrainForget { stroke } => {
            terrain::forget(world, stroke);
            Ok(())
        }
        Command::Heights { points } => {
            let heights = terrain::heights(world, &points);
            send_report(&Report::Heights { heights });
            Ok(())
        }
        Command::Parent { id, parent } => entity(id).and_then(|entity| {
            let parent = parent.map(|id| edit::parent(world, id)).transpose()?;
            edit::set_parent(world, entity, parent)
        }),
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
            send_report(&Report::PrefabSaved {
                path: Some(path),
                error: result.clone().err(),
            });
            return result;
        }
        Command::ApplyPrefab { id } => {
            let result = entity(id).and_then(|entity| {
                let path = instance_prefab(world, entity)?;
                save_instance(world, entity, &asset_path(&path)?)?;
                world.resource::<AssetServer>().reload(path.clone());
                Ok(path)
            });
            match &result {
                Ok(path) => info!("Saved {path}"),
                Err(error) => error!("Could not apply the prefab: {error}"),
            }
            send_report(&Report::PrefabSaved {
                path: result.clone().ok(),
                error: result.clone().err(),
            });
            return result.map(drop);
        }
        Command::Pointer(event) => {
            // A panel the game draws lies over the view at the pointer.
            if panels::pointer(world, &event) {
                return Ok(());
            }
            if let Some(mut preview) = world.get_resource_mut::<ModelPreview>()
                && preview.showing
            {
                preview.events.push(event);
                return Ok(());
            }
            match world.get_resource_mut::<SceneView>() {
                Some(mut view) if !view.stopped => view.events.push(event),
                _ => input::pointer(world, event),
            }
            Ok(())
        }
        Command::Key {
            code,
            pressed,
            text,
        } => input::key(world, &code, pressed, text),
        Command::FocusLost => {
            input::focus_lost(world);
            Ok(())
        }
        Command::Tool { tool } => {
            if let Some(mut view) = world.get_resource_mut::<SceneView>() {
                view.tool = tool;
            }
            Ok(())
        }
        Command::Snap { grid, .. } if !(grid > 0.0 && grid.is_finite()) => {
            Err(format!("a grid {grid} m apart is no grid"))
        }
        Command::Snap { grid, on } => {
            if let Some(mut view) = world.get_resource_mut::<SceneView>() {
                view.snap = scene_view::Snap { grid, on };
            }
            Ok(())
        }
        // Over a panel there is no scene to show the ghost on.
        Command::Ghost { path, pointer } => {
            let path = path.filter(|_| !panels::covers(world, pointer.into()));
            ground::ghost(world, path.as_deref(), pointer.into())
        }
        // Frames the previewed model, or the selection in the scene view.
        Command::Focus => {
            if let Some(mut preview) = world.get_resource_mut::<ModelPreview>()
                && preview.showing
            {
                preview.fit = true;
            } else if let Some(mut view) = world.get_resource_mut::<SceneView>() {
                view.focus = true;
            }
            Ok(())
        }
        Command::Fly {
            right,
            up,
            forward,
            fast,
        } => {
            // The preview takes the button's release, which would stop it.
            if world
                .get_resource::<ModelPreview>()
                .is_some_and(|preview| preview.showing)
            {
                return Ok(());
            }
            if let Some(mut view) = world.get_resource_mut::<SceneView>() {
                view.fly = Vec3::new(right, up, forward);
                view.fast = fast;
            }
            Ok(())
        }
        Command::Language { korean } => {
            besfa_ui::i18n::set_korean(korean);
            Ok(())
        }
        Command::Viewport { name, scale } => {
            panels::rescale(world, scale);
            viewport::reopen(world, &name)
        }
        Command::Preview { path } => {
            if path.is_some()
                && !world
                    .get_resource::<SceneView>()
                    .is_some_and(|view| !view.stopped)
            {
                Err("models are previewed in edit mode".into())
            } else {
                preview::show(world, path)
            }
        }
        Command::PreviewAnimation {
            clip,
            time,
            speed,
            looping,
        } => {
            preview::animate_with(world, clip, time, speed, looping);
            Ok(())
        }
        Command::PreviewParts { hidden, solo } => preview::parts(world, &hidden, solo),
        Command::PreviewLight { light } => {
            preview::light(world, light);
            Ok(())
        }
        Command::PreviewView { view } => {
            preview::look(world, view);
            Ok(())
        }
        Command::PreviewOverlays(overlays) => {
            preview::overlays(world, overlays);
            Ok(())
        }
        Command::PreviewSelect { id } => preview::select(world, id),
        Command::Thumbnail { path, file } => {
            if !(inside(&path) && (path.ends_with(".glb") || path.ends_with(".gltf"))) {
                Err(format!(
                    "{path} is not a glTF path inside the asset directory"
                ))
            } else if !(inside(&file) && file.ends_with(".png")) {
                Err(format!("{file} is not a .png path inside the project"))
            } else {
                thumbnail::request(world, path, project_file(&file));
                Ok(())
            }
        }
        Command::Reimport { path } => {
            if inside(&path) && (path.ends_with(".glb") || path.ends_with(".gltf")) {
                reimport::reimport(world, path);
                Ok(())
            } else {
                Err(format!(
                    "{path} is not a glTF path inside the asset directory"
                ))
            }
        }
        Command::Screenshot {
            file,
            target,
            eye,
            top,
            width,
            height,
            markers,
            game,
        } => {
            if !(inside(&file) && file.ends_with(".png")) {
                Err(format!("{file} is not a .png path inside the project"))
            } else {
                let view = if game == Some(true) {
                    Shot::game(width, height)
                } else {
                    Shot::new(
                        target.into(),
                        eye.map(Vec3::from),
                        top,
                        width,
                        height,
                        markers.unwrap_or(false),
                    )
                };
                view.map(|view| shot::request(world, file.clone(), project_file(&file), view))
            }
        }
        Command::PreviewScreenshot { path } => {
            if inside(&path) && path.ends_with(".png") {
                preview::screenshot(world, path.clone(), project_file(&path))
            } else {
                Err(format!("{path} is not a .png path inside the project"))
            }
        }
    };
    if let Err(error) = &result {
        error!("Could not apply the editor's change: {error}");
    }
    result
}

/// Tells the editor about an entity it asked for, then selects it.
fn spawned(world: &mut World, entity: Entity) {
    send_report(&Report::Spawned {
        id: entity.to_bits(),
    });
    select(world, Some(entity));
}

/// Puts the scene entity `make` spawns where `place` says, then reports
/// and selects it.
fn spawn_placed(
    world: &mut World,
    place: &Placement,
    make: impl FnOnce(&mut World) -> Result<Entity, String>,
) -> Result<(), String> {
    // Dropped on a panel the game draws: nothing is placed.
    if place.translation.is_none()
        && place
            .pointer
            .is_some_and(|pointer| panels::covers(world, pointer.into()))
    {
        return Ok(());
    }
    let parent = place.parent.map(|id| edit::parent(world, id)).transpose()?;
    let entity = make(world)?;
    if let (None, Some(pointer)) = (place.translation, place.pointer) {
        ground::place_at(world, entity, pointer.into());
    }
    edit::place(world, entity, place, parent);
    spawned(world, entity);
    Ok(())
}

/// Whether `path` is relative and stays inside the folder it is joined to.
fn inside(path: &str) -> bool {
    !path.starts_with('/')
        && !path.contains(':')
        && path
            .split(['/', '\\'])
            .all(|part| !part.is_empty() && part != "..")
}

/// A file under the asset directory, named the way asset paths are.
fn asset_path(path: &str) -> Result<std::path::PathBuf, String> {
    if inside(path) && path.ends_with(".scn.ron") {
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
