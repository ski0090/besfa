//! Changes to the parts a placed glTF model spawns, kept with the scene.
//!
//! The parts are not saved: the model spawns them anew from its file. So
//! when the editor sets, adds or removes a component of a part, the change
//! is noted in [`PartChanges`] on the placed root, which the scene file
//! keeps, and made again whenever the model spawns: as the scene loads,
//! and after the file is reimported.

use bevy::{
    prelude::*,
    reflect::serde::TypedReflectSerializer,
    world_serialization::{WorldAssetRoot, WorldInstanceReady},
};
use serde_json::Value;

use crate::{
    edit::{self, subtree},
    scene::{HANDLE_PATHS, SceneEntity},
    scene_view::EditorOnly,
};

/// Changes made in the editor to the parts a placed glTF model spawns.
/// Removing it spawns the parts again as the file has them.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct PartChanges {
    pub parts: Vec<PartChange>,
}

/// The latest change to one component of one part.
#[derive(Reflect, Default, Clone, Debug, PartialEq)]
pub struct PartChange {
    /// The part's names from the model's root down to it, joined by `/`.
    pub part: String,
    /// The component's type path.
    pub component: String,
    /// The component's whole value as JSON, the way the editor sends it;
    /// none once it is removed.
    pub value: Option<String>,
}

pub(crate) struct PartChangesPlugin;

impl Plugin for PartChangesPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<PartChanges>()
            .add_observer(spawned)
            .add_observer(cleared);
    }
}

/// The placed glTF model `entity` is a part of, and its path in it.
fn placement(world: &World, entity: Entity) -> Option<(Entity, String)> {
    if world.get::<SceneEntity>(entity).is_some() {
        return None;
    }
    let mut names = Vec::new();
    let mut current = entity;
    loop {
        names.push(label(world, current));
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

/// A part's name, or the way Bevy prints an unnamed entity.
fn label(world: &World, entity: Entity) -> String {
    world
        .get::<Name>(entity)
        .map_or_else(|| entity.to_string(), |name| name.as_str().to_owned())
}

/// Notes on its model's root that the editor changed `component` of
/// `entity`, if it is a part of a placed glTF model.
fn record(world: &mut World, entity: Entity, component: &str) {
    let Some((root, part)) = placement(world, entity) else {
        return;
    };
    let value = component_json(world, entity, component).map(|value| value.to_string());
    let change = PartChange {
        part,
        component: component.to_owned(),
        value,
    };
    let mut root = world.entity_mut(root);
    match root.get_mut::<PartChanges>() {
        Some(mut changes) => {
            let parts = &mut changes.parts;
            match parts
                .iter_mut()
                .find(|old| old.part == change.part && old.component == change.component)
            {
                Some(old) => *old = change,
                None => parts.push(change),
            }
        }
        None => {
            root.insert(PartChanges {
                parts: vec![change],
            });
        }
    }
}

/// `component` of `entity` as JSON, with handles as their paths.
fn component_json(world: &World, entity: Entity, component: &str) -> Option<Value> {
    let registry = world.resource::<AppTypeRegistry>().read();
    let registration = registry.get_with_type_path(component)?;
    let value = registration
        .data::<ReflectComponent>()?
        .reflect(world.entity(entity))?;
    serde_json::to_value(TypedReflectSerializer::with_processor(
        value.as_partial_reflect(),
        &registry,
        &HANDLE_PATHS,
    ))
    .ok()
}

/// Makes the changes again on the parts of `root` there are now.
fn apply(world: &mut World, root: Entity) {
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
                .and_then(|value: Value| edit::set(world, part, &change.component, &value)),
            None => edit::remove(world, part, &change.component),
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

/// After the editor changed `component` of `entity`: a part's change is
/// noted on its model's root, and changes set on a root, as an undo does,
/// are made at once.
pub(crate) fn edited(world: &mut World, entity: Entity, component: &str) {
    if component == PartChanges::type_path() {
        apply(world, entity);
    } else {
        record(world, entity, component);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_part_changes_on_the_root_and_makes_them_again() {
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        {
            let registry = world.resource::<AppTypeRegistry>();
            let mut registry = registry.write();
            registry.register::<Transform>();
            registry.register::<PartChanges>();
        }
        let root = world
            .spawn((SceneEntity, WorldAssetRoot(Handle::default())))
            .id();
        let scene = world.spawn((Name::new("Scene"), ChildOf(root))).id();
        let lid = world
            .spawn((Name::new("Lid"), Transform::default(), ChildOf(scene)))
            .id();
        let open = Transform::from_xyz(0.0, 1.0, 0.0);
        let transform = Transform::type_path();

        *world.get_mut::<Transform>(lid).unwrap() = open;
        edited(&mut world, lid, transform);
        let changes = world.get::<PartChanges>(root).unwrap();
        assert_eq!(changes.parts.len(), 1);
        assert_eq!(changes.parts[0].part, "Scene/Lid");

        // Spawned anew, the way the file has it.
        *world.get_mut::<Transform>(lid).unwrap() = Transform::default();
        apply(&mut world, root);
        assert_eq!(*world.get::<Transform>(lid).unwrap(), open);

        // The root itself, and the preview's model, keep nothing.
        edited(&mut world, root, transform);
        world
            .entity_mut(root)
            .insert(EditorOnly)
            .remove::<SceneEntity>();
        world.entity_mut(root).remove::<PartChanges>();
        edited(&mut world, lid, transform);
        assert!(world.get::<PartChanges>(root).is_none());
    }
}
