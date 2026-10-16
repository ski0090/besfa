//! Changes to the parts a placed glTF model spawns, kept with the scene.
//!
//! The parts are not saved: the model spawns them anew from its file. So
//! when the editor sets, adds or removes a component of a part, the change
//! is noted in [`PartChanges`] on the placed root, which the scene file
//! keeps, and made again whenever the model spawns: as the scene loads,
//! and after the file is reimported.

use bevy::{
    prelude::*,
    world_serialization::{WorldAssetRoot, WorldInstanceReady},
};
use serde_json::Value;

use crate::{
    EditorOnly,
    reflect::{self, subtree},
    scene::SceneEntity,
};

/// Changes made in the editor to the parts a placed glTF model spawns.
/// Removing it spawns the parts again as the file has them.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
#[type_path = "besfa_editor_plugin::part_changes"]
#[reflect(Component, Default)]
pub struct PartChanges {
    pub parts: Vec<PartChange>,
}

/// The latest change to one component of one part.
#[derive(Reflect, Default, Clone, Debug, PartialEq)]
#[type_path = "besfa_editor_plugin::part_changes"]
pub struct PartChange {
    /// The part's names from the model's root down to it, joined by `/`.
    pub part: String,
    /// The component's type path.
    pub component: String,
    /// The component's whole value as JSON, the way the editor sends it;
    /// none once it is removed.
    pub value: Option<String>,
}

pub struct PartChangesPlugin;

impl Plugin for PartChangesPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(spawned).add_observer(cleared);
    }
}

/// The placed glTF model `entity` is a part of, and its path in it.
#[doc(hidden)]
pub fn placement(world: &World, entity: Entity) -> Option<(Entity, String)> {
    if world.get::<SceneEntity>(entity).is_some() {
        return None;
    }
    let mut names = Vec::new();
    let mut current = entity;
    loop {
        // The part's name, or the way Bevy prints an unnamed entity.
        names.push(match world.get::<Name>(current) {
            Some(name) => name.as_str().to_owned(),
            None => current.to_string(),
        });
        current = world.get::<ChildOf>(current)?.parent();
        if world.get::<WorldAssetRoot>(current).is_some() {
            // The preview's model keeps its changes in the editor.
            if world.get::<EditorOnly>(current).is_some() {
                return None;
            }
            names.reverse();
            return Some((current, names.join("/")));
        }
    }
}

/// Makes the changes again on the parts of `root` there are now.
#[doc(hidden)]
pub fn apply(world: &mut World, root: Entity) {
    let Some(changes) = world.get::<PartChanges>(root).cloned() else {
        return;
    };
    let parts: Vec<(Entity, String)> = subtree(world, root)[1..]
        .iter()
        .filter_map(|&part| Some((part, placement(world, part)?.1)))
        .collect();
    for change in changes.parts {
        let Some(&(part, _)) = parts.iter().find(|(_, path)| *path == change.part) else {
            continue;
        };
        let result = match &change.value {
            Some(value) => serde_json::from_str(value)
                .map_err(|error| error.to_string())
                .and_then(|value: Value| reflect::set(world, part, &change.component, &value)),
            None => reflect::remove(world, part, &change.component),
        };
        if let Err(error) = result {
            warn!(
                "Could not make the saved change to {} of {}: {error}",
                change.component, change.part
            );
        }
    }
}

/// The model spawned its parts: as the scene loaded, or anew.
fn spawned(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    roots: Query<(), With<PartChanges>>,
) {
    let root = ready.entity;
    if roots.contains(root) {
        commands.queue(move |world: &mut World| apply(world, root));
    }
}

/// The changes were taken back: the parts spawn again as the file has them.
fn cleared(remove: On<Remove<PartChanges>>, mut commands: Commands, roots: Query<&WorldAssetRoot>) {
    if let Ok(scene) = roots.get(remove.entity) {
        let scene = scene.clone();
        commands.entity(remove.entity).try_insert(scene);
    }
}
