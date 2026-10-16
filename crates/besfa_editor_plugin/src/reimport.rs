//! Loads a model file again when the editor says it changed on disk, and
//! once every model showing it has spawned it anew, tells the game with
//! [`Reimported`] on each and the editor with a `reimported` report.
//!
//! Bevy respawns a placed model when its scene asset changes, unless the
//! change comes within a few frames of the last one: its spawner drops it
//! then, and the model keeps showing the file as it was. So once the file
//! has loaded and stayed loaded for a few frames, every model showing it is
//! respawned here, from the file as it is now.

use std::collections::HashSet;

use besfa_protocol::Report;
use bevy::{
    asset::{UntypedAssetId, UntypedAssetLoadFailedEvent},
    gltf::Gltf,
    prelude::*,
    world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady},
};

pub use besfa_components::Reimported;

use crate::inspect::send_report;

/// Frames the file must stay loaded before the models respawn: more than
/// the frames Bevy's spawner drops changes for.
const SETTLE_FRAMES: u32 = 5;

/// Frames to wait for the respawned models before giving up on them.
const GIVE_UP_FRAMES: u32 = 600;

pub(crate) struct ReimportPlugin;

impl Plugin for ReimportPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Reloads>()
            .add_observer(respawned)
            .add_systems(Last, follow);
    }
}

#[derive(Resource, Default)]
struct Reloads(Vec<Reload>);

struct Reload {
    /// Relative to the asset directory.
    path: String,
    /// The roots showing the file when it was asked for.
    roots: Vec<Entity>,
    stage: Stage,
}

enum Stage {
    /// Frames the file has been loading.
    Loading(u32),
    /// Frames since the file last loaded.
    Settling(u32),
    /// The roots were respawned this many frames ago; those that spawned
    /// anew since.
    Respawning(u32, HashSet<Entity>),
}

/// The asset a `Modified` event is about.
fn modified_id<A: Asset>(event: &AssetEvent<A>) -> Option<UntypedAssetId> {
    match event {
        AssetEvent::Modified { id } => Some(id.untyped()),
        _ => None,
    }
}

/// The file an asset comes from, without the scene's label.
fn file_of(path: &bevy::asset::AssetPath) -> String {
    path.without_label()
        .path()
        .to_string_lossy()
        .replace('\\', "/")
}

/// Loads the model at `path` again, if anything shows it.
pub(crate) fn reimport(world: &mut World, path: String) {
    let roots: Vec<Entity> = world
        .query::<(Entity, &WorldAssetRoot)>()
        .iter(world)
        .filter(|(_, root)| root.0.path().map(file_of).as_deref() == Some(path.as_str()))
        .map(|(entity, _)| entity)
        .collect();
    if roots.is_empty() {
        // Nothing shows it; it is read fresh when something does.
        send_report(&Report::Reimported { path, error: None });
        return;
    }
    world.resource::<AssetServer>().reload(path.clone());
    let mut reloads = world.resource_mut::<Reloads>();
    reloads.0.retain(|reload| reload.path != path);
    reloads.0.push(Reload {
        path,
        roots,
        stage: Stage::Loading(0),
    });
}

fn respawned(ready: On<WorldInstanceReady>, mut reloads: ResMut<Reloads>) {
    for reload in &mut reloads.0 {
        if let Stage::Respawning(_, spawned) = &mut reload.stage
            && reload.roots.contains(&ready.entity)
        {
            spawned.insert(ready.entity);
        }
    }
}

/// Follows each reload: the file loading or failing, the file staying
/// loaded, the models respawning; and reports the outcome.
fn follow(
    mut commands: Commands,
    mut reloads: ResMut<Reloads>,
    assets: Res<AssetServer>,
    mut scenes: MessageReader<AssetEvent<WorldAsset>>,
    mut gltfs: MessageReader<AssetEvent<Gltf>>,
    mut failed: MessageReader<UntypedAssetLoadFailedEvent>,
    roots: Query<&WorldAssetRoot>,
) {
    let modified = |id: UntypedAssetId| assets.get_path(id).map(|path| file_of(&path));
    let loaded: Vec<String> = scenes
        .read()
        .filter_map(modified_id)
        .chain(gltfs.read().filter_map(modified_id))
        .filter_map(modified)
        .collect();
    let failures: Vec<(String, String)> = failed
        .read()
        .map(|failure| {
            let error = failure.error.to_string().trim_end().to_owned();
            (file_of(&failure.path), error)
        })
        .collect();

    reloads.0.retain_mut(|reload| {
        if let Some((_, error)) = failures.iter().find(|(path, _)| *path == reload.path) {
            error!("Could not reimport {}: {error}", reload.path);
            send_report(&Report::Reimported {
                path: reload.path.clone(),
                error: Some(error.clone()),
            });
            return false;
        }
        // Loaded, or loaded again by a later reload: settle from now.
        if loaded.contains(&reload.path) {
            reload.stage = Stage::Settling(0);
        }
        reload.roots.retain(|&root| roots.contains(root));
        let given_up = |what: &str| {
            send_report(&Report::Reimported {
                path: reload.path.clone(),
                error: Some(format!("the file {what} in {GIVE_UP_FRAMES} frames")),
            });
            false
        };
        match &mut reload.stage {
            Stage::Loading(frames) if *frames > GIVE_UP_FRAMES => given_up("did not load"),
            Stage::Loading(frames) => {
                *frames += 1;
                true
            }
            Stage::Settling(frames) if *frames < SETTLE_FRAMES => {
                *frames += 1;
                true
            }
            Stage::Settling(_) => {
                for &root in &reload.roots {
                    if let Ok(scene) = roots.get(root) {
                        // A changed root spawns what it shows anew.
                        commands.entity(root).insert(scene.clone());
                    }
                }
                reload.stage = Stage::Respawning(0, HashSet::new());
                true
            }
            Stage::Respawning(frames, spawned) => {
                *frames += 1;
                if reload.roots.iter().all(|root| spawned.contains(root)) {
                    for &root in &reload.roots {
                        commands.trigger(Reimported { entity: root });
                    }
                    info!("Reimported {}", reload.path);
                    send_report(&Report::Reimported {
                        path: reload.path.clone(),
                        error: None,
                    });
                    return false;
                }
                if *frames > GIVE_UP_FRAMES {
                    return given_up("was not spawned again");
                }
                true
            }
        }
    });
}
