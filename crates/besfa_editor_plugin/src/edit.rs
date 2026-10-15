//! Changes the scene on the editor's commands: component values, added and
//! removed components, and spawned, duplicated, deleted and restored
//! entities. Deleting hides an entity instead of despawning it, so undoing
//! brings back the same entity and the editor's history keeps its ids. The
//! editor despawns it once no undo can bring it back.
//!
//! Components are named by their reflected type path, the same path the
//! scene file and the `entity` report use. Values are JSON in the shape
//! the `entity` report sends them.

use besfa_protocol::{Placement, SpawnKind};
use bevy::{
    asset::HandleDeserializeProcessor,
    ecs::{entity_disabling::Disabled, reflect::ReflectFromWorld},
    prelude::*,
    reflect::{TypeRegistration, TypeRegistry, serde::TypedReflectDeserializer},
    world_serialization::{DynamicWorldRoot, WorldAssetRoot},
};
use serde::de::DeserializeSeed;
use serde_json::Value;

use crate::{
    scene::{MeshColor, MeshShape, SceneEntity},
    scene_view::keep_parked,
};

/// An entity the editor deleted: disabled, so the game, rendering and
/// saving skip it, until undoing restores it or the editor despawns it.
#[derive(Component)]
pub(crate) struct Deleted;

/// Sets `component` on `entity` to `value`, adding it if the entity has
/// none, which is how undoing a removal puts it back. The editor sends the
/// whole value: a field left out takes its default when the type has one.
pub(crate) fn set(
    world: &mut World,
    entity: Entity,
    component: &str,
    value: &Value,
) -> Result<(), String> {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let (registration, reflect) = component_registration(&registry, component)?;
    let immutable = world
        .components()
        .get_id(registration.type_id())
        .and_then(|id| world.components().get_info(id))
        .is_some_and(|info| !info.mutable());
    if immutable {
        return Err(format!("{component} cannot be changed"));
    }
    let value = match world.get_resource::<AssetServer>().cloned() {
        // Handles name their asset by path, the way the scene file does;
        // changing a prefab's path loads that prefab.
        Some(assets) => {
            let mut loader = &assets;
            let mut processor = HandleDeserializeProcessor {
                load_from_path: &mut loader,
            };
            TypedReflectDeserializer::with_processor(registration, &registry, &mut processor)
                .deserialize(value)
        }
        None => TypedReflectDeserializer::new(registration, &registry).deserialize(value),
    }
    .map_err(|error| error.to_string())?;
    let mut target = entity_mut(world, entity)?;
    let result = match reflect.reflect_mut(&mut target) {
        Some(mut component) => component
            .try_apply(value.as_ref())
            .map_err(|error| error.to_string()),
        None => {
            reflect.insert(&mut target, value.as_ref(), &registry);
            Ok(())
        }
    };
    keep_parked(world, entity);
    result
}

/// Adds `component` to `entity` with its default value.
pub(crate) fn insert(world: &mut World, entity: Entity, component: &str) -> Result<(), String> {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let (registration, reflect) = component_registration(&registry, component)?;
    let value = if let Some(default) = registration.data::<ReflectDefault>() {
        default.default()
    } else if let Some(from_world) = registration.data::<ReflectFromWorld>() {
        from_world.from_world(world)
    } else {
        return Err(format!("{component} has no default value"));
    };
    reflect.insert(
        &mut entity_mut(world, entity)?,
        value.as_partial_reflect(),
        &registry,
    );
    keep_parked(world, entity);
    Ok(())
}

pub(crate) fn remove(world: &mut World, entity: Entity, component: &str) -> Result<(), String> {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let (_, reflect) = component_registration(&registry, component)?;
    reflect.remove(&mut entity_mut(world, entity)?);
    Ok(())
}

/// Spawns a scene entity at the origin, so saving keeps it.
pub(crate) fn spawn(world: &mut World, kind: SpawnKind) -> Entity {
    let mut entity = world.spawn((SceneEntity, Transform::default()));
    match kind {
        SpawnKind::Empty => entity.insert(Name::new("Entity")),
        SpawnKind::Cube => entity.insert((
            Name::new("Cube"),
            MeshShape::default(),
            MeshColor::default(),
        )),
        SpawnKind::Sphere => entity.insert((
            Name::new("Sphere"),
            MeshShape::Sphere { radius: 0.5 },
            MeshColor::default(),
        )),
        SpawnKind::Plane => entity.insert((
            Name::new("Plane"),
            MeshShape::Plane {
                size: Vec2::splat(10.0),
            },
            MeshColor::default(),
        )),
        SpawnKind::Light => entity.insert((
            Name::new("Light"),
            PointLight {
                shadow_maps_enabled: true,
                ..default()
            },
            Transform::from_xyz(0.0, 4.0, 0.0),
        )),
        SpawnKind::Camera => entity.insert((
            Name::new("Camera"),
            Camera3d::default(),
            Transform::from_xyz(0.0, 2.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
        )),
    };
    entity.id()
}

/// Spawns a copy of `entity` with every component that can be cloned. The
/// copy is a sibling: children are not copied.
pub(crate) fn duplicate(world: &mut World, entity: Entity) -> Result<Entity, String> {
    let copy = entity_mut(world, entity)?.clone_and_spawn();
    world.entity_mut(copy).insert(SceneEntity);
    Ok(copy)
}

/// Hides `entity` and its children; returns them. Children the game
/// disabled itself stay as they are when restored.
pub(crate) fn delete(world: &mut World, entity: Entity) -> Result<Vec<Entity>, String> {
    entity_mut(world, entity)?;
    let subtree = subtree(world, entity);
    for &entity in &subtree {
        if world.get::<Disabled>(entity).is_none() {
            world.entity_mut(entity).insert((Disabled, Deleted));
        }
    }
    Ok(subtree)
}

/// Shows what [`delete`] hid again.
pub(crate) fn restore(world: &mut World, entity: Entity) -> Result<(), String> {
    entity_mut(world, entity)?;
    for entity in subtree(world, entity) {
        if world.get::<Deleted>(entity).is_some() {
            world.entity_mut(entity).remove::<(Disabled, Deleted)>();
        }
    }
    Ok(())
}

/// Despawns an entity [`delete`] hid, with its children, once the
/// editor's history can no longer restore it. A live entity is refused.
pub(crate) fn despawn(world: &mut World, entity: Entity) -> Result<(), String> {
    if !entity_mut(world, entity)?.contains::<Deleted>() {
        return Err(format!("entity {entity} is not deleted"));
    }
    world.despawn(entity);
    Ok(())
}

/// Spawns a scene entity at the origin that shows an asset, named after its
/// file: a glTF model's first scene, or a scene or prefab file. The asset's
/// entities become its children; saving keeps only the asset's path.
pub(crate) fn instantiate(world: &mut World, path: &str) -> Result<Entity, String> {
    let file = path.rsplit('/').next().unwrap_or(path);
    let name = file
        .strip_suffix(".scn.ron")
        .or_else(|| file.strip_suffix(".glb"))
        .or_else(|| file.strip_suffix(".gltf"))
        .ok_or_else(|| format!("{path} is not a glTF model or a scene file"))?
        .to_string();
    let assets = world.resource::<AssetServer>().clone();
    let mut entity = world.spawn((SceneEntity, Name::new(name), Transform::default()));
    if path.ends_with(".scn.ron") {
        entity.insert(DynamicWorldRoot(assets.load(path.to_string())));
    } else {
        entity.insert(WorldAssetRoot(assets.load(format!("{path}#Scene0"))));
    }
    Ok(entity.id())
}

/// Puts a new `entity` where `place` says. `parent` is the one `place`
/// names, already checked with [`parent`].
pub(crate) fn place(world: &mut World, entity: Entity, place: &Placement, parent: Option<Entity>) {
    let mut entity = world.entity_mut(entity);
    if let Some(name) = &place.name {
        entity.insert(Name::new(name.clone()));
    }
    if let Some(mut transform) = entity.get_mut::<Transform>() {
        if let Some(translation) = place.translation {
            transform.translation = translation.into();
        }
        if let Some(yaw) = place.yaw {
            transform.rotation = Quat::from_rotation_y(yaw.to_radians());
        }
        if let Some(scale) = place.scale {
            transform.scale = Vec3::splat(scale);
        }
    }
    let entity = entity.id();
    if let Some(parent) = parent {
        reparent(world, entity, Some(parent));
    }
}

/// The scene entity `id` names, for an entity to go under: not deleted,
/// and not a part of a placed model, which saving would leave out.
pub(crate) fn parent(world: &World, id: u64) -> Result<Entity, String> {
    Entity::try_from_bits(id)
        .filter(|&entity| {
            world.get_entity(entity).is_ok_and(|entity| {
                entity.contains::<SceneEntity>() && !entity.contains::<Deleted>()
            })
        })
        .ok_or_else(|| format!("{id} is not a scene entity that can be a parent"))
}

/// Moves a scene entity under `parent`, or to the top without one, where it
/// stays in the world.
pub(crate) fn set_parent(
    world: &mut World,
    entity: Entity,
    parent: Option<Entity>,
) -> Result<(), String> {
    if !entity_mut(world, entity)?.contains::<SceneEntity>() {
        return Err(format!("{} is not a scene entity", entity.to_bits()));
    }
    if let Some(parent) = parent
        && ancestry(world, parent).contains(&entity)
    {
        return Err(format!("{} cannot go under itself", entity.to_bits()));
    }
    reparent(world, entity, parent);
    Ok(())
}

fn reparent(world: &mut World, entity: Entity, parent: Option<Entity>) {
    // Composed from the transforms, since the global ones of what was made
    // or moved this frame are not up to date yet.
    let global = in_world(world, entity);
    let local = match parent {
        Some(parent) => relative(global, in_world(world, parent)),
        None => global,
    };
    let mut entity = world.entity_mut(entity);
    match parent {
        Some(parent) => entity.insert(ChildOf(parent)),
        None => entity.remove::<ChildOf>(),
    };
    if let Some(mut transform) = entity.get_mut::<Transform>() {
        *transform = local;
    }
}

/// `entity` and the entities above it, the entity first.
fn ancestry(world: &World, entity: Entity) -> Vec<Entity> {
    let mut entities = vec![entity];
    while let Some(parent) = world.get::<ChildOf>(*entities.last().unwrap()) {
        entities.push(parent.parent());
    }
    entities
}

/// Where `entity` is in the world, from its transform and its parents'.
/// Composed part by part rather than as matrices, so a group that only
/// moves its children leaves their rotation and size exactly as they were.
fn in_world(world: &World, entity: Entity) -> Transform {
    ancestry(world, entity)
        .iter()
        .rev()
        .fold(Transform::IDENTITY, |global, &entity| {
            global.mul_transform(world.get::<Transform>(entity).copied().unwrap_or_default())
        })
}

/// `child`, in the world, as seen from `parent`, in the world.
// ponytail: exact for parents scaled the same along each axis; a parent
// both turned and stretched unevenly would shear, which a Transform cannot
// hold, so its children come out close but not exact.
fn relative(child: Transform, parent: Transform) -> Transform {
    let turn = parent.rotation.inverse();
    Transform {
        translation: turn * (child.translation - parent.translation) / parent.scale,
        rotation: turn * child.rotation,
        scale: child.scale / parent.scale,
    }
}

/// `entity` and everything under it, the entity first.
pub(crate) fn subtree(world: &World, entity: Entity) -> Vec<Entity> {
    let mut entities = vec![entity];
    let mut next = 0;
    while let Some(&entity) = entities.get(next) {
        next += 1;
        if let Some(children) = world.get::<Children>(entity) {
            entities.extend(children.iter());
        }
    }
    entities
}

fn component_registration<'a>(
    registry: &'a TypeRegistry,
    path: &str,
) -> Result<(&'a TypeRegistration, &'a ReflectComponent), String> {
    let registration = registry
        .get_with_type_path(path)
        .ok_or_else(|| format!("{path} is not registered for reflection"))?;
    let reflect = registration
        .data::<ReflectComponent>()
        .ok_or_else(|| format!("{path} is not a reflected component"))?;
    Ok((registration, reflect))
}

fn entity_mut(world: &mut World, entity: Entity) -> Result<EntityWorldMut<'_>, String> {
    world
        .get_entity_mut(entity)
        .map_err(|_| format!("entity {entity} does not exist"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const TRANSFORM: &str = "bevy_transform::components::transform::Transform";
    const SHAPE: &str = "besfa_editor_plugin::scene::MeshShape";

    fn world() -> World {
        let mut app = App::new();
        app.register_type::<Name>()
            .register_type::<Transform>()
            .register_type::<MeshShape>()
            .register_type::<MeshColor>();
        std::mem::take(app.world_mut())
    }

    #[test]
    fn sets_whole_values() {
        let mut world = world();
        let cube = world
            .spawn((Name::new("Cube"), Transform::from_xyz(1.0, 2.0, 3.0)))
            .id();

        set(
            &mut world,
            cube,
            TRANSFORM,
            &json!({
                "translation": [1.0, 2.0, 3.0],
                "rotation": [0.0, 0.0, 0.0, 1.0],
                "scale": [2, 2, 2],
            }),
        )
        .unwrap();
        set(&mut world, cube, "bevy_ecs::name::Name", &json!("Box")).unwrap();

        let transform = world.get::<Transform>(cube).unwrap();
        assert_eq!(transform.scale, Vec3::splat(2.0));
        assert_eq!(transform.translation, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(world.get::<Name>(cube).unwrap().as_str(), "Box");

        assert!(
            set(&mut world, cube, TRANSFORM, &json!({ "scale": "big" })).is_err(),
            "a value of the wrong shape is refused"
        );
        assert!(set(&mut world, cube, "no::Such", &json!(1)).is_err());
    }

    #[test]
    fn inserts_a_default_and_removes_it() {
        let mut world = world();
        let entity = world.spawn_empty().id();

        insert(&mut world, entity, SHAPE).unwrap();
        assert_eq!(world.get::<MeshShape>(entity), Some(&MeshShape::default()));
        set(
            &mut world,
            entity,
            SHAPE,
            &json!({ "Sphere": { "radius": 2.0 } }),
        )
        .unwrap();
        assert_eq!(
            world.get::<MeshShape>(entity),
            Some(&MeshShape::Sphere { radius: 2.0 })
        );

        remove(&mut world, entity, SHAPE).unwrap();
        assert!(world.get::<MeshShape>(entity).is_none());
    }

    #[test]
    fn places_and_reparents_keeping_entities_where_they_are_in_the_world() {
        let mut world = world();
        let forest = spawn(&mut world, SpawnKind::Empty);
        world.get_mut::<Transform>(forest).unwrap().translation = Vec3::new(-55.0, 0.0, 0.0);
        let tree = spawn(&mut world, SpawnKind::Cube);
        let place_tree = Placement {
            translation: Some([-60.0, 0.0, 5.0]),
            yaw: Some(90.0),
            scale: Some(2.0),
            name: Some("Oak".into()),
            parent: Some(forest.to_bits()),
        };
        let parent_of_tree = parent(&world, forest.to_bits()).unwrap();
        place(&mut world, tree, &place_tree, Some(parent_of_tree));

        assert_eq!(world.get::<Name>(tree).unwrap().as_str(), "Oak");
        assert_eq!(
            world.get::<ChildOf>(tree).map(ChildOf::parent),
            Some(forest)
        );
        let local = world.get::<Transform>(tree).unwrap();
        assert!(
            local
                .translation
                .abs_diff_eq(Vec3::new(-5.0, 0.0, 5.0), 1e-4)
        );
        assert_eq!(
            local.scale,
            Vec3::splat(2.0),
            "exact under a group that only moves"
        );
        assert!(
            in_world(&world, tree)
                .forward()
                .abs_diff_eq(Vec3::NEG_X, 1e-4)
        );

        // Out again, back where it was in the world.
        set_parent(&mut world, tree, None).unwrap();
        assert!(world.get::<ChildOf>(tree).is_none());
        let local = world.get::<Transform>(tree).unwrap();
        assert!(
            local
                .translation
                .abs_diff_eq(Vec3::new(-60.0, 0.0, 5.0), 1e-4)
        );

        set_parent(&mut world, tree, Some(forest)).unwrap();
        assert!(
            set_parent(&mut world, forest, Some(tree)).is_err(),
            "under itself"
        );
        let part = world.spawn((Transform::default(), ChildOf(tree))).id();
        assert!(
            parent(&world, part.to_bits()).is_err(),
            "not a scene entity"
        );
        assert!(set_parent(&mut world, part, None).is_err());
        delete(&mut world, forest).unwrap();
        assert!(parent(&world, forest.to_bits()).is_err(), "deleted");
    }

    #[test]
    fn spawns_duplicates_deletes_and_restores_scene_entities() {
        let mut world = world();

        let cube = spawn(&mut world, SpawnKind::Cube);
        assert!(world.get::<SceneEntity>(cube).is_some());
        assert_eq!(world.get::<Name>(cube).unwrap().as_str(), "Cube");
        world.get_mut::<Transform>(cube).unwrap().translation.x = 5.0;

        let copy = duplicate(&mut world, cube).unwrap();
        assert_ne!(copy, cube);
        assert!(world.get::<SceneEntity>(copy).is_some());
        assert_eq!(world.get::<Transform>(copy).unwrap().translation.x, 5.0);
        assert_eq!(world.get::<MeshShape>(copy), Some(&MeshShape::default()));

        let child = world.spawn(ChildOf(cube)).id();
        let hidden_by_game = world.spawn((ChildOf(cube), Disabled)).id();
        let visible = |world: &mut World| {
            let mut query = world.query::<Entity>();
            query.iter(world).collect::<Vec<_>>()
        };
        assert_eq!(
            delete(&mut world, cube).unwrap(),
            [cube, child, hidden_by_game]
        );
        let seen = visible(&mut world);
        assert!(!seen.contains(&cube) && !seen.contains(&child), "hidden");
        assert!(world.get::<Name>(cube).is_some(), "but kept");

        restore(&mut world, cube).unwrap();
        let seen = visible(&mut world);
        assert!(seen.contains(&cube) && seen.contains(&child));
        assert!(world.get::<Disabled>(hidden_by_game).is_some());
        assert!(delete(&mut world, Entity::from_bits(u64::MAX >> 1)).is_err());

        assert!(despawn(&mut world, cube).is_err(), "a live entity stays");
        delete(&mut world, cube).unwrap();
        despawn(&mut world, cube).unwrap();
        assert!(world.get_entity(cube).is_err() && world.get_entity(child).is_err());
        assert!(world.get_entity(copy).is_ok());
    }

    #[test]
    fn setting_a_missing_component_adds_it() {
        let mut world = world();
        let entity = world.spawn_empty().id();

        set(
            &mut world,
            entity,
            TRANSFORM,
            &json!({ "translation": [1, 2, 3], "rotation": [0, 0, 0, 1], "scale": [1, 1, 1] }),
        )
        .unwrap();

        assert_eq!(
            world.get::<Transform>(entity),
            Some(&Transform::from_xyz(1.0, 2.0, 3.0))
        );
    }
}
