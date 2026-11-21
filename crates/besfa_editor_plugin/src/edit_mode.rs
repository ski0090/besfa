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

use besfa_components::{SwitchScene, TerrainData, scene::start_switch};
use besfa_protocol::{Command, Placement, Report, Request, SpawnKind, VERSION};
use bevy::{
    ecs::{message::Messages, resource::IsResource},
    input::InputSystems,
    prelude::*,
    time::{TimeSystems, TimeUpdateStrategy},
    world_serialization::DynamicWorld,
};

use crate::{
    edit, ground, input,
    inspect::{self, Changes, Selected, select, send_report},
    panels, part_changes,
    preview::{self, ModelPreview},
    reimport,
    scene::{
        asset_file, first_scene, instance_prefab, open_scene, project_file, save, save_instance,
        save_prefab,
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
            .add_message::<SwitchScene>()
            .insert_resource(HeldSwitches(Some(Vec::new())))
            .add_systems(PreStartup, pause)
            // Before time moves on: whether this frame is one of a step's.
            .add_systems(First, count_step.before(TimeSystems))
            // Before Bevy reads this frame's input, which the commands write.
            .add_systems(PreUpdate, run_commands.before(InputSystems))
            .add_systems(PreUpdate, hold_switches.before(start_switch));
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

/// A `step` playing its frames: those left to start, whether they have all
/// played, the request to answer then, and how time moved on before it,
/// which comes back then.
#[derive(Resource)]
struct Stepping {
    left: u32,
    over: bool,
    seq: Option<u64>,
    clock: TimeUpdateStrategy,
}

/// Plays `frames` frames from the next one on, each one fixed timestep
/// long, never the wall clock's, so a step plays the same every run.
fn step(world: &mut World, frames: u32) {
    let clock = world.remove_resource::<TimeUpdateStrategy>();
    world.insert_resource(TimeUpdateStrategy::FixedTimesteps(1));
    world.resource_mut::<Time<Virtual>>().unpause();
    world.insert_resource(Stepping {
        left: frames,
        over: false,
        seq: None,
        clock: clock.unwrap_or_default(),
    });
}

/// Lets the frame starting be one of the step's, or once they have all
/// played, pauses before it gets any time, so the step is answered after
/// the last one's `Update`.
fn count_step(
    stepping: Option<ResMut<Stepping>>,
    mut time: ResMut<Time<Virtual>>,
    mut clock: ResMut<TimeUpdateStrategy>,
) {
    let Some(mut stepping) = stepping.filter(|stepping| !stepping.over) else {
        return;
    };
    if stepping.left > 0 {
        stepping.left -= 1;
        return;
    }
    time.pause();
    *clock = std::mem::take(&mut stepping.clock);
    stepping.over = true;
}

/// The scenes the game asked to switch to before Play, which Play switches
/// to; `None` once playing. Until then the scene shown is the one the
/// editor has open, whose changes may not be saved, and Save writes it.
#[derive(Resource)]
struct HeldSwitches(Option<Vec<SwitchScene>>);

fn hold_switches(mut switches: ResMut<Messages<SwitchScene>>, mut held: ResMut<HeldSwitches>) {
    if let Some(held) = &mut held.0 {
        for switch in switches.drain() {
            info!("Switching to {} at Play.", switch.0);
            held.push(switch);
        }
    }
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
    world.insert_resource(HeldSwitches(Some(Vec::new())));
    world.insert_resource(SceneView::default());
    world.insert_resource(ModelPreview::default());
    world.insert_resource(terrain::Strokes::default());
    world.insert_resource(reimport::Reloads::default());
    world.insert_resource(shot::Shots::default());
    world.insert_resource(thumbnail::Thumbnails::default());
    world.remove_resource::<input::LastPointer>();
    input::set_cursor(world, None);
    panels::reset(world);
    world.insert_resource(Time::<Virtual>::default());

    reload_files(world);
    send_report(&Report::Hello { version: VERSION });
    send_report(&inspect::systems(world));
    send_report(&inspect::addable_components(world));
    world.run_schedule(PreStartup);
    world.run_schedule(Startup);
    world.run_schedule(PostStartup);
}

/// Asks for the scene the game starts in and the terrain files again, as
/// the last save left them, since play may have changed what is loaded.
/// The scene's old asset goes first: it would spawn at once, before the new
/// one arrives.
fn reload_files(world: &mut World) {
    let server = world.resource::<AssetServer>().clone();
    let first = first_scene();
    if let Some(scene) = server.get_handle::<DynamicWorld>(&first) {
        world
            .resource_mut::<Assets<DynamicWorld>>()
            .remove(scene.id());
    }
    server.reload(first);
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
    // A step's frames run before anything after it, and it is answered
    // once they did, after the last one's reports.
    if world
        .get_resource::<Stepping>()
        .is_some_and(|stepping| !stepping.over)
    {
        return;
    }
    if let Some(Stepping { seq: Some(seq), .. }) = world.remove_resource::<Stepping>() {
        send_report(&Report::Done { seq, error: None });
    }
    let mut pointed = false;
    while let Some(line) = waiting.pop_front() {
        match serde_json::from_str::<Request>(&line) {
            Ok(Request { seq, command }) => {
                if pointed && !command.is_input() {
                    waiting.push_front(line);
                    return;
                }
                // A press on the scene takes the keyboard from a field of
                // the panels the game draws, which sets what was typed in
                // it when it is next drawn. The press waits a frame for
                // that, with all that follows it: a handle it drags starts
                // from the typed value, and what it picks is picked after
                // the value is set.
                if let Command::Pointer(event) = &command
                    && panels::press_waits(world, event)
                {
                    waiting.push_front(line);
                    return;
                }
                pointed |= matches!(command, Command::Pointer(_));
                let result = run(world, command);
                if let Some(mut stepping) = world.get_resource_mut::<Stepping>() {
                    stepping.seq = seq;
                    return;
                }
                if let Some(seq) = seq {
                    send_report(&Report::Done {
                        seq,
                        error: result.err(),
                    });
                }
                // The editor's viewport lost the keyboard, and with it a
                // field of the panels, which sets what was typed in it when
                // it is next drawn. What follows waits a frame for that: a
                // save, so the file has the value.
                if panels::settling(world) {
                    return;
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
            play(world);
            world.resource_mut::<Time<Virtual>>().unpause();
            Ok(())
        }
        Command::Step { frames, seconds } => {
            let timestep = world.resource::<Time<Fixed>>().timestep().as_secs_f64();
            let frames = match (frames, seconds) {
                (Some(frames), None) => frames,
                (None, Some(seconds)) if seconds >= 0.0 && seconds.is_finite() => {
                    (f64::from(seconds) / timestep).round() as u32
                }
                _ => return Err("step takes frames or seconds of 0 or more".into()),
            };
            if world.resource::<HeldSwitches>().0.is_some() {
                play(world);
            }
            if frames > 0 {
                step(world, frames);
            }
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
        Command::Resources { path } => {
            send_report(&inspect::resources(world, path.as_deref()));
            Ok(())
        }
        Command::Save => {
            let scene = open_scene(world);
            let result = terrain::save(world).and_then(|()| save(world, &asset_file(&scene)));
            match &result {
                Ok(()) => info!("Saved {scene}"),
                Err(error) => error!("Could not save {scene}: {error}"),
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
        Command::Spawn { kind, path, place } => {
            let two_d = world.contains_resource::<crate::scene_view::TwoD>();
            match path {
                _ if !edit::shows(kind, two_d) => {
                    let game = if two_d { "2D" } else { "3D" };
                    Err(format!("a {game} game does not show {kind:?}"))
                }
                None => spawn_placed(world, &place, |world| Ok(edit::spawn(world, kind))),
                Some(path) if kind == SpawnKind::Sprite && path.ends_with(".png") => {
                    spawn_placed(world, &place, |world| edit::instantiate(world, &path))
                }
                Some(path) => Err(format!("a {kind:?} does not show {path}")),
            }
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
            panels::unfocus(world);
            Ok(())
        }
        Command::PanelInput(input) => panels::input(world, input),
        Command::PanelState { state } => panels::restore(world, &state),
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
        // Over a panel there is no scene to show the ghost on, and a 3D
        // game shows no sprite: refusing it would log every move.
        Command::Ghost { path, pointer } => {
            let two_d = world.contains_resource::<crate::scene_view::TwoD>();
            let path = path.filter(|path| {
                !panels::covers(world, pointer.into()) && (two_d || !path.ends_with(".png"))
            });
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
            // The preview takes the button's release, which would stop it,
            // and so does a panel the game draws while the press is its.
            if world
                .get_resource::<ModelPreview>()
                .is_some_and(|preview| preview.showing)
                || panels::holds(world)
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
                let two_d = world.contains_resource::<crate::scene_view::TwoD>();
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
                    .map(|shot| {
                        if two_d {
                            shot.facing_xy(target.into())
                        } else {
                            shot
                        }
                    })
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

/// Leaves edit mode for play, with time still paused: the game's cameras
/// take the view and the scenes it asked for come.
fn play(world: &mut World) {
    panels::play(world);
    if let Some(held) = world.resource_mut::<HeldSwitches>().0.take() {
        world.write_message_batch(held);
    }
    // First, so a camera inside the model is not handed back.
    preview::close(world);
    scene_view::stop(world);
    info!("Playing.");
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

    /// Frames in which virtual time moved, FixedUpdate runs, and the frames
    /// moved when the selection last changed.
    #[derive(Resource, Default)]
    struct Moved(u32, u32, u32);

    #[test]
    fn steps_exactly_the_frames_asked_from_edit_mode() {
        let (mut app, editor) = editor_app();
        app.init_resource::<Moved>()
            .add_systems(Update, |time: Res<Time>, mut moved: ResMut<Moved>| {
                if time.delta_secs() > 0.0 {
                    moved.0 += 1;
                }
            })
            .add_systems(FixedUpdate, |mut moved: ResMut<Moved>| moved.1 += 1)
            .add_systems(
                PreUpdate,
                (|mut moved: ResMut<Moved>| moved.2 = moved.0)
                    .after(run_commands)
                    .run_if(resource_changed::<Selected>),
            );
        app.update();
        let entity = app.world_mut().spawn_empty().id();
        editor
            .send(r#"{"seq":1,"command":"step","frames":3}"#.into())
            .unwrap();
        let select = format!(r#"{{"command":"select","id":{}}}"#, entity.to_bits());
        editor.send(select).unwrap();
        for _ in 0..3 {
            app.update();
            assert_eq!(app.world().resource::<Selected>().0, None, "waits");
        }
        // Real time passing in between changes nothing.
        std::thread::sleep(std::time::Duration::from_millis(50));
        for _ in 0..5 {
            app.update();
        }

        let moved = app.world().resource::<Moved>();
        assert_eq!((moved.0, moved.1), (3, 3), "three frames, one tick each");
        assert!(paused(&app));
        let elapsed = app.world().resource::<Time<Virtual>>().elapsed();
        assert_eq!(elapsed, Time::<Fixed>::default().timestep() * 3);
        assert!(
            app.world().resource::<HeldSwitches>().0.is_none(),
            "playing"
        );
        assert!(matches!(
            app.world().resource::<TimeUpdateStrategy>(),
            TimeUpdateStrategy::Automatic
        ));
        assert!(
            app.world().resource::<Selected>().0.is_some(),
            "then the rest"
        );
        let moved = app.world().resource::<Moved>();
        assert_eq!(moved.2, 3, "the rest sees the last frame's Update");

        // Seconds go by whole timesteps: 1/64 s each.
        editor
            .send(r#"{"command":"step","seconds":0.25}"#.into())
            .unwrap();
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(app.world().resource::<Moved>().0, 3 + 16);
    }

    #[test]
    fn holds_a_scene_switch_until_play() {
        let (mut app, editor) = editor_app();
        let tutorial = SwitchScene("scenes/tutorial.scn.ron".into());
        app.world_mut().write_message(tutorial.clone());
        // Longer than a message lasts.
        for _ in 0..3 {
            app.update();
        }
        let switches = app.world().resource::<Messages<SwitchScene>>();
        assert!(switches.is_empty(), "nothing to switch on before Play");

        editor.send(r#"{"command":"play"}"#.into()).unwrap();
        app.update();
        let mut switches = app.world_mut().resource_mut::<Messages<SwitchScene>>();
        assert_eq!(switches.drain().collect::<Vec<_>>(), [tutorial]);
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
