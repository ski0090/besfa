//! Noting the editor's changes to the parts a placed glTF model spawns;
//! `besfa_components::part_changes` makes them again.
//!
//! The parts are not saved: the model spawns them anew from its file. So
//! when the editor sets, adds or removes a component of a part, the change
//! is noted in [`PartChanges`] on the placed root, which the scene file
//! keeps, and made again whenever the model spawns: as the scene loads,
//! and after the file is reimported.

use bevy::prelude::*;
use serde_json::Value;

pub use besfa_components::part_changes::*;

use crate::scene::json;

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
    let mut changes = root.entry::<PartChanges>().or_default();
    let parts = &mut changes.get_mut().parts;
    match parts
        .iter_mut()
        .find(|old| old.part == change.part && old.component == change.component)
    {
        Some(old) => *old = change,
        None => parts.push(change),
    }
}

/// `component` of `entity` as JSON, with handles as their paths.
fn component_json(world: &World, entity: Entity, component: &str) -> Option<Value> {
    let registry = world.resource::<AppTypeRegistry>().read();
    let registration = registry.get_with_type_path(component)?;
    let value = registration
        .data::<ReflectComponent>()?
        .reflect(world.entity(entity))?;
    json(value.as_partial_reflect(), &registry).ok()
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
    use bevy::world_serialization::WorldAssetRoot;

    use super::*;
    use crate::{EditorOnly, scene::SceneEntity};

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
