//! Saving the world a game is played in, apart from its scene file: what
//! playing built or changed, so closing the game and opening it again keeps
//! it. Mark what to keep with [`Persist`] and name the components to keep
//! in [`SaveFilter`]; [`save_world`] writes them in Bevy's RON world format,
//! keeping the save before beside it, and [`load_world`] puts them back.
//! An entity from the scene file is found again by its id there and has its
//! saved components put back on it; the others are spawned anew, after the
//! ones already there are taken away.

use std::{
    any::TypeId,
    fs,
    path::{Path, PathBuf},
};

use bevy::{
    ecs::entity::EntityHashMap,
    platform::collections::{HashMap, HashSet},
    prelude::*,
    reflect::TypePath,
    world_serialization::{DynamicWorldBuilder, WorldFilter, serde::WorldDeserializer},
};
use serde::de::DeserializeSeed;

use crate::scene::FileEntity;

/// Kept by a world save, with the components [`SaveFilter`] names.
#[derive(Component, Reflect, Clone, Copy, Debug, Default)]
#[type_path = "besfa_editor_plugin::save"]
#[reflect(Component, Default)]
pub struct Persist;

/// A saved entity's id in the scene file, which loading finds it by; only
/// in save files.
#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq)]
#[type_path = "besfa_editor_plugin::save"]
#[reflect(Component, Default)]
pub struct SceneId(pub u64);

/// The components a world save keeps of each [`Persist`] entity.
#[derive(Resource, Clone, Debug, Default)]
pub struct SaveFilter(HashSet<TypeId>);

impl SaveFilter {
    /// Keeps `T` of the entities that have it.
    pub fn keep<T: Component>(&mut self) -> &mut Self {
        self.0.insert(TypeId::of::<T>());
        self
    }
}

/// The save before the last one, kept beside it.
pub fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("bak")
}

/// Writes the [`Persist`] entities and their kept components to `path`,
/// moving the save already there to [`backup_path`]. The file is written
/// whole beside it first, so stopping halfway leaves the last save as it
/// was.
pub fn save_world(world: &mut World, path: &Path) -> Result<(), String> {
    let from_scene: Vec<(Entity, Entity)> = world
        .query_filtered::<(Entity, &FileEntity), With<Persist>>()
        .iter(world)
        .map(|(entity, file)| (entity, file.0))
        .collect();
    for &(entity, file) in &from_scene {
        world.entity_mut(entity).insert(SceneId(file.to_bits()));
    }
    let entities: Vec<Entity> = world
        .query_filtered::<Entity, With<Persist>>()
        .iter(world)
        .collect();
    let mut allowed = world
        .get_resource::<SaveFilter>()
        .cloned()
        .unwrap_or_default()
        .0;
    allowed.insert(TypeId::of::<Persist>());
    allowed.insert(TypeId::of::<SceneId>());
    let registry = world.resource::<AppTypeRegistry>().clone();
    let written = {
        let registry = registry.read();
        DynamicWorldBuilder::from_world(world, &registry)
            .with_component_filter(WorldFilter::Allowlist(allowed))
            .deny_all_resources()
            .extract_entities(entities.into_iter())
            .build()
            .serialize(&registry)
            .map_err(|error| error.to_string())
    };
    for &(entity, _) in &from_scene {
        world.entity_mut(entity).remove::<SceneId>();
    }
    let written = written?;
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    let fresh = path.with_extension("tmp");
    fs::write(&fresh, written).map_err(|error| error.to_string())?;
    // Copied, not moved: the save stays whole until the new one replaces
    // it at once, so a game cut off at any point leaves one to open.
    if path.exists() {
        fs::copy(path, backup_path(path)).map_err(|error| error.to_string())?;
    }
    fs::rename(&fresh, path).map_err(|error| error.to_string())
}

/// Puts back what `path` keeps: scene entities found by their id get their
/// saved components, the rest are spawned after the [`Persist`] entities
/// not from the scene are taken away.
pub fn load_world(world: &mut World, path: &Path) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let registry = world.resource::<AppTypeRegistry>().clone();
    let assets = world.resource::<AssetServer>().clone();
    let saved = {
        let registry = registry.read();
        let mut loader = &assets;
        let mut deserializer =
            ron::de::Deserializer::from_str(&text).map_err(|error| error.to_string())?;
        WorldDeserializer {
            type_registry: &registry,
            load_from_path: &mut loader,
        }
        .deserialize(&mut deserializer)
        .map_err(|error| error.to_string())?
    };
    let played: Vec<Entity> = world
        .query_filtered::<Entity, (With<Persist>, Without<FileEntity>)>()
        .iter(world)
        .collect();
    for entity in played {
        world.despawn(entity);
    }
    let from_scene: HashMap<u64, Entity> = world
        .query::<(Entity, &FileEntity)>()
        .iter(world)
        .map(|(entity, file)| (file.0.to_bits(), entity))
        .collect();
    let mut map = EntityHashMap::default();
    for saved_entity in &saved.entities {
        let id = saved_entity.components.iter().find_map(|component| {
            (component.reflect_type_path() == SceneId::type_path())
                .then(|| SceneId::from_reflect(component.as_partial_reflect()))
                .flatten()
        });
        if let Some(target) = id.and_then(|SceneId(id)| from_scene.get(&id)) {
            map.insert(saved_entity.entity, *target);
        }
    }
    saved
        .write_to_world(world, &mut map)
        .map_err(|error| error.to_string())?;
    let marked: Vec<Entity> = world
        .query_filtered::<Entity, With<SceneId>>()
        .iter(world)
        .collect();
    for entity in marked {
        world.entity_mut(entity).remove::<SceneId>();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq)]
    #[reflect(Component, Default)]
    struct Day(u32);

    #[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq)]
    #[reflect(Component, Default)]
    struct Wall(f32);

    /// Not kept: not in the filter.
    #[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq)]
    #[reflect(Component, Default)]
    struct Glow;

    #[test]
    fn keeps_what_play_built_and_changed_and_the_save_before() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        let mut filter = SaveFilter::default();
        filter.keep::<Day>().keep::<Wall>().keep::<Transform>();
        app.insert_resource(filter);
        let world = app.world_mut();
        // The scene's clock, by its file id, and a wall built in play.
        let clock = world
            .spawn((Persist, FileEntity(Entity::from_raw_u32(42).unwrap()), Day(3)))
            .id();
        world.spawn((Persist, Wall(150.0), Transform::from_xyz(1.0, 0.0, 2.0), Glow));
        let folder = std::env::temp_dir().join(format!("besfa-save-{}", std::process::id()));
        let path = folder.join("world.scn.ron");
        save_world(world, &path).unwrap();
        save_world(world, &path).unwrap();
        assert!(backup_path(&path).exists(), "the save before stays");
        assert!(world.get::<SceneId>(clock).is_none());

        // Play goes on: the day changes, the wall falls, another goes up.
        world.entity_mut(clock).insert(Day(5));
        let walls: Vec<Entity> = world.query_filtered::<Entity, With<Wall>>().iter(world).collect();
        world.despawn(walls[0]);
        world.spawn((Persist, Wall(10.0)));

        // A save cut off while writing leaves its half behind; the save
        // before it is still whole.
        fs::write(path.with_extension("tmp"), "(entities: {").unwrap();
        load_world(world, &path).unwrap();
        assert_eq!(world.get::<Day>(clock), Some(&Day(3)));
        let walls: Vec<(Wall, Transform, bool)> = world
            .query::<(&Wall, &Transform, Has<Glow>)>()
            .iter(world)
            .map(|(wall, at, glow)| (*wall, *at, glow))
            .collect();
        assert_eq!(walls, [(Wall(150.0), Transform::from_xyz(1.0, 0.0, 2.0), false)]);
        assert_eq!(world.query::<&SceneId>().iter(world).count(), 0);
        fs::remove_dir_all(folder).unwrap();
    }
}
